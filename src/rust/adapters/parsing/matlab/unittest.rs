//! Bounded MATLAB `matlab.unittest` frontend for the ADR-0046 admitted shape.
//!
//! Nothing here invokes MATLAB or Octave, opens a project, or evaluates code.
//! ADR-0037's prohibitions are carried forward.
//!
//! Declarations come from the pinned Tree-sitter MATLAB grammar (ADR-0058),
//! with the ADR-0046 declared subset applied over the tree. The lexer below
//! stays the lexical gate and runs first: it decides every lexical refusal,
//! reads the `classdef`, `methods`, and `function` header statements, and
//! supplies the command-syntax trigger. The grammar supplies block extents and
//! nesting, which `methods` blocks a class holds and which `function`
//! declarations a block holds. Outside the declared subset the frontend
//! abstains and never guesses a block boundary; a tree that needed error
//! recovery keeps only the declarations whose header precedes the failure.
//!
//! MATLAB's tick is overloaded exactly the way Ada's is, and it is decidable the
//! same way -- but the rule here is whitespace-sensitive rather than
//! token-sensitive: a tick immediately following an identifier character, `)`,
//! `]`, `}`, or another tick, with no intervening blank, is a transpose;
//! otherwise it opens a character array. That is what makes `[a' b']` two
//! transposes while `[a 'b']` is a concatenation with a character array.
//!
//! MATLAB's genuinely undecidable construct is command syntax: `a -1` is a
//! subtraction when `a` is a variable and the call `a('-1')` when `a` is a
//! function. The scanner this parser replaces argued the ambiguity never
//! reached it because command arguments are unquoted. That argument answers the
//! comment-and-string question, and a parser also has to answer the
//! block-structure question, where it does not hold: a command argument is an
//! unquoted word, so `end` and an unbalanced bracket can both sit in argument
//! position, and the two readings then disagree about where a block closes.
//! ADR-0046 D4b bounds it -- a statement whose command reading and expression
//! reading differ in block structure makes the file's block extents unproven,
//! and the file abstains.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};
use tree_sitter::{Node, Parser as TreeParser};

pub const MATLAB_ANCHOR_ENGINE: &str = "repogrammar-matlab-unittest-parser";
pub const MATLAB_ANCHOR_METHOD: &str = "bounded_matlab_unittest_class_v2";

/// Fixed support target for the one admitted exact anchor.
pub const MATLAB_TEST_TARGET: &str = "matlab_unittest.TestMethod";

const TEST_CASE_BASE: &str = "matlab.unittest.TestCase";
const MAX_UNITS: usize = 4_096;
/// Descent bound. Repository contents are untrusted, and no MATLAB class nests
/// blocks anywhere near this deep, so exceeding it is a degraded parse rather
/// than a stack overflow.
const MAX_BLOCK_DEPTH: usize = 256;
/// How many missing block terminators the Tree-sitter read closes at end of file
/// before it gives up on recovering declarations from a broken tree.
const MAX_END_REPAIRS: usize = 8;

/// MATLAB's reserved words. These can never name a variable, so a statement
/// beginning with one is never command syntax.
const RESERVED_WORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "classdef",
    "continue",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "parfor",
    "persistent",
    "return",
    "spmd",
    "switch",
    "try",
    "while",
];

/// Block terminators and openers that exist in Octave and not in MATLAB. In
/// MATLAB each is an ordinary identifier, so the two dialects disagree about
/// where a block closes, and the file's block extents become unproven.
const OCTAVE_BLOCK_WORDS: &[&str] = &[
    "endfunction",
    "endif",
    "endfor",
    "endwhile",
    "endswitch",
    "endparfor",
    "endclassdef",
    "endmethods",
    "endproperties",
    "endenumeration",
    "endevents",
    "end_try_catch",
    "end_unwind_protect",
    "unwind_protect",
    "unwind_protect_cleanup",
];

/// True for the only suffix this frontend may read.
pub fn is_matlab_source_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .ends_with(".m")
}

#[derive(Debug, Default)]
pub struct MatlabUnitTestParser;

impl SourceParser for MatlabUnitTestParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

pub(crate) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Matlab || !is_matlab_source_path(document.path) {
        return Err(ParseError::UnsupportedLanguage);
    }
    let full_range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let module = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#matlab_file:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Matlab,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };
    let mut units = vec![module.clone()];
    let mut facts = Vec::new();

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "matlab_declaration_scan",
            "source_byte_limit",
            full_range,
            "MATLAB source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    let lexed = lex(document.text);
    let mut diagnostics = Vec::new();
    // A parser reports a syntax failure, but these two are lexical
    // well-formedness violations that a parse alone would not surface, and each
    // one makes a missing anchor uninformative rather than meaningful.
    if lexed.unterminated_block_comment {
        diagnostics.push(degraded(
            document.path,
            "a MATLAB `%{` block comment is left open at end of file, so any declaration after it was read as comment",
        ));
    }
    if lexed.unterminated_text {
        diagnostics.push(degraded(
            document.path,
            "a MATLAB character array or string is left open at end of line, so the following token boundaries are unreliable",
        ));
    }

    let parsed = match lexed.divergence {
        Some(abstention) => FileParse {
            abstention: Some(abstention),
            ..FileParse::default()
        },
        None => tree_sitter_matlab_file(document.text, &lexed)?,
    };

    if let Some(abstention) = parsed.abstention {
        // Outside the declared subset the frontend abstains for the whole file:
        // a diverged reading unproves every later block boundary, so no anchor
        // it already collected may be reported as if the file were admitted.
        facts.push(unknown_fact(
            &module,
            abstention.reason,
            abstention.claim,
            abstention.kind,
            module.range.clone(),
            abstention.note,
        )?);
        diagnostics.push(degraded(document.path, abstention.note));
        return finish(units, facts, diagnostics);
    }

    if parsed.structural_failure {
        diagnostics.push(degraded(
            document.path,
            "MATLAB block structure does not close, so the declarations after the failure point were never parsed",
        ));
    }

    for anchor in parsed.anchors {
        if units.len() >= MAX_UNITS {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "matlab_declaration_scan",
                "scanner_resource_limit",
                module.range.clone(),
                "MATLAB parser exceeded the bounded unit limit",
            )?);
            break;
        }
        let (kind, target, assumption, note) = match anchor.kind {
            AnchorKind::TestClass => (
                CodeUnitKind::MatlabTestClass,
                "matlab_unittest.TestCase",
                "matlab_anchor_kind=unittest_test_class",
                "bounded MATLAB matlab.unittest.TestCase class anchor",
            ),
            AnchorKind::TestMethod => (
                CodeUnitKind::MatlabTestMethod,
                MATLAB_TEST_TARGET,
                "matlab_anchor_kind=unittest_test_method",
                "bounded MATLAB matlab.unittest Test methods-block anchor",
            ),
        };
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}",
                document.path,
                kind.as_str(),
                anchor.start,
                anchor.end
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Matlab,
            kind,
            range: SourceRange::new(anchor.start, anchor.end).map_err(ParseError::Internal)?,
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(&unit, target, assumption, note)?);
        units.push(unit);
    }

    if parsed.unbound_test_block {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "matlab_unittest_class_binding",
            "test_methods_block_without_testcase_base",
            module.range.clone(),
            "a Test methods block appears in a class that does not derive from matlab.unittest.TestCase, so the framework is unproven",
        )?);
    }

    for observation in parsed.observations {
        facts.push(unknown_fact(
            &module,
            observation.reason,
            observation.claim,
            observation.kind,
            module.range.clone(),
            observation.note,
        )?);
    }

    finish(units, facts, diagnostics)
}

