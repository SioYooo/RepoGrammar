//! Bounded MATLAB `matlab.unittest` frontend for the ADR-0046 admitted shape.
//!
//! Nothing here invokes MATLAB or Octave, opens a project, or evaluates code.
//! ADR-0037's prohibitions are carried forward.
//!
//! This is a recursive-descent parser over a declared subset, not a line
//! scanner. It lexes the whole file once, then descends
//! `classdef` -> `methods` -> `function` -> statement region, and it accepts or
//! refuses each construct against a grammar. Outside the declared subset it
//! abstains: it never recovers, never resumes, and never guesses a block
//! boundary.
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

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

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

/// Statement keywords that open a block terminated by `end` inside a function
/// body. `methods`, `properties`, `events`, `enumeration`, and `arguments` are
/// deliberately absent: they are contextual, and treating them as unconditional
/// openers invents a block for `methods = getMethods(x);`.
const STATEMENT_BLOCK_OPENERS: &[&str] = &[
    "if", "for", "while", "switch", "try", "parfor", "spmd", "function",
];

/// Keywords that continue an already-open block rather than opening a new one.
const BLOCK_CONTINUATIONS: &[&str] = &["elseif", "else", "case", "otherwise", "catch"];

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
        None => parse_tokens(document.text, &lexed.tokens),
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
    facts.sort_by(|left, right| {
        (
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
        )
            .cmp(&(
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
            ))
    });
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
                    return true
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

fn parse_tokens(text: &str, tokens: &[Token]) -> FileParse {
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
        while self
            .peek()
            .is_some_and(|token| matches!(token.kind, TokenKind::Separator | TokenKind::LineBreak))
        {
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
                Some(dot) if dot.kind == TokenKind::Symbol && dot.byte == b'.' && cursor < end => {
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
        let assignment = (cursor..end).find(|index| self.tokens[*index].kind == TokenKind::Assign);
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
