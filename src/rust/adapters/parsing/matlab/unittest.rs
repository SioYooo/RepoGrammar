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
    parse_output_using(document, tree_sitter_matlab_file)
}

/// The syntax layer is a parameter only so the test-only differential can run
/// the retired hand-written parser through the same downstream code.
fn parse_output_using(
    document: SourceDocument<'_>,
    parse: impl FnOnce(&str, &Lexed) -> Result<FileParse, ParseError>,
) -> Result<SourceParseOutput, ParseError> {
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
        None => parse(document.text, &lexed)?,
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
// Legacy hand-written parser (test-only differential oracle, ADR-0058)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod legacy_parser {
    use super::*;

    /// Statement keywords that open a block terminated by `end` inside a function
    /// body. `methods`, `properties`, `events`, `enumeration`, and `arguments` are
    /// deliberately absent: they are contextual, and treating them as unconditional
    /// openers invents a block for `methods = getMethods(x);`.
    const STATEMENT_BLOCK_OPENERS: &[&str] = &[
        "if", "for", "while", "switch", "try", "parfor", "spmd", "function",
    ];

    /// Keywords that continue an already-open block rather than opening a new one.
    const BLOCK_CONTINUATIONS: &[&str] = &["elseif", "else", "case", "otherwise", "catch"];

    /// How a descent ended.
    enum Halt {
        /// The token stream does not match the block grammar.
        Structural,
        /// A construct outside the declared subset. The file abstains.
        Outside(Abstention),
    }

    type Step = Result<(), Halt>;

    struct Parser<'a> {
        text: &'a str,
        tokens: &'a [Token],
        index: usize,
        /// Descent depth, bounded because repository contents are untrusted: a file
        /// of ten thousand nested `if`s must degrade, not overflow the stack.
        depth: usize,
        out: FileParse,
    }

    pub(super) fn parse_tokens(text: &str, tokens: &[Token]) -> FileParse {
        let mut parser = Parser {
            text,
            tokens,
            index: 0,
            depth: 0,
            out: FileParse::default(),
        };
        match parser.parse_file() {
            Ok(()) => {}
            Err(Halt::Structural) => parser.out.structural_failure = true,
            Err(Halt::Outside(abstention)) => {
                parser.out.abstention = Some(abstention);
                parser.out.anchors.clear();
                parser.out.unbound_test_block = false;
                parser.out.observations.clear();
            }
        }
        parser.out
    }

    impl<'a> Parser<'a> {
        fn peek(&self) -> Option<&Token> {
            self.tokens.get(self.index)
        }

        fn word(&self, token: &Token) -> &'a str {
            &self.text[token.start..token.end]
        }

        fn peek_word(&self) -> Option<&'a str> {
            self.peek().and_then(|token| {
                (token.kind == TokenKind::Word).then(|| &self.text[token.start..token.end])
            })
        }

        fn skip_separators(&mut self) {
            while self.peek().is_some_and(|token| {
                matches!(token.kind, TokenKind::Separator | TokenKind::LineBreak)
            }) {
                self.index += 1;
            }
        }

        /// The extent of the statement beginning at `self.index`, ending at the
        /// first `;`, `,`, or line break outside brackets. A line break inside `[]`
        /// or `{}` is a row separator, not a statement end.
        fn statement_span(&self) -> StatementSpan {
            let mut depth = 0usize;
            let mut cursor = self.index;
            let mut balanced = true;
            let mut depth_zero_end = None;
            while cursor < self.tokens.len() {
                let token = self.tokens[cursor];
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
                    TokenKind::Word if depth == 0 && self.word(&token) == "end" => {
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

        /// ADR-0046 D1: a `classdef` must be the first construct in its file, so a
        /// file that does not open with one can never carry the admitted anchor and
        /// receives no structural claim at all.
        fn parse_file(&mut self) -> Step {
            self.skip_separators();
            if self.peek_word() != Some("classdef") {
                return Ok(());
            }
            self.parse_classdef()
        }

        fn parse_classdef(&mut self) -> Step {
            let header = *self.peek().ok_or(Halt::Structural)?;
            let span = self.statement_span();
            if !span.balanced {
                return Err(Halt::Structural);
            }
            let derives_test_case = self.classdef_derives_test_case(self.index + 1, span.end)?;
            if derives_test_case {
                let (start, end) = line_bounds(self.text, header.start);
                self.out.anchors.push(Anchor {
                    kind: AnchorKind::TestClass,
                    start,
                    end,
                });
            }
            self.index = span.end;

            loop {
                self.skip_separators();
                let Some(token) = self.peek().copied() else {
                    return Err(Halt::Structural);
                };
                if token.kind != TokenKind::Word {
                    return Err(Halt::Outside(unadmitted_classdef_construct()));
                }
                match self.word(&token) {
                    "end" => {
                        self.index += 1;
                        return Ok(());
                    }
                    "methods" => self.parse_methods_block(derives_test_case)?,
                    "properties" | "events" | "enumeration" => self.parse_opaque_block()?,
                    _ => return Err(Halt::Outside(unadmitted_classdef_construct())),
                }
            }
        }

        /// `classdef [ (attrs) ] Name [ < Base [ & Base ]* ]`. Attributes are parsed
        /// so the header is exact; ADR-0046 D2b keeps `Abstract` out of the claim.
        fn classdef_derives_test_case(&self, mut cursor: usize, end: usize) -> Result<bool, Halt> {
            if self
                .tokens
                .get(cursor)
                .is_some_and(|token| token.byte == b'(')
            {
                cursor = self.skip_bracketed(cursor, end)?;
            }
            // The class name.
            let name = self.tokens.get(cursor).ok_or(Halt::Structural)?;
            if name.kind != TokenKind::Word {
                return Err(Halt::Structural);
            }
            cursor += 1;
            let Some(separator) = self.tokens.get(cursor) else {
                return Ok(false);
            };
            if separator.kind != TokenKind::Symbol || separator.byte != b'<' || cursor >= end {
                return Ok(false);
            }
            cursor += 1;
            let mut derives = false;
            while cursor < end {
                let (base, next) = self.read_dotted_name(cursor, end)?;
                if base == TEST_CASE_BASE {
                    derives = true;
                }
                cursor = next;
                match self.tokens.get(cursor) {
                    Some(token) if token.byte == b'&' && cursor < end => cursor += 1,
                    _ => break,
                }
            }
            Ok(derives)
        }

        fn read_dotted_name(&self, mut cursor: usize, end: usize) -> Result<(String, usize), Halt> {
            let mut name = String::new();
            loop {
                let token = self.tokens.get(cursor).ok_or(Halt::Structural)?;
                if cursor >= end || token.kind != TokenKind::Word {
                    return Err(Halt::Structural);
                }
                name.push_str(self.word(token));
                cursor += 1;
                match self.tokens.get(cursor) {
                    Some(dot)
                        if dot.kind == TokenKind::Symbol && dot.byte == b'.' && cursor < end =>
                    {
                        name.push('.');
                        cursor += 1;
                    }
                    _ => return Ok((name, cursor)),
                }
            }
        }

        fn skip_bracketed(&self, cursor: usize, end: usize) -> Result<usize, Halt> {
            let mut depth = 0usize;
            let mut index = cursor;
            while index < end {
                match self.tokens[index].kind {
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
            Err(Halt::Structural)
        }

        /// Attribute names of a `( … )` list, each paired with whether it carried a
        /// `= value`.
        fn attribute_list(&self, cursor: usize, end: usize) -> Vec<(&'a str, bool)> {
            let mut attributes = Vec::new();
            let mut depth = 0usize;
            let mut index = cursor;
            let mut pending: Option<&'a str> = None;
            let mut valued = false;
            while index < end {
                let token = self.tokens[index];
                match token.kind {
                    TokenKind::Open => depth += 1,
                    TokenKind::Close => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            break;
                        }
                    }
                    TokenKind::Word if depth == 1 && pending.is_none() => {
                        pending = Some(self.word(&token));
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

        fn parse_methods_block(&mut self, derives_test_case: bool) -> Step {
            let span = self.statement_span();
            if !span.balanced {
                return Err(Halt::Structural);
            }
            let mut is_test = false;
            let mut valued_test = false;
            let mut is_abstract = false;
            if self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| token.byte == b'(')
            {
                for (name, valued) in self.attribute_list(self.index + 1, span.end) {
                    match name {
                        "Test" if valued => valued_test = true,
                        "Test" => is_test = true,
                        "Abstract" => is_abstract = true,
                        _ => {}
                    }
                }
            }
            self.index = span.end;

            if valued_test && !is_test {
                // `methods (Test = <expr>)` needs the expression evaluated to know
                // whether the block is a test block, and this frontend evaluates
                // nothing.
                self.out.observations.push(Abstention {
                    reason: UnknownReasonCode::InsufficientSupport,
                    claim: "matlab_test_attribute_value",
                    kind: "test_attribute_with_value",
                    note: "a methods block sets the Test attribute to an expression, which cannot be read without evaluating it",
                });
            }
            if is_test && !derives_test_case {
                self.out.unbound_test_block = true;
            }
            if is_test && is_abstract {
                // An abstract method has no implementation, so the declaring class
                // declares no runnable test.
                self.out.observations.push(Abstention {
                    reason: UnknownReasonCode::InsufficientSupport,
                    claim: "matlab_abstract_test_methods",
                    kind: "abstract_test_methods_block",
                    note: "a Test methods block is declared Abstract, so its members are signatures rather than test implementations",
                });
            }
            let anchors_tests = is_test && derives_test_case && !is_abstract;

            loop {
                self.skip_separators();
                let Some(token) = self.peek().copied() else {
                    return Err(Halt::Structural);
                };
                if token.kind == TokenKind::Word && self.word(&token) == "end" {
                    self.index += 1;
                    return Ok(());
                }
                if token.kind == TokenKind::Word && self.word(&token) == "function" {
                    self.parse_function(anchors_tests)?;
                    continue;
                }
                // A bare method signature: the body lives in an `@Class` folder or
                // the block is abstract. It declares no implementation and never
                // anchors.
                let span = self.statement_span();
                if !span.balanced || span.depth_zero_end.is_some() {
                    return Err(Halt::Structural);
                }
                if span.end == self.index {
                    return Err(Halt::Structural);
                }
                self.index = span.end;
            }
        }

        /// `properties`, `events`, and `enumeration` bodies hold declarations, not
        /// statements. They are consumed for their extent only.
        fn parse_opaque_block(&mut self) -> Step {
            let span = self.statement_span();
            if !span.balanced {
                return Err(Halt::Structural);
            }
            self.index = span.end;
            loop {
                self.skip_separators();
                let Some(token) = self.peek().copied() else {
                    return Err(Halt::Structural);
                };
                if token.kind == TokenKind::Word && self.word(&token) == "end" {
                    self.index += 1;
                    return Ok(());
                }
                let span = self.statement_span();
                if !span.balanced || span.depth_zero_end.is_some() || span.end == self.index {
                    return Err(Halt::Structural);
                }
                self.index = span.end;
            }
        }

        /// `function [out, …] = name[.accessor](args)`, then a statement region,
        /// then `end`.
        fn parse_function(&mut self, anchors_tests: bool) -> Step {
            let keyword = *self.peek().ok_or(Halt::Structural)?;
            let span = self.statement_span();
            if !span.balanced || span.depth_zero_end.is_some() {
                return Err(Halt::Structural);
            }
            self.parse_function_header(self.index + 1, span.end)?;
            self.index = span.end;
            if anchors_tests {
                let (start, end) = line_bounds(self.text, keyword.start);
                self.out.anchors.push(Anchor {
                    kind: AnchorKind::TestMethod,
                    start,
                    end,
                });
            }
            self.parse_statement_region(true)?;
            self.expect_end()
        }

        fn parse_function_header(&self, mut cursor: usize, end: usize) -> Step {
            let assignment =
                (cursor..end).find(|index| self.tokens[*index].kind == TokenKind::Assign);
            if let Some(assignment) = assignment {
                cursor = assignment + 1;
            }
            // The function name, optionally a `get.`/`set.` property accessor.
            let (_, next) = self.read_dotted_name(cursor, end)?;
            cursor = next;
            if cursor < end {
                let token = self.tokens.get(cursor).ok_or(Halt::Structural)?;
                if token.byte != b'(' {
                    return Err(Halt::Structural);
                }
                cursor = self.skip_bracketed(cursor, end)?;
            }
            if cursor < end {
                return Err(Halt::Structural);
            }
            Ok(())
        }

        fn expect_end(&mut self) -> Step {
            self.skip_separators();
            match self.peek().copied() {
                Some(token) if token.kind == TokenKind::Word && self.word(&token) == "end" => {
                    self.index += 1;
                    Ok(())
                }
                _ => Err(Halt::Structural),
            }
        }

        /// A run of statements up to the `end` that closes the enclosing block. The
        /// parser recognises block structure and deliberately leaves expressions
        /// unparsed; that opacity is what D4b's command-syntax bound protects.
        fn parse_statement_region(&mut self, function_body: bool) -> Step {
            if self.depth >= MAX_BLOCK_DEPTH {
                return Err(Halt::Structural);
            }
            self.depth += 1;
            let region = self.statement_region_body(function_body);
            self.depth -= 1;
            region
        }

        fn statement_region_body(&mut self, function_body: bool) -> Step {
            let mut first_statement = function_body;
            loop {
                self.skip_separators();
                let Some(token) = self.peek().copied() else {
                    return Ok(());
                };
                if token.kind == TokenKind::Word {
                    let word = self.word(&token);
                    if word == "end" {
                        return Ok(());
                    }
                    if STATEMENT_BLOCK_OPENERS.contains(&word) {
                        self.parse_nested_block(word)?;
                        first_statement = false;
                        continue;
                    }
                    if word == "arguments" && first_statement && self.opens_arguments_block() {
                        self.parse_nested_block(word)?;
                        continue;
                    }
                    if BLOCK_CONTINUATIONS.contains(&word) {
                        let span = self.statement_span();
                        if !span.balanced || span.depth_zero_end.is_some() {
                            return Err(Halt::Structural);
                        }
                        self.index = span.end;
                        first_statement = false;
                        continue;
                    }
                }
                self.parse_simple_statement()?;
                first_statement = false;
            }
        }

        /// `arguments` is a keyword only as a function's leading block; elsewhere it
        /// is an ordinary name, so `arguments = 3` opens nothing.
        fn opens_arguments_block(&self) -> bool {
            !self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| token.kind == TokenKind::Assign)
        }

        fn parse_nested_block(&mut self, word: &str) -> Step {
            if word == "function" {
                return self.parse_function(false);
            }
            let span = self.statement_span();
            if !span.balanced || span.depth_zero_end.is_some() {
                return Err(Halt::Structural);
            }
            self.index = span.end;
            self.parse_statement_region(false)?;
            self.expect_end()
        }

        fn parse_simple_statement(&mut self) -> Step {
            let span = self.statement_span();
            if !span.balanced {
                if self.possible_command_syntax() {
                    return Err(Halt::Outside(command_abstention(
                        "command_syntax_bracket_balance",
                    )));
                }
                return Err(Halt::Structural);
            }
            if let Some(position) = span.depth_zero_end {
                if self.possible_command_syntax() {
                    // Under the command reading `end` is an unquoted argument; under
                    // the expression reading it closes a block. Nothing in the file
                    // decides which, so the block extents are unproven.
                    return Err(Halt::Outside(command_abstention(
                        "command_syntax_end_keyword",
                    )));
                }
                if position != self.index {
                    return Err(Halt::Structural);
                }
            }
            if span.end == self.index {
                return Err(Halt::Structural);
            }
            self.index = span.end;
            Ok(())
        }

        /// The over-approximating command-syntax trigger: an unreserved identifier
        /// at statement start, a blank, and something that is neither an assignment
        /// nor a call's `(`. Over-detection only adds a divergence check;
        /// under-detection would miscount an `end`.
        fn possible_command_syntax(&self) -> bool {
            let Some(head) = self.peek() else {
                return false;
            };
            if head.kind != TokenKind::Word || RESERVED_WORDS.contains(&self.word(head)) {
                return false;
            }
            let Some(next) = self.tokens.get(self.index + 1) else {
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
    }
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
}

/// Temporary ADR-0058 differential: the retired hand-written parser against the
/// Tree-sitter frontend, through the same downstream code. Deleted together
/// with the legacy parser once the maintainer accepts the verdict.
#[cfg(test)]
mod differential {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn document(text: &str) -> SourceDocument<'_> {
        SourceDocument {
            path: "tests/CatalogTest.m",
            language: Language::Matlab,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn grammar_is_clean(text: &str) -> bool {
        let lexed = lex(text);
        let mut parser = TreeParser::new();
        parser
            .set_language(&tree_sitter_matlab::LANGUAGE.into())
            .expect("grammar");
        !parser
            .parse(grammar_view(text, &lexed).as_bytes(), None)
            .expect("parse")
            .root_node()
            .has_error()
    }

    fn new_output(text: &str) -> Result<SourceParseOutput, ParseError> {
        parse_output_using(document(text), tree_sitter_matlab_file)
    }

    fn legacy_output(text: &str) -> Result<SourceParseOutput, ParseError> {
        parse_output_using(document(text), |text, lexed| {
            Ok(legacy_parser::parse_tokens(text, &lexed.tokens))
        })
    }

    fn summary(output: &Result<SourceParseOutput, ParseError>) -> String {
        match output {
            Err(error) => format!("error {error:?}"),
            Ok(output) => {
                let anchors = output
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
                let unknowns = output
                    .report
                    .semantic_facts
                    .iter()
                    .flat_map(|fact| fact.assumptions.iter())
                    .filter(|a| a.starts_with("matlab_unknown_kind="))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "units[{anchors}] unknown[{unknowns}] diagnostics={}",
                    output.report.diagnostics.len()
                )
            }
        }
    }

    fn class_with(body: &str) -> String {
        format!(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n        function loadsCatalog(testCase)\n{body}\n        end\n        function filtersCatalog(testCase)\n        end\n        function sortsCatalog(testCase)\n        end\n    end\nend\n"
        )
    }

    fn head(header: &str, rest: &str) -> String {
        format!("{header}\n{rest}end\n")
    }

    const BASE: &str = "classdef CatalogTest < matlab.unittest.TestCase";
    const FUNC: &str = "        function f(testCase)\n        end\n";

    fn methods(attrs: &str, body: &str) -> String {
        format!("    methods{attrs}\n{body}    end\n")
    }

    /// Sources both frontends must read identically: every anchor variation,
    /// every refusal class of the ADR-0046 subset, and near-misses. Inputs whose
    /// only difference is a syntax error in a region the legacy parser skipped
    /// belong to `declared_divergence`, not here.
    fn branch_corpus() -> Vec<(String, String)> {
        let mut corpus: Vec<(String, String)> = Vec::new();
        let mut add = |name: &str, text: String| corpus.push((name.to_string(), text));

        // -- anchors and anchor variations --------------------------------
        add("empty", String::new());
        add("blank", "\n\n   \n".to_string());
        add("lone_end", "end".to_string());
        add("lone_brackets", "]]]".to_string());
        add("lone_classdef", "classdef".to_string());
        add("classdef_dangling_lt", "classdef X <".to_string());
        add("lone_percent_brace", "%{".to_string());
        add("lone_tick", "'".to_string());
        add("empty_class", "classdef A\nend\n".to_string());
        add("empty_test_class", format!("{BASE}\nend\n"));
        add("empty_methods", head(BASE, &methods(" (Test)", "")));
        add(
            "three_tests",
            head(BASE, &methods(" (Test)", &format!("{FUNC}{FUNC}{FUNC}"))),
        );
        for (name, header) in [
            (
                "base_first",
                "classdef A < matlab.unittest.TestCase & handle",
            ),
            (
                "base_second",
                "classdef A < handle & matlab.unittest.TestCase",
            ),
            ("base_tight", "classdef A < matlab.unittest.TestCase&handle"),
            (
                "base_spaced_dots",
                "classdef A < matlab . unittest . TestCase",
            ),
            ("base_other", "classdef A < handle"),
            ("base_prefix", "classdef A < matlab.unittest.TestCase2"),
            ("base_suffix", "classdef A < xmatlab.unittest.TestCase"),
            ("base_case", "classdef A < MATLAB.UNITTEST.TESTCASE"),
            ("base_none", "classdef A"),
            ("base_short", "classdef A < TestCase"),
            (
                "class_attr_abstract",
                "classdef (Abstract) A < matlab.unittest.TestCase",
            ),
            (
                "class_attr_two",
                "classdef (Sealed, Hidden) A < matlab.unittest.TestCase",
            ),
            (
                "class_attr_valued",
                "classdef (ConstructOnLoad = true) A < matlab.unittest.TestCase",
            ),
            (
                "header_comment",
                "classdef A < matlab.unittest.TestCase % comment",
            ),
            ("header_semicolon", "classdef A < matlab.unittest.TestCase;"),
            ("upper_classdef", "CLASSDEF A < matlab.unittest.TestCase"),
            (
                "base_three",
                "classdef A < handle & matlab.mixin.Copyable & matlab.unittest.TestCase",
            ),
        ] {
            add(
                name,
                head(header, &methods(" (Test)", &format!("{FUNC}{FUNC}"))),
            );
        }
        for (name, attrs) in [
            ("m_test", " (Test)"),
            ("m_test_access", " (Test, Access = public)"),
            ("m_access_test", " (Access = public, Test)"),
            ("m_test_valued", " (Test = true)"),
            ("m_test_valued_false", " (Test = false)"),
            ("m_test_valued_abstract", " (Test = true, Abstract)"),
            ("m_test_abstract", " (Test, Abstract)"),
            ("m_abstract_test", " (Abstract, Test)"),
            ("m_abstract", " (Abstract)"),
            ("m_abstract_valued", " (Abstract = true, Test)"),
            ("m_access", " (Access = private)"),
            ("m_negated", " (~Test)"),
            ("m_setup", " (TestClassSetup)"),
            ("m_method_setup", " (TestMethodSetup)"),
            ("m_static", " (Static)"),
            ("m_test_static", " (Test, Static)"),
            ("m_test_expr", " (Test = ~isempty(x))"),
            ("m_test_tags", " (Test, TestTags = {'a', 'b'})"),
            ("m_none", ""),
            ("m_tight", "(Test)"),
            ("m_empty_attrs", " ()"),
            ("m_attr_comment", " (Test) % comment"),
            ("m_attr_semicolon", " (Test);"),
            ("m_upper", " (TEST)"),
            ("m_test_lower", " (test)"),
            ("m_access_value_is_test", " (Access = Test)"),
            ("m_access_cell_test", " (Access = {Test})"),
            ("m_test_hidden", " (Test, Hidden)"),
            (
                "m_attr_continuation",
                " (Test, ...\n        Access = public)",
            ),
        ] {
            add(name, head(BASE, &methods(attrs, &format!("{FUNC}{FUNC}"))));
        }
        add(
            "two_test_blocks",
            head(
                BASE,
                &format!("{}{}", methods(" (Test)", FUNC), methods(" (Test)", FUNC)),
            ),
        );
        add(
            "mixed_blocks",
            head(
                BASE,
                &format!(
                    "{}{}{}",
                    methods(" (TestClassSetup)", FUNC),
                    methods(" (Test)", FUNC),
                    methods(" (Access = private)", FUNC)
                ),
            ),
        );
        add(
            "all_block_kinds",
            head(
                BASE,
                "    properties\n        x = 1\n        y double = 3\n    end\n    properties (Access = private)\n        z\n    end\n    events\n        Changed\n    end\n    enumeration\n        Red, Green\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        add(
            "properties_validation",
            head(
                BASE,
                "    properties\n        a (1,1) double {mustBePositive} = 3\n        b (1,:) char = 'x'\n        c = [1 2 3]\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        add(
            "enumeration_args",
            head(
                BASE,
                "    enumeration\n        Red (1, 2)\n        Green (3, 4)\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        for (name, header) in [
            ("fn_varargout", "        function varargout = f(testCase)"),
            ("fn_multi_out", "        function [a, b] = f(testCase)"),
            ("fn_multi_out_tight", "        function [a,b]=f(testCase)"),
            ("fn_no_parens", "        function f"),
            ("fn_empty_parens", "        function f()"),
            ("fn_varargin", "        function f(testCase, varargin)"),
            ("fn_getter", "        function get.Prop(obj)"),
            ("fn_setter", "        function set.Prop(obj, v)"),
            ("fn_tilde", "        function f(~)"),
            ("fn_tilde_two", "        function f(~, x)"),
            ("fn_comment", "        function f(testCase) % comment"),
            ("fn_semicolon", "        function f(testCase);"),
            ("fn_comma", "        function f(testCase),"),
            (
                "fn_continuation",
                "        function f(testCase, ...\n            other)",
            ),
            ("fn_dotted", "        function a.b(testCase)"),
            ("fn_out_single", "        function out = f(testCase)"),
            ("fn_empty_out", "        function [] = f(testCase)"),
            ("fn_default_args", "        function f(testCase, x)"),
            ("fn_upper", "        FUNCTION f(testCase)"),
        ] {
            add(
                name,
                head(
                    BASE,
                    &methods(" (Test)", &format!("{header}\n        end\n")),
                ),
            );
        }
        let bodies: &[(&str, &str)] = &[
            ("b_assign", "            x = 1;"),
            ("b_if", "            if true\n                x = 1;\n            end"),
            ("b_if_else", "            if a\n x = 1;\n elseif b\n x = 2;\n else\n x = 3;\n end"),
            ("b_if_inline", "            if true, x = 1; end"),
            ("b_for", "            for i = 1:3\n                disp(i);\n            end"),
            ("b_for_inline", "            for k=1:3 disp(k), end"),
            ("b_parfor", "            parfor i = 1:3\n                x(i) = i;\n            end"),
            ("b_while", "            while x < 3\n                x = x + 1;\n            end"),
            ("b_switch", "            switch x\n                case 1\n                    y = 1;\n                case {2, 3}\n                    y = 2;\n                otherwise\n                    y = 3;\n            end"),
            ("b_try", "            try\n                f();\n            catch ME\n                g(ME);\n            end"),
            ("b_try_plain", "            try\n                f();\n            catch\n                g();\n            end"),
            ("b_spmd", "            spmd\n                x = 1;\n            end"),
            ("b_nested_blocks", "            for i = 1:3\n                if i > 1\n                    while true\n                        break\n                    end\n                end\n            end"),
            ("b_nested_function", "            function inner()\n                x = 1;\n            end\n            inner();"),
            ("b_end_index", "            v = x(end);\n            w = x(end+1);\n            z = x(2:end);"),
            ("b_end_index_nested", "            q = x{end}(end);"),
            ("b_end_string", "            s = 'end';\n            t = \"end\";"),
            ("b_end_comment", "            % end\n            x = 1; % end"),
            ("b_block_comment", "            %{\n            end\n            function bad(\n            %}\n            x = 1;"),
            ("b_continuation", "            x = [1 2 ...\n                 3];\n            y = a + ...\n                b;"),
            ("b_continuation_comment", "            x = 1 + ... comment end\n                2;"),
            ("b_matrix_rows", "            m = [1 2\n                 3 4];"),
            ("b_cell", "            c = {1, 'a'; 2, 'b'};"),
            ("b_transpose", "            v = [a' b'];\n            w = a.';\n            x = (a + b)';"),
            ("b_transpose_chain", "            y = a'';"),
            ("b_string_quotes", "            s = 'it''s';\n            t = \"say \"\"hi\"\"\";"),
            ("b_concat_string", "            w = [a 'end'];"),
            ("b_lambda", "            f = @(x) x + 1;\n            g = @sin;"),
            ("b_global", "            global G\n            persistent P"),
            ("b_return", "            if x\n                return\n            end"),
            ("b_break_continue", "            for i = 1:3\n                if i == 2, continue, end\n                break\n            end"),
            ("b_arguments_block", "            arguments\n                x (1,1) double = 3\n            end\n            y = x;"),
            ("b_arguments_var", "            arguments = 3;\n            y = arguments;"),
            ("b_methods_var", "            methods = getMethods(x);"),
            ("b_properties_var", "            properties = 1;"),
            ("b_function_in_string", "            s = 'function bad(x)';"),
            ("b_function_in_comment", "            % function inAComment(testCase)\n            x = 1;"),
            ("b_command_plain", "            format long\n            hold on\n            clc"),
            ("b_command_dash", "            warning off all\n            close all"),
            ("b_command_semicolon", "            format long;"),
            ("b_call_space", "            disp ('x')\n            foo (1)"),
            ("b_unary_minus_cmd", "            a -1\n            b - 1"),
            ("b_assert", "            testCase.verifyTrue(true);\n            testCase.verifyEqual(a, b, 'AbsTol', 1e-6);"),
            ("b_numbers", "            x = 1e-3 + 0x1F + 3i + .5 + 5. + 1.5e+10;"),
            ("b_ranges", "            r = 1:2:10;"),
            ("b_logical", "            t = a && b || ~c;"),
            ("b_dot_ops", "            y = a.*b + c./d - e.^2;"),
            ("b_struct", "            s.a.b = 1;\n            s(2).a = 3;"),
            ("b_if_comma_cmd", "            if x, disp hello, end"),
            ("b_empty_line_run", "\n\n            x = 1;\n\n"),
            ("b_unicode", "            % 目录 ünïcöde\n            s = '目录';"),
            ("b_tab_indent", "\t\t\tx = 1;"),
            ("b_cmd_in_if", "            if x\n                format long\n            end"),
            ("b_cmd_dot_args", "            cd ..\n            addpath ./lib"),
            ("b_cmd_then_end", "            format long\n        "),
        ];
        for (name, body) in bodies {
            add(name, class_with(body));
        }
        add(
            "local_function_after",
            format!(
                "{}\nfunction localHelper()\nend\n",
                class_with("            x = 1;")
            ),
        );
        add(
            "local_function_with_end_after",
            format!(
                "{}\nfunction localHelper()\n    if x\n        y\n    end\nend\n",
                class_with("            x = 1;")
            ),
        );
        add(
            "trailing_comment_after",
            format!("{}% done\n", class_with("            x = 1;")),
        );
        add(
            "second_classdef_ignored",
            format!("{}classdef B\nend\n", class_with("            x = 1;")),
        );
        add(
            "comment_before_classdef",
            format!("% header\n% more\n{}", class_with("            x = 1;")),
        );
        add(
            "blank_before_classdef",
            format!("\n\n{}", class_with("            x = 1;")),
        );
        add(
            "block_comment_before_classdef",
            format!("%{{\nlicense\n%}}\n{}", class_with("            x = 1;")),
        );
        add(
            "function_file",
            "function y = f(x)\n    y = x;\nend\n".to_string(),
        );
        add("script_file", "x = 1;\ny = 2;\ndisp(x + y)\n".to_string());
        add(
            "function_then_classdef",
            format!("function f()\nend\n{}", class_with("            x = 1;")),
        );
        add(
            "crlf",
            class_with("            x = 1;").replace('\n', "\r\n"),
        );
        add(
            "tabs_and_spaces",
            class_with("\t x = 1;").replace("    ", "\t"),
        );
        add(
            "bare_signature_abstract",
            head(
                BASE,
                "    methods (Abstract)\n        r = compute(obj, x)\n        other(obj)\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        add(
            "bare_signature_with_body_mix",
            head(
                BASE,
                "    methods (Test)\n        r = declared(obj)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        add(
            "no_semicolon_class_end",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend".to_string(),
        );
        add(
            "end_same_line_class",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase), end\n    end\nend\n".to_string(),
        );
        add(
            "one_line_function",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase) x = 1; end\n    end\nend\n".to_string(),
        );
        add(
            "unbound_test_block_other_base",
            head(
                "classdef A < handle",
                &methods(" (Test)", &format!("{FUNC}{FUNC}")),
            ),
        );
        add(
            "unbound_test_block_no_base",
            head("classdef A", &methods(" (Test)", FUNC)),
        );
        add(
            "unbound_valued_no_base",
            head("classdef A", &methods(" (Test = true)", FUNC)),
        );
        add(
            "end_inside_function_header_default",
            head(
                BASE,
                &methods(" (Test)", "        function f(testCase, x)\n        end\n"),
            ),
        );

        // -- lexical (dialect) refusals ------------------------------------
        for (name, body) in [
            ("d_hash_comment", "            # octave comment\n            x = 1;"),
            ("d_hash_trailing", "            x = 1; # trailing"),
            ("d_bang", "            !ls"),
            ("d_bang_negation", "            y = !x;"),
            ("d_backslash_cont", "            x = 1 + \\\n 2;"),
            ("d_endif", "            if x\n                y = 1;\n            endif"),
            ("d_endfor", "            for i = 1:3\n            endfor"),
            ("d_endfunction", "            endfunction"),
            ("d_unwind", "            unwind_protect\n            unwind_protect_cleanup\n            end_unwind_protect"),
            ("d_escaped_dq", "            s = \"a\\\"b\";"),
            ("d_backslash_in_sq", "            s = 'a\\b';"),
            ("d_end_try_catch", "            try\n            catch\n            end_try_catch"),
        ] {
            add(name, class_with(body));
        }
        add(
            "d_hash_before_classdef",
            format!("# octave\n{}", class_with("            x = 1;")),
        );
        add(
            "d_hash_after_class",
            format!("{}# trailing\n", class_with("            x = 1;")),
        );
        add(
            "d_hash_in_properties",
            head(
                BASE,
                "    properties\n        x = 1; # note\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );

        // -- command-syntax ambiguity (D4b) ---------------------------------
        for (name, body) in [
            ("c_end_arg", "            format end"),
            ("c_end_arg_hold", "            hold end"),
            ("c_end_arg_two", "            foo bar end"),
            ("c_end_arg_nested_if", "            if x\n                format end\n            end"),
            ("c_end_arg_after_comma", "            x = 1; format end"),
            ("c_end_arg_inline_if", "            if true, format end"),
            ("c_bracket_open", "            disp ["),
            ("c_bracket_open_paren", "            disp ("),
            ("c_bracket_close", "            disp ]"),
            ("c_bracket_arg_brace", "            hold {"),
            ("c_bracket_balanced", "            disp [1 2]"),
            ("c_end_in_paren", "            disp (end)"),
            ("c_end_in_index", "            disp x(end)"),
            ("c_end_in_bracket", "            disp [end]"),
            ("c_end_then_more", "            format end\n            x = 1;"),
            ("c_end_arg_in_for", "            for i = 1:3\n                format end\n            end"),
            ("c_arguments_cmd", "            arguments x end"),
            ("c_end_arg_after_nested_function", "            function g()\n                format end\n            end"),
            ("c_end_arg_in_switch", "            switch x\n                case 1\n                    format end\n            end"),
            ("c_end_arg_in_try", "            try\n                format end\n            catch\n            end"),
        ] {
            add(name, class_with(body));
        }
        add(
            "c_end_arg_in_second_function",
            head(
                BASE,
                "    methods (Test)\n        function a(testCase)\n        end\n        function b(testCase)\n            format end\n        end\n    end\n",
            ),
        );
        add(
            "c_end_in_non_test_methods",
            head(
                BASE,
                "    methods (Test)\n        function a(testCase)\n        end\n    end\n    methods\n        function helper(obj)\n            format end\n        end\n    end\n",
            ),
        );
        add(
            "c_local_function_not_read",
            format!(
                "{}\nfunction local()\n    format end\nend\n",
                class_with("            x = 1;")
            ),
        );

        // -- classdef body shape ----------------------------------------
        for (name, body) in [
            ("o_assignment", "    foo = 3\n"),
            ("o_word", "    foo\n"),
            ("o_number", "    123\n"),
            ("o_string", "    'text'\n"),
            ("o_bracket", "    ]\n"),
            ("o_stray_function", "    function f()\n    end\n"),
            ("o_stray_if", "    if x\n    end\n"),
            ("o_call", "    foo(1)\n"),
            ("o_classdef_nested", "    classdef B\n    end\n"),
            ("o_arguments", "    arguments\n    end\n"),
        ] {
            add(
                name,
                head(BASE, &format!("{}{body}", methods(" (Test)", FUNC))),
            );
            add(
                &format!("{name}_first"),
                head(BASE, &format!("{body}{}", methods(" (Test)", FUNC))),
            );
        }
        add(
            "o_after_properties",
            head(BASE, "    properties\n        x\n    end\n    foo = 1\n    methods (Test)\n        function t(testCase)\n        end\n    end\n"),
        );

        // -- structural failures --------------------------------------
        add(
            "s_function_missing_end",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase)\n            x = 1;\n    end\nend\n".to_string(),
        );
        add(
            "s_methods_missing_end",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase)\n        end\nend\n".to_string(),
        );
        add(
            "s_class_missing_end",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase)\n        end\n    end\n".to_string(),
        );
        add(
            "s_if_missing_end",
            class_with("            if true\n                x = 1;"),
        );
        add(
            "s_extra_end_in_body",
            class_with("            x = 1;\n            end"),
        );
        add(
            "s_extra_end_after_class",
            format!("{}end\n", class_with("            x = 1;")),
        );
        add("s_unbalanced_paren", class_with("            x = (1 + 2;"));
        add("s_unbalanced_close", class_with("            x = 1);"));
        add("s_unbalanced_bracket", class_with("            x = [1 2;"));
        add("s_unbalanced_brace", class_with("            x = {1, 2;"));
        add("s_mismatched", class_with("            x = (1 + 2];"));
        add(
            "s_methods_attr_unbalanced",
            "classdef A < matlab.unittest.TestCase\n    methods (Test\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add("s_classdef_no_name", "classdef\nend\n".to_string());
        add(
            "s_classdef_dangling_lt",
            "classdef A <\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add(
            "s_classdef_dangling_dot",
            "classdef A < matlab.unittest.\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add(
            "s_classdef_dangling_amp",
            "classdef A < handle &\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add(
            "s_classdef_attr_unbalanced",
            "classdef (Abstract A < handle\nend\n".to_string(),
        );
        for (name, header) in [
            ("s_fn_trailing", "        function f(a) b"),
            ("s_fn_no_name", "        function = f"),
            ("s_fn_bare", "        function"),
            ("s_fn_unbalanced", "        function f(a"),
            ("s_fn_double_parens", "        function f(a)(b)"),
            ("s_fn_name_number", "        function 3f(a)"),
            ("s_fn_out_only", "        function x ="),
        ] {
            add(
                name,
                head(
                    BASE,
                    &methods(" (Test)", &format!("{header}\n        end\n{FUNC}")),
                ),
            );
        }
        add(
            "s_bare_signature_with_end",
            head(BASE, &methods(" (Test)", "        foo bar end\n")),
        );
        add(
            "s_bare_signature_unbalanced",
            head(BASE, &methods(" (Test)", "        foo(\n")),
        );
        add(
            "s_properties_unbalanced",
            head(
                BASE,
                "    properties\n        x = [1\n    end\n    methods (Test)\n        function t(testCase)\n        end\n    end\n",
            ),
        );
        add(
            "s_properties_missing_end",
            "classdef A < matlab.unittest.TestCase\n    properties\n        x\n    methods (Test)\n        function t(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add(
            "s_second_test_after_failure",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function a(testCase)\n            x = (1;\n        end\n        function b(testCase)\n        end\n    end\nend\n".to_string(),
        );
        add(
            "s_block_comment_open",
            format!("{}%{{\nnever closed\n", class_with("            x = 1;")),
        );
        add(
            "s_block_comment_open_inside",
            class_with("            %{\n            x = 1;"),
        );
        add(
            "s_unterminated_string",
            class_with("            s = 'unterminated;"),
        );
        add(
            "s_unterminated_dq",
            class_with("            s = \"unterminated;"),
        );
        add(
            "s_percent_brace_trailing_text",
            class_with("            %{ not a block\n            x = 1;"),
        );
        add(
            "s_depth_exceeded",
            format!(
                "{BASE}\n    methods (Test)\n        function loadsCatalog(testCase)\n{}{}        end\n    end\nend\n",
                "            if true\n".repeat(300),
                "            end\n".repeat(300),
            ),
        );
        add(
            "s_depth_exact_limit",
            format!(
                "{BASE}\n    methods (Test)\n        function loadsCatalog(testCase)\n{}{}        end\n    end\nend\n",
                "            if true\n".repeat(255),
                "            end\n".repeat(255),
            ),
        );
        add(
            "s_depth_plus_one",
            format!(
                "{BASE}\n    methods (Test)\n        function loadsCatalog(testCase)\n{}{}        end\n    end\nend\n",
                "            if true\n".repeat(256),
                "            end\n".repeat(256),
            ),
        );
        add(
            "s_depth_4000",
            format!(
                "{BASE}\n    methods (Test)\n        function loadsCatalog(testCase)\n{}{}        end\n    end\nend\n",
                "            if true\n".repeat(4_000),
                "            end\n".repeat(4_000),
            ),
        );
        add(
            "s_depth_mixed_blocks",
            format!(
                "{BASE}\n    methods (Test)\n        function t(testCase)\n{}{}        end\n    end\nend\n",
                "            for i = 1:2\n            while x\n            try\n".repeat(100),
                "            end\n            end\n            end\n".repeat(100),
            ),
        );
        add(
            "s_unicode_noise",
            "classdef A < matlab.unittest.TestCase\n    methods (Test)\n        function t(testCase)\n            % \u{65e5}\u{672c}\u{8a9e}\n            s = '\u{76ee}\u{5f55}';\n        end\n    end\nend\n".to_string(),
        );
        corpus
    }

    #[test]
    fn matlab_frontends_agree_on_every_committed_fixture() {
        let mut checked = 0;
        for (name, text) in [
            (
                "unittest_command_ambiguity",
                include_str!("../../../../fixtures/matlab/release/v0_2/unittest_command_ambiguity/CatalogTest.m"),
            ),
            (
                "unittest_degraded",
                include_str!("../../../../fixtures/matlab/release/v0_2/unittest_degraded/CatalogTest.m"),
            ),
            (
                "unittest_exact_tests",
                include_str!("../../../../fixtures/matlab/release/v0_2/unittest_exact_tests/CatalogTest.m"),
            ),
            (
                "unittest_low_support",
                include_str!("../../../../fixtures/matlab/release/v0_2/unittest_low_support/SoloTest.m"),
            ),
            (
                "unittest_unbound_block",
                include_str!("../../../../fixtures/matlab/release/v0_2/unittest_unbound_block/CatalogHelper.m"),
            ),
        ] {
            assert_eq!(
                new_output(text),
                legacy_output(text),
                "fixture {name} differs: new {} legacy {}",
                summary(&new_output(text)),
                summary(&legacy_output(text))
            );
            checked += 1;
        }
        assert_eq!(checked, 5);
    }

    /// Inputs the Tree-sitter frontend reads differently by design, with the new
    /// summary pinned. Each is invalid MATLAB the retired parser skipped or read
    /// leniently (`syntax_error_in_skipped_region`, `unknown_added`), except
    /// `m_empty_attrs`, which is valid MATLAB the grammar rejects.
    const DECLARED_DIVERGENCE: &[(&str, &str)] = &[
        // Grammar gap: tree-sitter-matlab rejects an empty attribute list.
        (
            "m_empty_attrs",
            "units[matlab_test_class:0-48,module:0-157] unknown[] diagnostics=1",
        ),
        // `FUNCTION` is not a keyword; the legacy read took it for a signature.
        (
            "fn_upper",
            "units[matlab_test_class:0-48,module:0-120] unknown[] diagnostics=1",
        ),
        // A stray `end` after the class is in the region the legacy never read.
        (
            "s_extra_end_after_class",
            "units[matlab_test_class:0-48,module:0-260,matlab_test_method:67-107,matlab_test_method:138-180,matlab_test_method:192-232] unknown[] diagnostics=1",
        ),
        // Statement-body syntax errors: the legacy parser checked only bracket
        // balance (and `(1 + 2]` not even that), the grammar checks the grammar.
        (
            "s_unbalanced_bracket",
            "units[matlab_test_class:0-48,module:0-259] unknown[] diagnostics=1",
        ),
        (
            "s_unbalanced_brace",
            "units[matlab_test_class:0-48,module:0-260] unknown[] diagnostics=1",
        ),
        (
            "s_mismatched",
            "units[matlab_test_class:0-48,module:0-262,matlab_test_method:67-107] unknown[] diagnostics=1",
        ),
        // Header and block syntax errors the legacy parser read leniently.
        (
            "s_classdef_dangling_lt",
            "units[module:0-85] unknown[] diagnostics=1",
        ),
        (
            "s_classdef_dangling_amp",
            "units[module:0-94] unknown[] diagnostics=1",
        ),
        (
            "s_fn_no_name",
            "units[matlab_test_class:0-48,module:0-153,matlab_test_method:67-88] unknown[] diagnostics=1",
        ),
        (
            "s_properties_missing_end",
            "units[matlab_test_class:0-38,module:0-135] unknown[] diagnostics=1",
        ),
    ];

    #[test]
    fn declared_divergence_is_pinned_to_the_grammar_reading() {
        let corpus = branch_corpus();
        for (name, expected) in DECLARED_DIVERGENCE {
            let (_, text) = corpus
                .iter()
                .find(|(candidate, _)| candidate == name)
                .unwrap_or_else(|| panic!("{name} is not in the branch corpus"));
            assert_eq!(&summary(&new_output(text)), expected, "{name}");
        }
    }

    #[test]
    fn matlab_frontends_agree_on_the_branch_corpus() {
        let corpus: Vec<_> = branch_corpus()
            .into_iter()
            .filter(|(name, _)| {
                DECLARED_DIVERGENCE
                    .iter()
                    .all(|(declared, _)| declared != name)
            })
            .collect();
        let mut differing = Vec::new();
        for (name, text) in &corpus {
            let new = new_output(text);
            let legacy = legacy_output(text);
            if new != legacy {
                differing.push(format!(
                    "{name}\n    new    : {}\n    legacy : {}",
                    summary(&new),
                    summary(&legacy)
                ));
            }
        }
        assert!(
            differing.is_empty(),
            "{} of {} branch sources differ\n{}",
            differing.len(),
            corpus.len(),
            differing.join("\n")
        );
    }

    /// Always-on exploration of mutated sources: reports, never fails. Mutations
    /// that break a skipped region are the declared divergence.
    #[test]
    #[ignore = "exploration; prints differences"]
    fn matlab_mutation_exploration() {
        let mut differing = 0usize;
        let mut silent_tree_error = 0usize;
        let mut silent_anchor_change = 0usize;
        let mut total = 0usize;
        for (name, text) in branch_corpus() {
            let lines: Vec<&str> = text.lines().collect();
            for drop in 0..lines.len().min(40) {
                let mutated: String = lines
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != drop)
                    .map(|(_, line)| format!("{line}\n"))
                    .collect();
                total += 1;
                let new = new_output(&mutated);
                let legacy = legacy_output(&mutated);
                if new != legacy {
                    differing += 1;
                    let legacy_silent = legacy
                        .as_ref()
                        .map(|output| output.report.diagnostics.is_empty())
                        .unwrap_or(false);
                    let clean = grammar_is_clean(&mutated);
                    let class = match (legacy_silent, clean) {
                        (true, false) => {
                            silent_tree_error += 1;
                            if summary(&new).split(" unknown").next()
                                != summary(&legacy).split(" unknown").next()
                            {
                                silent_anchor_change += 1;
                            }
                            continue;
                        }
                        (true, true) => "SILENT_LEGACY_CLEAN_TREE",
                        (false, true) => "FLAGGED_LEGACY_CLEAN_TREE",
                        (false, false) => "FLAGGED_LEGACY",
                    };
                    println!(
                        "{class} {name} drop line {drop}\n    new    : {}\n    legacy : {}",
                        summary(&new),
                        summary(&legacy)
                    );
                }
            }
        }
        println!(
            "legacy-silent invalid-syntax differences: {silent_tree_error} (unit set changed: {silent_anchor_change})"
        );
        println!("mutations: {differing} differ of {total}");
    }

    /// Compare both frontends over an external directory of `.m` files; prints a
    /// per-file summary of differences and never fails on them.
    #[test]
    #[ignore = "reads REPOGRAMMAR_MATLAB_DIFFERENTIAL_CORPUS"]
    fn matlab_external_corpus_differential() {
        let list = std::env::var("REPOGRAMMAR_MATLAB_DIFFERENTIAL_CORPUS")
            .expect("REPOGRAMMAR_MATLAB_DIFFERENTIAL_CORPUS");
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        let root = std::path::Path::new(&list);
        if root.is_dir() {
            let mut stack = vec![root.to_path_buf()];
            while let Some(directory) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().is_some_and(|extension| extension == "m") {
                        paths.push(path);
                    }
                }
            }
        } else {
            paths = std::fs::read_to_string(root)
                .expect("read corpus file list")
                .lines()
                .map(std::path::PathBuf::from)
                .collect();
        }
        paths.sort();
        let (mut compared, mut differing, mut legacy_panics, mut new_panics) = (0, 0, 0, 0);
        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            compared += 1;
            let legacy = std::panic::catch_unwind(|| legacy_output(&text));
            let new = std::panic::catch_unwind(|| new_output(&text));
            match (legacy, new) {
                (Ok(legacy), Ok(new)) => {
                    if legacy != new {
                        differing += 1;
                        println!(
                            "DIFF {}\n    new    : {}\n    legacy : {}",
                            path.display(),
                            summary(&new),
                            summary(&legacy)
                        );
                    }
                }
                (legacy, new) => {
                    legacy_panics += usize::from(legacy.is_err());
                    new_panics += usize::from(new.is_err());
                    println!(
                        "PANIC {} legacy_panicked={} new_panicked={}",
                        path.display(),
                        legacy.is_err(),
                        new.is_err()
                    );
                }
            }
        }
        println!(
            "compared {compared} files: {differing} differ, legacy panics {legacy_panics}, new panics {new_panics}"
        );
    }
}