fn degraded(path: &str, message: &str) -> ParseDiagnostic {
    ParseDiagnostic {
        path: path.to_string(),
        range: None,
        severity: ParseDiagnosticSeverity::Error,
        message: message.to_string(),
    }
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
) -> Result<SourceParseOutput, ParseError> {
    units.sort_by(|left, right| {
        (left.range.start_byte, left.range.end_byte, left.id.as_str()).cmp(&(
            right.range.start_byte,
            right.range.end_byte,
            right.id.as_str(),
        ))
    });
    sort_anchor_facts(&mut facts);
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics,
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Number,
    /// A `'…'` character array or a `"…"` string, kept opaque.
    Text,
    /// A `'` or `.'` transpose.
    Transpose,
    Open,
    Close,
    /// A single `=`, which is assignment. `==`, `~=`, `<=`, and `>=` are
    /// operators and lex as `Symbol`.
    Assign,
    /// `;` or `,`.
    Separator,
    LineBreak,
    Symbol,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
    /// Whether at least one blank byte precedes this token on its line. The
    /// tick rule and the command-syntax trigger both turn on this.
    blank_before: bool,
    byte: u8,
}

#[derive(Debug, Default)]
struct Lexed {
    tokens: Vec<Token>,
    unterminated_block_comment: bool,
    unterminated_text: bool,
    /// Byte ranges of character arrays and strings left open at end of line.
    unterminated_text_spans: Vec<(usize, usize)>,
    /// Where an unclosed `%{` block comment begins.
    unterminated_block_comment_from: Option<usize>,
    divergence: Option<Abstention>,
}

/// A construct outside the declared subset. The `kind` is a bounded class name;
/// no divergent source text is ever copied into a fact.
#[derive(Debug, Clone, Copy)]
struct Abstention {
    reason: UnknownReasonCode,
    claim: &'static str,
    kind: &'static str,
    note: &'static str,
}

const DIALECT_CLAIM: &str = "matlab_dialect_invariance";
const BLOCK_STRUCTURE_CLAIM: &str = "matlab_block_structure";
const CLASSDEF_SHAPE_CLAIM: &str = "matlab_classdef_body_shape";

fn dialect_abstention(kind: &'static str) -> Abstention {
    Abstention {
        reason: UnknownReasonCode::ConflictingFacts,
        claim: DIALECT_CLAIM,
        kind,
        note: "the file uses a construct MATLAB and Octave read differently, so its block extents hold under only one of the two dialects",
    }
}

fn command_abstention(kind: &'static str) -> Abstention {
    Abstention {
        reason: UnknownReasonCode::ConflictingFacts,
        claim: BLOCK_STRUCTURE_CLAIM,
        kind,
        note: "a statement reads as either command syntax or an expression, and the two readings disagree about where a block closes",
    }
}

fn lex(text: &str) -> Lexed {
    let mut lexed = Lexed::default();
    let mut in_block_comment = false;
    let mut block_comment_from = 0usize;
    let mut line_start = 0usize;
    for raw in text.split_inclusive('\n') {
        let line_end = line_start + raw.len();
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let trimmed = body.trim();
        // A `%{` / `%}` block delimiter must stand alone on its line.
        if in_block_comment {
            if trimmed == "%}" {
                in_block_comment = false;
            }
            line_start = line_end;
            continue;
        }
        if trimmed == "%{" {
            in_block_comment = true;
            block_comment_from = line_start;
            line_start = line_end;
            continue;
        }
        let continued = lex_line(body, line_start, &mut lexed);
        if !continued {
            lexed.tokens.push(Token {
                kind: TokenKind::LineBreak,
                start: line_end,
                end: line_end,
                blank_before: false,
                byte: b'\n',
            });
        }
        line_start = line_end;
    }
    lexed.unterminated_block_comment = in_block_comment;
    lexed.unterminated_block_comment_from = in_block_comment.then_some(block_comment_from);
    lexed
}

fn note_divergence(lexed: &mut Lexed, kind: &'static str) {
    if lexed.divergence.is_none() {
        lexed.divergence = Some(dialect_abstention(kind));
    }
}

/// Lex one physical line. Returns true when the line ended in a `...`
/// continuation, in which case the statement continues on the next line and no
/// `LineBreak` is emitted.
fn lex_line(line: &str, offset: usize, lexed: &mut Lexed) -> bool {
    LineLexer {
        line,
        offset,
        lexed,
        index: 0,
        // The byte immediately before a tick decides what it means, with no
        // whitespace skipping: `a'` is a transpose, `a '` opens a character
        // array.
        previous_byte: None,
        blank_before: true,
    }
    .run()
}

struct LineLexer<'a, 'b> {
    line: &'a str,
    offset: usize,
    lexed: &'b mut Lexed,
    index: usize,
    previous_byte: Option<u8>,
    blank_before: bool,
}

impl LineLexer<'_, '_> {
    fn emit(&mut self, kind: TokenKind, start: usize, end: usize, byte: u8) {
        self.lexed.tokens.push(Token {
            kind,
            start: self.offset + start,
            end: self.offset + end,
            blank_before: self.blank_before,
            byte,
        });
        self.blank_before = false;
        self.previous_byte = self.line.as_bytes().get(end - 1).copied();
        self.index = end;
    }

    fn run(mut self) -> bool {
        let bytes = self.line.as_bytes();
        while self.index < bytes.len() {
            let index = self.index;
            let byte = bytes[index];
            match byte {
                b'%' => return false,
                // Octave comments. MATLAB has no `#` at all, so the two dialects
                // do not agree on where this line's code ends.
                b'#' => {
                    note_divergence(self.lexed, "octave_hash_comment");
                    return false;
                }
                // A shell escape in MATLAB (rest of line is a command) and
                // logical negation in Octave. The readings share no token
                // stream.
                b'!' => {
                    note_divergence(self.lexed, "shell_escape_or_octave_negation");
                    return false;
                }
                _ if byte.is_ascii_whitespace() => {
                    self.blank_before = true;
                    self.previous_byte = Some(byte);
                    self.index += 1;
                    continue;
                }
                // `...` continues the statement on the next line, and everything
                // after it to end of line is ignored the way a comment is.
                b'.' if bytes.get(index + 1) == Some(&b'.')
                    && bytes.get(index + 2) == Some(&b'.') =>
                {
                    return true;
                }
                // `\` at end of line is an Octave continuation and a MATLAB
                // syntax error, so the statement extent differs between them.
                b'\\' if self.line[index + 1..].trim().is_empty() => {
                    note_divergence(self.lexed, "octave_backslash_continuation");
                    return false;
                }
                _ => {}
            }

            if byte.is_ascii_alphabetic() || byte == b'_' {
                let mut end = index;
                while end < bytes.len() && is_identifier_byte(bytes[end]) {
                    end += 1;
                }
                if OCTAVE_BLOCK_WORDS.contains(&&self.line[index..end]) {
                    note_divergence(self.lexed, "octave_block_keyword");
                }
                self.emit(TokenKind::Word, index, end, byte);
                continue;
            }

            if byte.is_ascii_digit() {
                let end = lex_number(bytes, index);
                self.emit(TokenKind::Number, index, end, byte);
                continue;
            }

            // `.'` is the non-conjugate transpose, not a character array.
            if byte == b'.'
                && bytes.get(index + 1) == Some(&b'\'')
                && tick_is_transpose(self.previous_byte)
                && index > 0
            {
                self.emit(TokenKind::Transpose, index, index + 2, b'\'');
                continue;
            }

            if byte == b'\'' || byte == b'"' {
                if byte == b'\'' && tick_is_transpose(self.previous_byte) {
                    self.emit(TokenKind::Transpose, index, index + 1, b'\'');
                    continue;
                }
                let (end, closed, escaped) = lex_quoted(bytes, index, byte);
                if !closed {
                    self.lexed.unterminated_text = true;
                    self.lexed
                        .unterminated_text_spans
                        .push((self.offset + index, self.offset + end));
                }
                if escaped && byte == b'"' {
                    // Octave honours backslash escapes inside double quotes and
                    // MATLAB does not, so `"a\"b"` is one string under Octave
                    // and a string followed by an unterminated one under MATLAB.
                    note_divergence(self.lexed, "octave_escaped_double_quoted_string");
                }
                self.emit(TokenKind::Text, index, end, byte);
                continue;
            }

            let (kind, width) = classify_operator(bytes, index);
            self.emit(kind, index, index + width, byte);
        }
        false
    }
}

/// Consume a numeric literal. A trailing `.` is consumed only when a digit
/// follows, so `1.'` stays a number and a transpose.
fn lex_number(bytes: &[u8], mut index: usize) -> usize {
    if bytes[index] == b'0' && matches!(bytes.get(index + 1), Some(b'x') | Some(b'X')) {
        index += 2;
        while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
            index += 1;
        }
        return index;
    }
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        let mut lookahead = index + 1;
        if matches!(bytes.get(lookahead), Some(b'+') | Some(b'-')) {
            lookahead += 1;
        }
        if bytes.get(lookahead).is_some_and(u8::is_ascii_digit) {
            index = lookahead;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
    }
    if matches!(bytes.get(index), Some(b'i') | Some(b'j')) {
        index += 1;
    }
    index
}

/// Consume a quoted run with `''`/`""` doubling. Returns the index after it,
/// whether it closed on this line, and whether it contained a backslash.
fn lex_quoted(bytes: &[u8], start: usize, quote: u8) -> (usize, bool, bool) {
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            escaped = true;
        }
        if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return (index + 1, true, escaped);
        }
        index += 1;
    }
    (index, false, escaped)
}

fn classify_operator(bytes: &[u8], index: usize) -> (TokenKind, usize) {
    let byte = bytes[index];
    let next = bytes.get(index + 1).copied();
    match (byte, next) {
        (b'=', Some(b'=')) => (TokenKind::Symbol, 2),
        (b'~', Some(b'=')) | (b'<', Some(b'=')) | (b'>', Some(b'=')) => (TokenKind::Symbol, 2),
        (b'&', Some(b'&')) | (b'|', Some(b'|')) => (TokenKind::Symbol, 2),
        (b'.', Some(b'*')) | (b'.', Some(b'/')) | (b'.', Some(b'\\')) | (b'.', Some(b'^')) => {
            (TokenKind::Symbol, 2)
        }
        (b'=', _) => (TokenKind::Assign, 1),
        (b'(', _) | (b'[', _) | (b'{', _) => (TokenKind::Open, 1),
        (b')', _) | (b']', _) | (b'}', _) => (TokenKind::Close, 1),
        (b';', _) | (b',', _) => (TokenKind::Separator, 1),
        _ => (TokenKind::Symbol, 1),
    }
}

fn tick_is_transpose(previous: Option<u8>) -> bool {
    previous.is_some_and(|byte| {
        is_identifier_byte(byte) || byte == b')' || byte == b']' || byte == b'}' || byte == b'\''
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnchorKind {
    TestClass,
    TestMethod,
}

#[derive(Debug)]
struct Anchor {
    kind: AnchorKind,
    start: usize,
    end: usize,
}

#[derive(Default)]
struct FileParse {
    anchors: Vec<Anchor>,
    unbound_test_block: bool,
    /// The declared subset was left. Every anchor is dropped.
    abstention: Option<Abstention>,
    /// The block grammar did not close. Anchors proven before the failure point
    /// are kept, because each one rests only on headers that precede it.
    structural_failure: bool,
    observations: Vec<Abstention>,
}

// ---------------------------------------------------------------------------
// Tree-sitter syntax layer (ADR-0058)
// ---------------------------------------------------------------------------

fn tree_sitter_matlab_file(text: &str, lexed: &Lexed) -> Result<FileParse, ParseError> {
    let mut parser = TreeParser::new();
    parser
        .set_language(&tree_sitter_matlab::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load MATLAB grammar: {error}")))?;
    let source = grammar_view(text, lexed);
    let parse_failed = || ParseError::Internal("Tree-sitter MATLAB parse failed".to_string());
    let mut tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(parse_failed)?;
    // A block left open at end of file leaves a recovered tree with no
    // `methods` or `function` nodes. Closing the blocks reads the declarations
    // the file did declare, and the read still fails at the end of the file.
    let mut repaired = false;
    if tree.root_node().has_error() {
        for count in 1..=MAX_END_REPAIRS {
            let closed = format!("{source}{}", "end\n".repeat(count));
            let candidate = parser
                .parse(closed.as_bytes(), None)
                .ok_or_else(parse_failed)?;
            if !candidate.root_node().has_error() {
                tree = candidate;
                repaired = true;
                break;
            }
        }
    }
    Ok(parse_tree(text, &lexed.tokens, tree.root_node(), repaired))
}

/// The text the grammar reads, with byte offsets identical to `text`. The lexer
/// is authoritative for the lexical subset and has already reported a
/// character array or block comment left open as a degraded parse, so those
/// runs are neutralised rather than allowed to turn the rest of the file into
/// one recovered region. The grammar also needs a final line break to close the
/// last statement.
fn grammar_view(text: &str, lexed: &Lexed) -> String {
    let mut bytes = text.as_bytes().to_vec();
    for &(start, end) in &lexed.unterminated_text_spans {
        bytes[start] = b'0';
        bytes[start + 1..end].fill(b' ');
    }
    if let Some(start) = lexed.unterminated_block_comment_from {
        for byte in &mut bytes[start..] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }
    if bytes.last().is_some_and(|byte| *byte != b'\n') {
        bytes.push(b'\n');
    }
    // Only ASCII bytes were replaced, over whole characters, so the text is
    // still valid UTF-8.
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_string())
}

/// How a left-to-right read of the file ends. The earliest position wins, and
/// at one position a construct outside the subset precedes a structural
/// failure, exactly as in the retired descent.
#[derive(Clone, Copy)]
enum Cause {
    /// The block grammar does not close. Declarations whose header precedes the
    /// failure point are still real.
    Structural,
    /// A construct outside the declared subset. The file abstains.
    Outside(Abstention),
}

/// The ADR-0046 declared-subset policy, read off a Tree-sitter MATLAB tree
/// (ADR-0058).
///
/// The lexer stays the lexical gate and the statement-level reader for the two
/// things a grammar cannot decide: the command-syntax ambiguity (D4b) and the
/// `classdef`, `methods`, and `function` header statements. The grammar supplies
/// block extents and nesting: which `methods` blocks a class holds and which
/// `function` declarations a block holds. An `ERROR` or `MISSING` node ends the
/// read at the end of that node as a structural failure, so a declaration after
/// a recovered region never yields an anchor.
fn parse_tree(text: &str, tokens: &[Token], root: Node<'_>, repaired: bool) -> FileParse {
    let mut out = FileParse::default();
    // ADR-0046 D1: only a file that opens with `classdef` carries the anchor.
    let Some(head_index) = tokens
        .iter()
        .position(|token| !matches!(token.kind, TokenKind::Separator | TokenKind::LineBreak))
    else {
        return out;
    };
    let head = tokens[head_index];
    if head.kind != TokenKind::Word || token_word(text, &head) != "classdef" {
        return out;
    }
    let header = statement_span(text, tokens, head_index);
    let derives_test_case = if header.balanced {
        class_derives_test_case(text, tokens, head_index + 1, header.end)
    } else {
        Err(())
    };
    let Ok(derives_test_case) = derives_test_case else {
        out.structural_failure = true;
        return out;
    };

    let mut causes: Vec<(usize, Cause)> = Vec::new();
    let mut anchors: Vec<(usize, Anchor)> = Vec::new();
    let mut observations: Vec<(usize, Abstention)> = Vec::new();
    let mut unbound: Vec<usize> = Vec::new();
    if derives_test_case {
        let (start, end) = line_bounds(text, head.start);
        anchors.push((
            head.start,
            Anchor {
                kind: AnchorKind::TestClass,
                start,
                end,
            },
        ));
    }

    // Only the first construct is read: a local function after the class is
    // outside the declared subset, so it is neither anchored nor checked.
    let scope = match first_construct(root) {
        Some(node) if node.kind() == "class_definition" => node,
        _ => root,
    };
    let mut blocks: Vec<Node<'_>> = Vec::new();
    let mut functions: Vec<Node<'_>> = Vec::new();
    walk(scope, |node| {
        // The anonymous keyword leaf of a block shares the block's kind name.
        if !node.is_named() {
            return false;
        }
        match node.kind() {
            "methods" | "properties" | "events" | "enumeration" => {
                blocks.push(node);
                // Only a `methods` block holds functions.
                return node.kind() == "methods";
            }
            "function_definition" => functions.push(node),
            _ => {}
        }
        true
    });

    for block in blocks.iter().filter(|block| block.kind() == "methods") {
        let keyword = token_index_at(tokens, block.start_byte());
        let Some(token) = tokens
            .get(keyword)
            .filter(|token| token.start == block.start_byte())
        else {
            continue;
        };
        let span = statement_span(text, tokens, keyword);
        if !span.balanced {
            causes.push((token.start, Cause::Structural));
            continue;
        }
        let mut is_test = false;
        let mut valued_test = false;
        let mut is_abstract = false;
        if tokens
            .get(keyword + 1)
            .is_some_and(|token| token.byte == b'(')
        {
            for (name, valued) in attribute_list(text, tokens, keyword + 1, span.end) {
                match name {
                    "Test" if valued => valued_test = true,
                    "Test" => is_test = true,
                    "Abstract" => is_abstract = true,
                    _ => {}
                }
            }
        }
        if valued_test && !is_test {
            // `methods (Test = <expr>)` needs the expression evaluated to know
            // whether the block is a test block, and this frontend evaluates
            // nothing.
            observations.push((
                token.start,
                Abstention {
                    reason: UnknownReasonCode::InsufficientSupport,
                    claim: "matlab_test_attribute_value",
                    kind: "test_attribute_with_value",
                    note: "a methods block sets the Test attribute to an expression, which cannot be read without evaluating it",
                },
            ));
        }
        if is_test && !derives_test_case {
            unbound.push(token.start);
        }
        if is_test && is_abstract {
            // An abstract method has no implementation, so the declaring class
            // declares no runnable test.
            observations.push((
                token.start,
                Abstention {
                    reason: UnknownReasonCode::InsufficientSupport,
                    claim: "matlab_abstract_test_methods",
                    kind: "abstract_test_methods_block",
                    note: "a Test methods block is declared Abstract, so its members are signatures rather than test implementations",
                },
            ));
        }
        let anchors_tests = is_test && derives_test_case && !is_abstract;
        for function in children_through_errors(*block)
            .into_iter()
            .filter(|child| child.kind() == "function_definition")
        {
            let keyword = token_index_at(tokens, function.start_byte());
            let Some(token) = tokens
                .get(keyword)
                .filter(|token| token.start == function.start_byte())
            else {
                continue;
            };
            let span = statement_span(text, tokens, keyword);
            if !span.balanced
                || span.depth_zero_end.is_some()
                || !function_header_is_admitted(text, tokens, keyword + 1, span.end)
            {
                causes.push((token.start, Cause::Structural));
                continue;
            }
            if anchors_tests {
                let (start, end) = line_bounds(text, token.start);
                anchors.push((
                    token.start,
                    Anchor {
                        kind: AnchorKind::TestMethod,
                        start,
                        end,
                    },
                ));
            }
        }
    }

    for function in &functions {
        if let Some(cause) = command_ambiguity(text, tokens, *function) {
            causes.push(cause);
        }
    }
    for block in &blocks {
        if let Some(position) = opaque_statement_failure(text, tokens, *block) {
            causes.push((position, Cause::Structural));
        }
    }
    if let Some(position) = region_depth_violation(scope) {
        causes.push((position, Cause::Structural));
    }
    // Error recovery decides where a recovered region ends, which is not where a
    // left-to-right read fails, so a construct outside the subset that the
    // token stream proves outranks it. Headers and depth are exact positions.
    let mut recovery = None;
    if repaired {
        causes.push((text.len(), Cause::Structural));
    } else if root.has_error() {
        recovery = Some(first_error_end(root).unwrap_or(text.len()));
        if let Some(position) = unadmitted_body_construct(text, tokens, header.end, &blocks) {
            causes.push((position, Cause::Outside(unadmitted_classdef_construct())));
        }
    }
    let mut halt = causes
        .iter()
        .copied()
        .min_by_key(|(position, cause)| (*position, matches!(cause, Cause::Structural)));
    if let Some(failure) = recovery {
        halt = match halt {
            Some((position, cause))
                if matches!(cause, Cause::Outside(_)) || position <= failure =>
            {
                Some((position, cause))
            }
            _ => Some((failure, Cause::Structural)),
        };
    }
    let limit = match halt {
        Some((_, Cause::Outside(abstention))) => {
            // A diverged reading unproves every block boundary, so no anchor it
            // already collected may be reported as if the file were admitted.
            out.abstention = Some(abstention);
            return out;
        }
        Some((position, Cause::Structural)) => {
            out.structural_failure = true;
            position
        }
        None => usize::MAX,
    };
    out.anchors = anchors
        .into_iter()
        .filter(|(position, _)| *position < limit)
        .map(|(_, anchor)| anchor)
        .collect();
    out.observations = observations
        .into_iter()
        .filter(|(position, _)| *position < limit)
        .map(|(_, observation)| observation)
        .collect();
    out.unbound_test_block = unbound.iter().any(|position| *position < limit);
    out
}

/// The first named node that is not a comment.
fn first_construct(root: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = root.walk();
    let first = root
        .named_children(&mut cursor)
        .find(|node| node.kind() != "comment");
    first
}

/// Children of `node`, looking one level into recovery `ERROR` nodes.
fn children_through_errors(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    let mut found = Vec::new();
    for child in node.named_children(&mut cursor) {
        if child.is_error() {
            let mut inner = child.walk();
            found.extend(child.named_children(&mut inner));
        } else {
            found.push(child);
        }
    }
    found
}

/// Document-order walk without recursion, because repository contents are
/// untrusted and a deeply nested file must not overflow the stack. `visit`
/// returns whether to descend into the node.
fn walk<'t>(root: Node<'t>, mut visit: impl FnMut(Node<'t>) -> bool) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if visit(node) {
            for index in (0..node.child_count()).rev() {
                if let Some(child) = node.child(index) {
                    stack.push(child);
                }
            }
        }
    }
}

/// Where the recovered region ends: the end of the innermost `ERROR` node, or
/// the start of a `MISSING` token, whichever comes first.
fn first_error_end(root: Node<'_>) -> Option<usize> {
    let mut earliest: Option<usize> = None;
    walk(root, |node| {
        if !node.has_error() && !node.is_missing() {
            return false;
        }
        let position = if node.is_missing() {
            Some(node.start_byte())
        } else if node.is_error() && !has_error_child(node) {
            Some(node.end_byte())
        } else {
            None
        };
        if let Some(position) = position {
            earliest = Some(earliest.map_or(position, |current| current.min(position)));
        }
        true
    });
    earliest
}

fn has_error_child(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| child.has_error() || child.is_missing());
    found
}

/// The start of the first block that nests deeper than `MAX_BLOCK_DEPTH`.
/// Every statement region is one level: a function body, and each nested
/// `if`, `for`/`parfor`, `while`, `switch`, `try`, `spmd`, or `arguments` block.
/// `elseif`, `else`, `case`, `otherwise`, and `catch` clauses are continuations
/// and add none.
fn region_depth_violation(scope: Node<'_>) -> Option<usize> {
    let mut stack = vec![(scope, 0usize)];
    let mut earliest: Option<usize> = None;
    while let Some((node, depth)) = stack.pop() {
        let depth = depth
            + usize::from(matches!(
                node.kind(),
                "function_definition"
                    | "if_statement"
                    | "for_statement"
                    | "while_statement"
                    | "switch_statement"
                    | "try_statement"
                    | "spmd_statement"
                    | "arguments_statement"
            ));
        if depth > MAX_BLOCK_DEPTH {
            earliest = Some(earliest.map_or(node.start_byte(), |c| c.min(node.start_byte())));
            continue;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index) {
                stack.push((child, depth));
            }
        }
    }
    earliest
}

/// D4b: a statement whose command reading and expression reading differ in
/// block structure makes the file's block extents unproven. The grammar's
/// scanner commits to one reading, so the lexer's over-approximating trigger
/// decides, at every statement start inside the function's body.
fn command_ambiguity(text: &str, tokens: &[Token], function: Node<'_>) -> Option<(usize, Cause)> {
    let keyword = token_index_at(tokens, function.start_byte());
    tokens
        .get(keyword)
        .filter(|token| token.start == function.start_byte())?;
    let mut index = statement_span(text, tokens, keyword).end;
    let mut at_start = true;
    let mut first_statement = true;
    let mut depth = 0usize;
    while let Some(token) = tokens.get(index) {
        if token.start >= function.end_byte() {
            break;
        }
        match token.kind {
            TokenKind::Separator | TokenKind::LineBreak if depth == 0 => at_start = true,
            kind => {
                if at_start {
                    at_start = false;
                    // `arguments` leading a function body opens a block, not a
                    // command.
                    let opens_arguments = first_statement
                        && token.kind == TokenKind::Word
                        && token_word(text, token) == "arguments"
                        && !tokens
                            .get(index + 1)
                            .is_some_and(|next| next.kind == TokenKind::Assign);
                    first_statement = opens_arguments;
                    if opens_arguments {
                        let span = statement_span(text, tokens, index);
                        if !span.balanced || span.depth_zero_end.is_some() {
                            return Some((token.start, Cause::Structural));
                        }
                    }
                    if !opens_arguments && possible_command_syntax(text, tokens, index) {
                        let span = statement_span(text, tokens, index);
                        let kind = if !span.balanced {
                            Some("command_syntax_bracket_balance")
                        } else if span.depth_zero_end.is_some() {
                            Some("command_syntax_end_keyword")
                        } else {
                            None
                        };
                        if let Some(kind) = kind {
                            return Some((token.start, Cause::Outside(command_abstention(kind))));
                        }
                    }
                }
                match kind {
                    TokenKind::Open => depth += 1,
                    TokenKind::Close => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        index += 1;
    }
    None
}

/// A `methods`, `properties`, `events`, or `enumeration` body outside its
/// functions holds signatures and declarations that this frontend reads only
/// for extent: each must be a non-empty, bracket-balanced statement that does
/// not carry a bare `end`, because that `end` could close the block.
fn opaque_statement_failure(text: &str, tokens: &[Token], block: Node<'_>) -> Option<usize> {
    let first = token_index_at(tokens, block.start_byte());
    let closing = token_index_at(tokens, block.end_byte()).checked_sub(1)?;
    let closes = tokens.get(closing)?;
    if closing <= first || closes.kind != TokenKind::Word || token_word(text, closes) != "end" {
        return None;
    }
    let functions: Vec<(usize, usize)> = children_through_errors(block)
        .into_iter()
        .filter(|child| child.kind() == "function_definition")
        .map(|child| (child.start_byte(), child.end_byte()))
        .collect();
    let mut index = statement_span(text, tokens, first).end;
    loop {
        index = skip_separator_tokens(tokens, index);
        if index >= closing {
            return None;
        }
        let token = tokens[index];
        if let Some((_, end)) = functions.iter().find(|(start, _)| *start == token.start) {
            index = token_index_at(tokens, *end).max(index + 1);
            continue;
        }
        let span = statement_span(text, tokens, index);
        if !span.balanced || span.depth_zero_end.is_some() || span.end == index {
            return Some(token.start);
        }
        index = span.end;
    }
}

/// In a tree that needed recovery, the first token of the class body that is
/// neither a block the grammar read nor the class's closing `end`.
fn unadmitted_body_construct(
    text: &str,
    tokens: &[Token],
    body_start: usize,
    blocks: &[Node<'_>],
) -> Option<usize> {
    let mut index = body_start;
    loop {
        index = skip_separator_tokens(tokens, index);
        let token = tokens.get(index)?;
        if let Some(block) = blocks
            .iter()
            .find(|block| block.start_byte() == token.start)
        {
            index = token_index_at(tokens, block.end_byte()).max(index + 1);
            continue;
        }
        if token.kind == TokenKind::Word
            && matches!(
                token_word(text, token),
                "end" | "methods" | "properties" | "events" | "enumeration"
            )
        {
            return None;
        }
        return Some(token.start);
    }
}

fn token_word<'a>(text: &'a str, token: &Token) -> &'a str {
    &text[token.start..token.end]
}

/// The first token that starts at or after `byte`.
fn token_index_at(tokens: &[Token], byte: usize) -> usize {
    tokens.partition_point(|token| token.start < byte)
}

fn skip_separator_tokens(tokens: &[Token], mut index: usize) -> usize {
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Separator | TokenKind::LineBreak))
    {
        index += 1;
    }
    index
}

/// The extent of the statement beginning at token `index`, ending at the first
/// `;`, `,`, or line break outside brackets. A line break inside `[]` or `{}`
/// is a row separator, not a statement end.
fn statement_span(text: &str, tokens: &[Token], index: usize) -> StatementSpan {
    let mut depth = 0usize;
    let mut cursor = index;
    let mut balanced = true;
    let mut depth_zero_end = None;
    while cursor < tokens.len() {
        let token = tokens[cursor];
        match token.kind {
            TokenKind::Open => depth += 1,
            TokenKind::Close => {
                if depth == 0 {
                    balanced = false;
                } else {
                    depth -= 1;
                }
            }
            TokenKind::Separator | TokenKind::LineBreak if depth == 0 => {
                return StatementSpan {
                    end: cursor,
                    balanced,
                    depth_zero_end,
                };
            }
            TokenKind::Word if depth == 0 && token_word(text, &token) == "end" => {
                depth_zero_end.get_or_insert(cursor);
            }
            _ => {}
        }
        cursor += 1;
    }
    StatementSpan {
        end: cursor,
        balanced: balanced && depth == 0,
        depth_zero_end,
    }
}

/// The over-approximating command-syntax trigger: an unreserved identifier at
/// statement start, a blank, and something that is neither an assignment nor a
/// call's `(`. Over-detection only adds a divergence check; under-detection
/// would miscount an `end`.
fn possible_command_syntax(text: &str, tokens: &[Token], index: usize) -> bool {
    let Some(head) = tokens.get(index) else {
        return false;
    };
    if head.kind != TokenKind::Word || RESERVED_WORDS.contains(&token_word(text, head)) {
        return false;
    }
    let Some(next) = tokens.get(index + 1) else {
        return false;
    };
    if !next.blank_before {
        return false;
    }
    // An assignment, a bare `foo` statement, and a `foo (…)` call are all
    // decided without the workspace, so none of them is ambiguous.
    let decided = matches!(
        next.kind,
        TokenKind::Assign | TokenKind::Separator | TokenKind::LineBreak
    ) || (next.kind == TokenKind::Open && next.byte == b'(');
    !decided
}

/// `classdef [ (attrs) ] Name [ < Base [ & Base ]* ]`. Attributes are skipped
/// so the header is exact; ADR-0046 D2b keeps `Abstract` out of the claim.
fn class_derives_test_case(
    text: &str,
    tokens: &[Token],
    mut cursor: usize,
    end: usize,
) -> Result<bool, ()> {
    if tokens.get(cursor).is_some_and(|token| token.byte == b'(') {
        cursor = skip_bracketed(tokens, cursor, end)?;
    }
    // The class name.
    let name = tokens.get(cursor).ok_or(())?;
    if name.kind != TokenKind::Word {
        return Err(());
    }
    cursor += 1;
    let Some(separator) = tokens.get(cursor) else {
        return Ok(false);
    };
    if separator.kind != TokenKind::Symbol || separator.byte != b'<' || cursor >= end {
        return Ok(false);
    }
    cursor += 1;
    let mut derives = false;
    while cursor < end {
        let (base, next) = read_dotted_name(text, tokens, cursor, end)?;
        if base == TEST_CASE_BASE {
            derives = true;
        }
        cursor = next;
        match tokens.get(cursor) {
            Some(token) if token.byte == b'&' && cursor < end => cursor += 1,
            _ => break,
        }
    }
    Ok(derives)
}

fn read_dotted_name(
    text: &str,
    tokens: &[Token],
    mut cursor: usize,
    end: usize,
) -> Result<(String, usize), ()> {
    let mut name = String::new();
    loop {
        let token = tokens.get(cursor).ok_or(())?;
        if cursor >= end || token.kind != TokenKind::Word {
            return Err(());
        }
        name.push_str(token_word(text, token));
        cursor += 1;
        match tokens.get(cursor) {
            Some(dot) if dot.kind == TokenKind::Symbol && dot.byte == b'.' && cursor < end => {
                name.push('.');
                cursor += 1;
            }
            _ => return Ok((name, cursor)),
        }
    }
}

fn skip_bracketed(tokens: &[Token], cursor: usize, end: usize) -> Result<usize, ()> {
    let mut depth = 0usize;
    let mut index = cursor;
    while index < end {
        match tokens[index].kind {
            TokenKind::Open => depth += 1,
            TokenKind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    Err(())
}

/// Attribute names of a `( … )` list, each paired with whether it carried a
/// `= value`.
fn attribute_list<'a>(
    text: &'a str,
    tokens: &[Token],
    cursor: usize,
    end: usize,
) -> Vec<(&'a str, bool)> {
    let mut attributes = Vec::new();
    let mut depth = 0usize;
    let mut index = cursor;
    let mut pending: Option<&'a str> = None;
    let mut valued = false;
    while index < end {
        let token = tokens[index];
        match token.kind {
            TokenKind::Open => depth += 1,
            TokenKind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            TokenKind::Word if depth == 1 && pending.is_none() => {
                pending = Some(token_word(text, &token));
            }
            TokenKind::Assign if depth == 1 => valued = true,
            TokenKind::Separator if depth == 1 => {
                if let Some(name) = pending.take() {
                    attributes.push((name, valued));
                }
                valued = false;
            }
            _ => {}
        }
        index += 1;
    }
    if let Some(name) = pending.take() {
        attributes.push((name, valued));
    }
    attributes
}

/// `function [out, …] = name[.accessor](args)`: the header statement between
/// the keyword and its end.
fn function_header_is_admitted(
    text: &str,
    tokens: &[Token],
    mut cursor: usize,
    end: usize,
) -> bool {
    if let Some(assignment) = (cursor..end).find(|index| tokens[*index].kind == TokenKind::Assign) {
        cursor = assignment + 1;
    }
    // The function name, optionally a `get.`/`set.` property accessor.
    let Ok((_, next)) = read_dotted_name(text, tokens, cursor, end) else {
        return false;
    };
    cursor = next;
    if cursor < end {
        let Some(token) = tokens.get(cursor) else {
            return false;
        };
        if token.byte != b'(' {
            return false;
        }
        let Ok(next) = skip_bracketed(tokens, cursor, end) else {
            return false;
        };
        cursor = next;
    }
    cursor >= end
}

struct StatementSpan {
    end: usize,
    balanced: bool,
    depth_zero_end: Option<usize>,
}

fn unadmitted_classdef_construct() -> Abstention {
    Abstention {
        reason: UnknownReasonCode::InsufficientSupport,
        claim: CLASSDEF_SHAPE_CLAIM,
        kind: "unadmitted_classdef_body_construct",
        note: "a classdef body holds a construct outside the declared subset, so the extents of the blocks around it are unproven",
    }
}

/// The anchor range is the physical line the declaration keyword sits on.
fn line_bounds(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let start = bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    let end = bytes[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(text.len(), |position| offset + position + 1);
    (start, end)
}

fn anchor_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            assumption.to_string(),
        ],
    })
}

fn unknown_fact(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    affected_claim: &str,
    kind: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={affected_claim}"),
            format!("matlab_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: MATLAB_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: MATLAB_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        MatlabUnitTestParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/CatalogTest.m",
                    language: Language::Matlab,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse MATLAB source")
    }

    fn tests_found(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(MATLAB_TEST_TARGET))
            .count()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| {
                assumption
                    .strip_prefix("matlab_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    /// Wrap `body` as the sole statement of one test method in a class that
    /// also declares two other tests, so an abstention is visible as the loss of
    /// anchors that would otherwise exist.
    fn class_with_body(body: &str) -> String {
        format!(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             {body}\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20       function sortsCatalog(testCase)\n        end\n\
             \x20   end\nend\n"
        )
    }

    // -- exclusions: constructs the parser must refuse rather than guess ----

    /// The scanner argued command syntax never reached it because command
    /// arguments are unquoted. A parser has to decide block structure, and an
    /// unquoted `end` in argument position is exactly where the two readings
    /// disagree.
    #[test]
    fn a_command_whose_argument_is_end_abstains_instead_of_guessing() {
        let parsed = output(&class_with_body("            format end"));
        assert_eq!(
            tests_found(&parsed),
            0,
            "an undecidable block boundary must drop every anchor in the file"
        );
        assert!(unknown_kinds(&parsed).contains(&"command_syntax_end_keyword".to_string()));
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
    }

    /// The other divergence class: an unquoted bracket is one character of a
    /// command argument and a grouping token in the expression reading.
    #[test]
    fn a_command_with_an_unbalanced_bracket_argument_abstains() {
        let parsed = output(&class_with_body("            warning off]"));
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"command_syntax_bracket_balance".to_string()));
    }

    /// `dbstop if error` is real MATLAB and carries a block keyword as an
    /// argument. Both readings consume the same tokens and neither opens a
    /// block, so the ambiguity is claim-irrelevant and the file still anchors.
    #[test]
    fn a_command_argument_that_is_a_block_keyword_but_not_end_still_anchors() {
        let parsed = output(&class_with_body(
            "            dbstop if error\n            hold on\n            a -1",
        ));
        assert_eq!(tests_found(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
    }

    /// Octave closes blocks with keywords MATLAB reads as ordinary identifiers,
    /// so the two dialects disagree about where the block ends.
    #[test]
    fn an_octave_block_terminator_abstains_rather_than_choosing_a_dialect() {
        for octave in ["endfunction", "endif", "unwind_protect"] {
            let parsed = output(&class_with_body(&format!("            x = {octave};")));
            assert_eq!(tests_found(&parsed), 0, "{octave}");
            assert!(
                unknown_kinds(&parsed).contains(&"octave_block_keyword".to_string()),
                "{octave}"
            );
        }
    }

    /// Octave honours backslash escapes inside double quotes and MATLAB does
    /// not, so the same bytes are one string or two.
    #[test]
    fn an_escaped_double_quoted_string_abstains_because_the_dialects_lex_it_differently() {
        let parsed = output(&class_with_body("            s = \"a\\\"b\";"));
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"octave_escaped_double_quoted_string".to_string()));
    }

    #[test]
    fn an_octave_hash_comment_or_a_shell_escape_abstains() {
        let hashed = output(&class_with_body("            x = 1; # trailing"));
        assert_eq!(tests_found(&hashed), 0);
        assert!(unknown_kinds(&hashed).contains(&"octave_hash_comment".to_string()));

        let escaped = output(&class_with_body("            !ls"));
        assert_eq!(tests_found(&escaped), 0);
        assert!(unknown_kinds(&escaped).contains(&"shell_escape_or_octave_negation".to_string()));
    }

    /// A classdef body admits only `methods`, `properties`, `events`, and
    /// `enumeration`. Anything else -- including a block kind a later release
    /// adds -- leaves the declared subset.
    #[test]
    fn an_unrecognised_classdef_body_construct_abstains() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20       function sortsCatalog(testCase)\n        end\n\
             \x20   end\n\
             \x20   contracts (Guaranteed)\n        x = 1;\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_classdef_body_construct".to_string()));
    }

    /// An abstract method has no implementation, so the class declares no
    /// runnable test even though the block carries `Test`.
    #[test]
    fn an_abstract_test_methods_block_declares_no_test() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Abstract, Test)\n\
             \x20       loadsCatalog(testCase)\n\
             \x20       filtersCatalog(testCase)\n\
             \x20       sortsCatalog(testCase)\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"abstract_test_methods_block".to_string()));
    }

    /// `methods (Test = <expr>)` needs the expression evaluated, and this
    /// frontend evaluates nothing.
    #[test]
    fn a_test_attribute_with_a_value_is_not_read_as_a_test_block() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test = shouldRun())\n\
             \x20       function loadsCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"test_attribute_with_value".to_string()));
    }

    /// `methods` is contextual, not reserved. A flat keyword list would open a
    /// block here and swallow the rest of the class.
    #[test]
    fn a_contextual_keyword_used_as_a_variable_opens_no_block() {
        let parsed = output(&class_with_body(
            "            methods = 3;\n            properties = 4;\n            arguments = 5;",
        ));
        assert_eq!(tests_found(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
    }

    /// A `classdef` must be the first construct in its file, so a function file
    /// cannot carry the anchor and receives no structural claim -- and no
    /// diagnostic, because nothing was attempted.
    #[test]
    fn a_file_that_does_not_open_with_classdef_is_never_parsed_for_the_anchor() {
        let parsed = output(
            "function localHelper()\n    methods (Test)\n\
             \x20   function looksLikeATest(testCase)\n    end\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(parsed.report.diagnostics.is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn a_function_nested_inside_a_test_body_is_not_itself_a_test() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           function inner(value)\n            end\n\
             \x20       end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn a_test_block_in_a_class_that_is_not_a_testcase_anchors_nothing() {
        let parsed = output(
            "classdef CatalogHelper < handle\n    methods (Test)\n\
             \x20       function looksLikeATest(obj)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed)
            .contains(&"test_methods_block_without_testcase_base".to_string()));
    }

    // -- admissions: the shape ADR-0046 declares -----------------------------

    #[test]
    fn admitted_methods_anchor_and_helpers_after_the_classdef_do_not() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           testCase.verifyTrue(true);\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n\
             \x20           if true\n                   x = 1;\n               end\n\
             \x20       end\n\
             \x20   end\n\
             \x20   methods (Access = private)\n\
             \x20       function helper(testCase)\n            end\n\
             \x20   end\n\
             end\n\
             function localHelper()\n\
             end\n",
        );
        assert_eq!(
            tests_found(&parsed),
            2,
            "the private method and the file-local function are not tests"
        );
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::MatlabTestClass)
                .count(),
            1
        );
    }

    #[test]
    fn a_transpose_is_not_a_character_array() {
        // `[a' b']` is two transposes; `[a 'b']` is a concatenation with a
        // character array. Only the blank tells them apart.
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           v = [a' b'];\n\
             \x20           w = [a 'end'];\n\
             \x20           u = a.' * b;\n\
             \x20           t = 1.' + 2;\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(
            tests_found(&parsed),
            2,
            "a stray `end` inside a character array must not close the block"
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_comment_or_a_string_never_declares_a_test() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           % function inAComment(testCase)\n\
             \x20           s = 'function inAString(testCase)';\n\
             \x20           d = \"function inADoubleQuoted(testCase)\";\n\
             \x20       end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn a_block_comment_hides_everything_inside_it() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n\
             %{\n        function hidden(testCase)\n        end\n%}\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn an_ellipsis_continuation_comment_does_not_close_a_block() {
        // Everything after `...` is ignored to end of line, so the bare `end`
        // in that trailing text is not a block terminator.
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           total = 1 + ... sum everything through to the end\n\
             \x20                   2;\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
        assert!(parsed.report.diagnostics.is_empty());
    }

    /// A `...` suppresses the line break, so a continued header is one
    /// statement. The line scanner read only the first physical line, found an
    /// empty superclass list, and reported an unbound `Test` block for a class
    /// that plainly derives from `matlab.unittest.TestCase`.
    #[test]
    fn a_continued_classdef_header_is_one_statement_and_still_binds_the_framework() {
        let parsed = output(
            "classdef CatalogTest < ...\n\
             \x20       matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
        assert!(
            !unknown_kinds(&parsed)
                .contains(&"test_methods_block_without_testcase_base".to_string()),
            "the base is on the continuation line, not absent"
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn an_end_used_as_an_index_does_not_close_a_block() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           v = data(2:end);\n            w = c{end};\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
    }

    #[test]
    fn a_one_line_block_stays_balanced() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           if true, x = 1; end\n\
             \x20           switch x\n                case 1\n                    y = 2;\n\
             \x20               otherwise\n                    y = 3;\n            end\n\
             \x20           try\n                z = 1;\n            catch err\n                z = 2;\n            end\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn only_the_test_attribute_opens_the_block() {
        for attributes in ["(Access = public)", "(Static)", ""] {
            let parsed = output(&format!(
                "classdef CatalogTest < matlab.unittest.TestCase\n    methods {attributes}\n\
                 \x20       function notATest(testCase)\n        end\n    end\nend\n"
            ));
            assert_eq!(tests_found(&parsed), 0, "{attributes}");
        }
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test, TestTags = {'unit'})\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    /// The header grammar has to accept everything a real test class carries:
    /// class attributes, a multiple-inheritance list, properties with
    /// validation, an `arguments` block, output lists, and `get.` accessors.
    #[test]
    fn a_realistic_class_header_and_body_parse_without_degrading() {
        let parsed = output(
            "classdef (Sealed) CatalogTest < matlab.unittest.TestCase & handle\n\
             \x20   properties (Access = private)\n\
             \x20       Items (1,:) double = [1 2 3]\n\
             \x20   end\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           arguments\n            end\n\
             \x20           testCase.verifyTrue(true);\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20       function sortsCatalog(testCase)\n        end\n\
             \x20   end\n\
             \x20   methods\n\
             \x20       function value = get.Items(testCase)\n            value = 1;\n        end\n\
             \x20       [left, right] = split(testCase, value)\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 3);
        assert!(
            parsed.report.diagnostics.is_empty(),
            "{:?}",
            parsed.report.diagnostics
        );
    }

    #[test]
    fn matching_is_case_sensitive_unlike_the_scanner_languages_before_it() {
        let parsed = output(
            "CLASSDEF CatalogTest < MATLAB.UNITTEST.TESTCASE\n    METHODS (TEST)\n\
             \x20       FUNCTION loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn unbalanced_blocks_and_an_open_block_comment_report_a_degraded_parse() {
        let unbalanced = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n            if true\n        end\n    end\nend\n",
        );
        assert_eq!(unbalanced.report.diagnostics.len(), 1);
        assert_eq!(
            unbalanced.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );

        let open_comment = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n%{\nnever closed\n",
        );
        assert!(open_comment
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
        // The anchor found before the violation is still real: it rests only on
        // headers that precede it.
        assert_eq!(tests_found(&open_comment), 1);
    }

    #[test]
    fn an_unterminated_character_array_reports_a_degraded_parse() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n            s = 'unterminated;\n        end\n\
             \x20   end\nend\n",
        );
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
    }

    /// Repository contents are untrusted. Neither pathological nesting nor
    /// non-ASCII bytes may panic, hang, or produce an anchor.
    #[test]
    fn adversarial_source_degrades_instead_of_overflowing_or_panicking() {
        let deep = format!(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n{}{}        end\n    end\nend\n",
            "            if true\n".repeat(4_000),
            "            end\n".repeat(4_000),
        );
        let parsed = output(&deep);
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
        assert_eq!(
            tests_found(&parsed),
            1,
            "the header before the bound is real"
        );

        let unicode = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           % 目录 ünïcöde コメント\n\
             \x20           s = '目录';\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&unicode), 2);

        // Bare punctuation, unbalanced brackets, and lone keywords must resolve
        // to a bounded outcome rather than an anchor.
        for text in ["end", "]]]", "classdef", "classdef X <", "%{", "'"] {
            let parsed = output(text);
            assert_eq!(tests_found(&parsed), 0, "{text}");
        }
    }

    #[test]
    fn a_well_formed_class_reports_no_diagnostic() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    /// Every abstention names a bounded class and never copies the source that
    /// triggered it.
    #[test]
    fn an_abstention_never_copies_the_divergent_source_text() {
        let parsed = output(&class_with_body("            format end"));
        for fact in &parsed.report.semantic_facts {
            let rendered = format!("{:?}{:?}", fact.assumptions, fact.evidence.note);
            assert!(!rendered.contains("format"), "{rendered}");
        }
        for diagnostic in &parsed.report.diagnostics {
            assert!(!diagnostic.message.contains("format"), "{diagnostic:?}");
        }
    }

    #[test]
    fn only_matlab_sources_are_admitted() {
        let parser = MatlabUnitTestParser;
        assert_eq!(
            parser.parse(SourceDocument {
                path: "Contents.mlx",
                language: Language::Matlab,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                text: "",
            }),
            Err(ParseError::UnsupportedLanguage)
        );
        assert!(is_matlab_source_path("tests/CatalogTest.m"));
    }

    fn pinned_summary(parsed: &SourceParseOutput) -> String {
        let anchors = parsed
            .report
            .units
            .iter()
            .map(|unit| {
                format!(
                    "{}:{}-{}",
                    unit.kind.as_str(),
                    unit.range.start_byte,
                    unit.range.end_byte
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "units[{anchors}] unknown[{}] diagnostics={}",
            unknown_kinds(parsed)
                .iter()
                .map(|kind| format!("matlab_unknown_kind={kind}"))
                .collect::<Vec<_>>()
                .join(","),
            parsed.report.diagnostics.len()
        )
    }

    fn pinned_class(body: &str) -> String {
        format!(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n        function loadsCatalog(testCase)\n{body}\n        end\n        function filtersCatalog(testCase)\n        end\n        function sortsCatalog(testCase)\n        end\n    end\nend\n"
        )
    }

    fn pinned_methods(header: &str, attrs: &str, tail: &str) -> String {
        format!(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods{attrs}\n{header}\n        end\n{tail}    end\nend\n"
        )
    }

    /// Inputs the Tree-sitter frontend reads differently from the retired
    /// hand-written parser by design (ADR-0058, accepted 2026-10-22), with the
    /// current summary pinned. Each is invalid MATLAB the retired parser skipped
    /// or read leniently, except the empty attribute list, which is valid MATLAB
    /// the grammar rejects.
    #[test]
    fn grammar_reading_divergences_are_pinned() {
        let func = "        function f(testCase)\n        end\n";
        let sig = |header: &str| pinned_methods(header, " (Test)", "");
        let cases: Vec<(&str, String, &str)> = vec![
            (
                "empty_attribute_list",
                format!("classdef CatalogTest < matlab.unittest.TestCase\n    methods ()\n{func}{func}    end\nend\n"),
                "units[matlab_test_class:0-48,module:0-157] unknown[] diagnostics=1",
            ),
            (
                "uppercase_function",
                sig("        FUNCTION f(testCase)"),
                "units[matlab_test_class:0-48,module:0-120] unknown[] diagnostics=1",
            ),
            (
                "extra_end_after_class",
                format!("{}end\n", pinned_class("            x = 1;")),
                "units[matlab_test_class:0-48,module:0-260,matlab_test_method:67-107,matlab_test_method:138-180,matlab_test_method:192-232] unknown[] diagnostics=1",
            ),
            (
                "unbalanced_bracket",
                pinned_class("            x = [1 2;"),
                "units[matlab_test_class:0-48,module:0-259] unknown[] diagnostics=1",
            ),
            (
                "unbalanced_brace",
                pinned_class("            x = {1, 2;"),
                "units[matlab_test_class:0-48,module:0-260] unknown[] diagnostics=1",
            ),
            (
                "mismatched_bracket",
                pinned_class("            x = (1 + 2];"),
                "units[matlab_test_class:0-48,module:0-262,matlab_test_method:67-107] unknown[] diagnostics=1",
            ),
            (
                "classdef_dangling_lt",
                "classdef A <\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
                "units[module:0-85] unknown[] diagnostics=1",
            ),
            (
                "classdef_dangling_amp",
                "classdef A < handle &\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
                "units[module:0-94] unknown[] diagnostics=1",
            ),
            (
                "function_without_name",
                pinned_methods("        function = f", " (Test)", func),
                "units[matlab_test_class:0-48,module:0-153,matlab_test_method:67-88] unknown[] diagnostics=1",
            ),
            (
                "properties_missing_end",
                "classdef A < matlab.unittest.TestCase\n    properties\n        x\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
                "units[matlab_test_class:0-38,module:0-135] unknown[] diagnostics=1",
            ),
        ];
        for (name, source, expected) in cases {
            assert_eq!(pinned_summary(&output(&source)), expected, "{name}");
        }
    }
}
