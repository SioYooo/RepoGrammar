//! Bounded R testthat frontend for the ADR-0042 admitted shape.
//!
//! Only `tests/testthat/test-*.R` bytes reach this parser, because that is the
//! set testthat's own `test_dir()` runner reads. The path is identity evidence
//! rather than a style preference, and the repository must additionally declare
//! `testthat` in `DESCRIPTION` before any anchor forms.
//!
//! Nothing here invokes R, `parse`, `eval`, `source`, renv, package
//! restoration, or a repository script. ADR-0036's execution prohibitions all
//! name running R and are unreached: nothing runs.
//!
//! This is a hand-written recursive-descent parser over the declared R subset
//! in ADR-0042 D4, not a text scan. It lexes R's tokens -- comments, both quote
//! styles, R 4.0 raw strings, backtick names, numbers, and the full operator
//! set -- reproduces R's newline rule, and builds a real expression tree with
//! R's own precedence and associativity. The anchor is then a question about
//! that tree: *is this top-level expression itself the call?*
//!
//! Asking the question structurally is the point. A byte scan cannot tell
//! `test_that(...)` apart from `if (interactive()) test_that(...)`, because
//! both sit at parenthesis and brace depth zero; the tree can, because the
//! second is a subexpression of an `if`. This repository has shipped the
//! opposite defect four times -- a Rust attribute matched inside a function
//! body, a TS/JS runner matched inside a member call, a Go declaration matched
//! inside a string, and both branches of a Delphi `{$IFDEF}` -- each from asking
//! whether text appears rather than whether a construct exists.
//!
//! Outside the declared subset the parser abstains: it never guesses, never
//! recovers, and never resynchronizes to a later expression. Input is untrusted,
//! so both the input size and the recursion depth are bounded.

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

pub const R_ANCHOR_ENGINE: &str = "repogrammar-r-testthat-parser";
pub const R_ANCHOR_METHOD: &str = "bounded_r_test_that_v2";

/// Fixed support target for the one admitted exact anchor.
pub const R_TEST_THAT_TARGET: &str = "testthat.test_that";

const CALLEE: &str = "test_that";
const MAX_BLOCK_UNITS: usize = 4_096;
/// Bound on recursive-descent nesting. Input is untrusted, and a file of
/// nothing but open parentheses must abstain rather than exhaust the stack.
const MAX_PARSE_DEPTH: usize = 128;

/// True for the only paths this frontend may read.
///
/// `test_dir()` scans `tests/testthat/` and runs files whose names begin with
/// `test`; this admits the `test-` spelling the package convention uses.
pub fn is_testthat_path(path: &str) -> bool {
    let mut components = path.split('/').collect::<Vec<_>>();
    let Some(name) = components.pop() else {
        return false;
    };
    if !name.starts_with("test-") || !(name.ends_with(".R") || name.ends_with(".r")) {
        return false;
    }
    components
        .windows(2)
        .any(|pair| pair == ["tests", "testthat"])
}

#[derive(Debug, Default)]
pub struct RTestThatParser;

impl SourceParser for RTestThatParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        parse_output(document, context).map(|output| output.report)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document, context)
    }
}

pub(crate) fn parse_output(
    document: SourceDocument<'_>,
    context: &ParserProjectContext,
) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::R || !is_testthat_path(document.path) {
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
            "unit:{}#r_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::R,
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
            "r_test_parse",
            "source_byte_limit",
            full_range,
            "R source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    if !context.r_declares_testthat {
        // A directory named tests/testthat in a project that does not depend on
        // testthat establishes nothing.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::MissingDependency,
            "r_testthat_identity",
            "testthat_not_declared",
            full_range,
            "no DESCRIPTION in this repository declares testthat, so the framework identity is unproven",
        )?);
        return finish(units, facts, Vec::new());
    }

    let program = parse_program(document.text);

    // Leaving the declared subset is a decidable, source-visible event, and it
    // makes a missing anchor uninformative: fewer admitted calls after that
    // point is indistinguishable from a file that simply has fewer calls. The
    // expressions already proven are kept; the diagnostic states that the ones
    // not found prove nothing. The parser never resynchronizes past the
    // boundary, so nothing after it is guessed.
    let mut diagnostics = Vec::new();
    if let Some(abstention) = program.abstention {
        diagnostics.push(degraded(document.path, abstention.diagnostic()));
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "r_test_parse",
            abstention.unknown_kind(),
            module.range.clone(),
            abstention.note(),
        )?);
    }

    if program.rebinds_callee {
        // `test_that` is an ordinary R binding, not a keyword. A file that
        // assigns the name itself has redefined what every later call in it
        // means, and RepoGrammar cannot evaluate the replacement. Nothing in
        // the file anchors.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::MonkeyPatch,
            "r_testthat_identity",
            "test_that_rebound_in_file",
            full_range,
            "this file binds the name test_that itself, so a call to it is not proven to be testthat's",
        )?);
        return finish(units, facts, diagnostics);
    }

    if program.conditional_registration {
        // `if (interactive())` and `for (...)` decide at run time whether -- and
        // how often -- a test is registered, and RepoGrammar evaluates neither.
        // The call is not admitted, and saying so understates support rather
        // than unproving the unconditional calls beside it.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::BuildVariantAmbiguity,
            "r_conditional_test_registration",
            "conditional_test_registration",
            module.range.clone(),
            "an otherwise admitted test_that call is reached only under a runtime condition or loop",
        )?);
    }

    let mut limit_hit = false;
    for (ordinal, block) in program.anchors.into_iter().enumerate() {
        if ordinal >= MAX_BLOCK_UNITS {
            limit_hit = true;
            break;
        }
        let range = SourceRange::new(block.start, block.end).map_err(ParseError::Internal)?;
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#r_test_that_block:{}-{}:{}",
                document.path, block.start, block.end, ordinal
            ))
            .map_err(ParseError::Internal)?,
            language: Language::R,
            kind: CodeUnitKind::RTestThatBlock,
            range: range.clone(),
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(&unit)?);
        units.push(unit);
    }

    if limit_hit {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "r_test_parse",
            "parser_resource_limit",
            module.range.clone(),
            "R parser exceeded the bounded block limit",
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
// Abstention
// ---------------------------------------------------------------------------

/// Why the parse stopped, and where.
///
/// Both classes are decidable from source alone. Neither is ever recovered
/// from: the parser reports the boundary and stops, so no expression after it
/// is guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Abstention {
    reason: AbstainReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbstainReason {
    /// A byte or construct outside the declared subset of ADR-0042 D4.
    UnadmittedConstruct,
    /// Nesting past [`MAX_PARSE_DEPTH`].
    DepthLimit,
}

impl Abstention {
    fn unadmitted() -> Self {
        Self {
            reason: AbstainReason::UnadmittedConstruct,
        }
    }

    fn depth_limit() -> Self {
        Self {
            reason: AbstainReason::DepthLimit,
        }
    }

    fn unknown_kind(self) -> &'static str {
        match self.reason {
            AbstainReason::UnadmittedConstruct => "unadmitted_r_construct",
            AbstainReason::DepthLimit => "parser_depth_limit",
        }
    }

    fn diagnostic(self) -> &'static str {
        match self.reason {
            AbstainReason::UnadmittedConstruct => {
                "an R construct outside the declared subset ends the parse, so the expressions after it are unread"
            }
            AbstainReason::DepthLimit => {
                "R nesting exceeded the bounded parse depth, so the expressions after it are unread"
            }
        }
    }

    fn note(self) -> &'static str {
        match self.reason {
            AbstainReason::UnadmittedConstruct => {
                "R source left the declared parsed subset, so no later expression in this file was read"
            }
            AbstainReason::DepthLimit => {
                "R source nested past the bounded parse depth, so no later expression in this file was read"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// A token of the declared R subset.
///
/// Reserved words that are only *values* (`TRUE`, `NULL`, `NA`, `Inf`, ...)
/// stay [`TokenKind::Identifier`]: they parse exactly as symbols do, so the
/// tree is the same shape either way and this frontend reads no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Newline,
    Semicolon,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    LDoubleBracket,
    RBracket,
    Identifier,
    BacktickName,
    StringLiteral,
    RawStringLiteral,
    Number,
    If,
    Else,
    For,
    In,
    While,
    Repeat,
    Function,
    Break,
    Next,
    /// `<-` and `<<-`.
    Assign,
    /// `->` and `->>`.
    RightAssign,
    /// `=` in a value position.
    EqAssign,
    Tilde,
    /// `|` and `||`.
    Or,
    /// `&` and `&&`.
    And,
    Not,
    /// `<`, `>`, `<=`, `>=`, `==`, `!=`; non-associative in R.
    Compare,
    Plus,
    Minus,
    Star,
    Slash,
    /// `%...%` and the R 4.1 native pipe `|>`; one precedence level in R.
    Special,
    Colon,
    Caret,
    Dollar,
    At,
    /// `::` and `:::`.
    NsGet,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'.'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'
}

/// Tokenize as much of `text` as lies inside the declared subset.
///
/// The tokens before an abstention are still proven, so they are returned with
/// it; the parser reads that prefix and stops where the lexer stopped.
fn lex(text: &str) -> (Vec<Token>, Option<Abstention>) {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        let kind = match byte {
            b' ' | b'\t' | b'\r' | 0x0b | 0x0c => {
                index += 1;
                continue;
            }
            b'\n' => {
                index += 1;
                TokenKind::Newline
            }
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'"' | b'\'' => {
                let Some(end) = quoted_end(bytes, index, byte) else {
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index = end;
                TokenKind::StringLiteral
            }
            b'`' => {
                let Some(end) = quoted_end(bytes, index, b'`') else {
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index = end;
                TokenKind::BacktickName
            }
            b'%' => {
                // R's lexer refuses a newline inside `%...%`.
                let mut cursor = index + 1;
                loop {
                    match bytes.get(cursor) {
                        Some(b'%') => break,
                        Some(b'\n') | None => return (tokens, Some(Abstention::unadmitted())),
                        Some(_) => cursor += 1,
                    }
                }
                index = cursor + 1;
                TokenKind::Special
            }
            b'r' | b'R' if raw_string_prefix(bytes, index) => {
                let Some(end) = raw_string_end(bytes, index) else {
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index = end;
                TokenKind::RawStringLiteral
            }
            byte if byte.is_ascii_digit() => {
                let Some(end) = number_end(bytes, index) else {
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index = end;
                TokenKind::Number
            }
            b'.' if bytes.get(index + 1).is_some_and(u8::is_ascii_digit) => {
                let Some(end) = number_end(bytes, index) else {
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index = end;
                TokenKind::Number
            }
            byte if is_ident_start(byte) => {
                index += 1;
                while index < bytes.len() && is_ident_continue(bytes[index]) {
                    index += 1;
                }
                keyword_kind(&text[start..index])
            }
            _ => {
                let Some((kind, width)) = operator(bytes, index) else {
                    // Every remaining byte -- `?`, `_`, a stray symbol, or any
                    // non-ASCII byte in code position -- begins no token of the
                    // declared subset.
                    return (tokens, Some(Abstention::unadmitted()));
                };
                index += width;
                kind
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: index,
        });
    }
    (tokens, None)
}

fn keyword_kind(word: &str) -> TokenKind {
    match word {
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "while" => TokenKind::While,
        "repeat" => TokenKind::Repeat,
        "function" => TokenKind::Function,
        "break" => TokenKind::Break,
        "next" => TokenKind::Next,
        _ => TokenKind::Identifier,
    }
}

/// Longest-match operator table for the declared subset.
///
/// `?` (help) and `=>` (the option-gated pipe bind) are deliberately absent:
/// see ADR-0042 D4.
fn operator(bytes: &[u8], index: usize) -> Option<(TokenKind, usize)> {
    let rest = &bytes[index..];
    let table: [(&[u8], TokenKind); 33] = [
        (b"<<-", TokenKind::Assign),
        (b"->>", TokenKind::RightAssign),
        (b":::", TokenKind::NsGet),
        (b"<-", TokenKind::Assign),
        (b"->", TokenKind::RightAssign),
        (b"<=", TokenKind::Compare),
        (b">=", TokenKind::Compare),
        (b"==", TokenKind::Compare),
        (b"!=", TokenKind::Compare),
        (b"&&", TokenKind::And),
        (b"||", TokenKind::Or),
        (b"|>", TokenKind::Special),
        (b"::", TokenKind::NsGet),
        (b"[[", TokenKind::LDoubleBracket),
        (b"<", TokenKind::Compare),
        (b">", TokenKind::Compare),
        (b"=", TokenKind::EqAssign),
        (b"&", TokenKind::And),
        (b"|", TokenKind::Or),
        (b"!", TokenKind::Not),
        (b"+", TokenKind::Plus),
        (b"-", TokenKind::Minus),
        (b"*", TokenKind::Star),
        (b"/", TokenKind::Slash),
        (b"^", TokenKind::Caret),
        (b"~", TokenKind::Tilde),
        (b":", TokenKind::Colon),
        (b"$", TokenKind::Dollar),
        (b"@", TokenKind::At),
        (b"\\", TokenKind::Function),
        (b"(", TokenKind::LParen),
        (b")", TokenKind::RParen),
        (b",", TokenKind::Comma),
    ];
    if rest.starts_with(b"=>") {
        // `=>` is R's pipe bind, admitted only when the `_R_USE_PIPEBIND_`
        // option is set. Whether it parses is therefore an option this
        // frontend cannot determine, so it is refused rather than read.
        return None;
    }
    for (spelling, kind) in table {
        if rest.starts_with(spelling) {
            return Some((kind, spelling.len()));
        }
    }
    match rest.first()? {
        b'{' => Some((TokenKind::LBrace, 1)),
        b'}' => Some((TokenKind::RBrace, 1)),
        b'[' => Some((TokenKind::LBracket, 1)),
        b']' => Some((TokenKind::RBracket, 1)),
        b';' => Some((TokenKind::Semicolon, 1)),
        _ => None,
    }
}

/// End of a quoted run, or `None` when it never closes.
fn quoted_end(bytes: &[u8], start: usize, delimiter: u8) -> Option<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == delimiter => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// True when an R 4.0 raw string starts at `start`.
fn raw_string_prefix(bytes: &[u8], start: usize) -> bool {
    matches!(bytes.get(start + 1), Some(b'"') | Some(b'\''))
}

/// End of an R 4.0 raw string starting at `start`, or `None` when it never
/// closes.
///
/// The forms are `r"(...)"`, `r"[...]"`, `r"{...}"`, any number of dashes
/// between the quote and the opener, and an uppercase `R` prefix.
fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = *bytes.get(start + 1)?;
    let mut index = start + 2;
    let dash_start = index;
    while bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let dashes = index - dash_start;
    let close = match bytes.get(index)? {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        _ => return None,
    };
    index += 1;
    let mut terminator = vec![close];
    terminator.extend(std::iter::repeat_n(b'-', dashes));
    terminator.push(quote);
    while index < bytes.len() {
        if bytes[index..].starts_with(&terminator) {
            return Some(index + terminator.len());
        }
        index += 1;
    }
    None
}

/// End of an R numeric constant, or `None` when the run is not one.
fn number_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if bytes[index] == b'0' && matches!(bytes.get(index + 1), Some(b'x') | Some(b'X')) {
        index += 2;
        let digits = index;
        while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
            index += 1;
        }
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                index += 1;
            }
        }
        if index == digits {
            return None;
        }
        if matches!(bytes.get(index), Some(b'p') | Some(b'P')) {
            index = exponent_end(bytes, index)?;
        }
    } else {
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
        if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
            index = exponent_end(bytes, index)?;
        }
    }
    if matches!(bytes.get(index), Some(b'L') | Some(b'i')) {
        index += 1;
    }
    // A constant may not run straight into an identifier: `1abc` is not R.
    if bytes
        .get(index)
        .is_some_and(|byte| is_ident_continue(*byte) || !byte.is_ascii())
    {
        return None;
    }
    Some(index)
}

fn exponent_end(bytes: &[u8], marker: usize) -> Option<usize> {
    let mut index = marker + 1;
    if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
        index += 1;
    }
    let digits = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    (index > digits).then_some(index)
}

// ---------------------------------------------------------------------------
// Syntax tree
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Node<'a> {
    start: usize,
    end: usize,
    kind: NodeKind<'a>,
}

#[derive(Debug)]
enum NodeKind<'a> {
    /// A bare symbol. A backtick-quoted name is [`NodeKind::QuotedName`]
    /// instead, so a callee spelled `` `test_that` `` is never a bare
    /// identifier here.
    Identifier(&'a str),
    QuotedName(&'a str),
    /// `raw` marks an R 4.0 raw string; `inner` is the literal's own text,
    /// without its delimiters.
    Str {
        raw: bool,
        inner: &'a str,
    },
    Number,
    Brace(Vec<Node<'a>>),
    Paren(Box<Node<'a>>),
    Call {
        callee: Box<Node<'a>>,
        args: Vec<Arg<'a>>,
    },
    Index {
        object: Box<Node<'a>>,
        args: Vec<Arg<'a>>,
    },
    Unary(Box<Node<'a>>),
    Binary {
        op: TokenKind,
        lhs: Box<Node<'a>>,
        rhs: Box<Node<'a>>,
    },
    Function {
        defaults: Vec<Node<'a>>,
        body: Box<Node<'a>>,
    },
    If {
        cond: Box<Node<'a>>,
        then_branch: Box<Node<'a>>,
        else_branch: Option<Box<Node<'a>>>,
    },
    For {
        variable: &'a str,
        sequence: Box<Node<'a>>,
        body: Box<Node<'a>>,
    },
    While {
        cond: Box<Node<'a>>,
        body: Box<Node<'a>>,
    },
    Repeat(Box<Node<'a>>),
    /// `break`, `next`, and an empty argument such as the first slot of
    /// `m[, 1]`.
    Atom,
}

/// One element of an R argument list. Both halves are optional: `f(a = )` has a
/// name and no value, and `m[, 1]` has a first element with neither.
#[derive(Debug)]
struct Arg<'a> {
    named: bool,
    value: Option<Node<'a>>,
}

impl<'a> Node<'a> {
    fn visit<'b>(&'b self, seen: &mut dyn FnMut(&'b Node<'a>)) {
        seen(self);
        match &self.kind {
            NodeKind::Identifier(_)
            | NodeKind::QuotedName(_)
            | NodeKind::Str { .. }
            | NodeKind::Number
            | NodeKind::Atom => {}
            NodeKind::Brace(items) => items.iter().for_each(|item| item.visit(seen)),
            NodeKind::Paren(inner) | NodeKind::Unary(inner) | NodeKind::Repeat(inner) => {
                inner.visit(seen)
            }
            NodeKind::Call { callee, args } => {
                callee.visit(seen);
                visit_args(args, seen);
            }
            NodeKind::Index { object, args } => {
                object.visit(seen);
                visit_args(args, seen);
            }
            NodeKind::Binary { lhs, rhs, .. } => {
                lhs.visit(seen);
                rhs.visit(seen);
            }
            NodeKind::Function { defaults, body } => {
                defaults.iter().for_each(|item| item.visit(seen));
                body.visit(seen);
            }
            NodeKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                cond.visit(seen);
                then_branch.visit(seen);
                if let Some(branch) = else_branch {
                    branch.visit(seen);
                }
            }
            NodeKind::For { sequence, body, .. } => {
                sequence.visit(seen);
                body.visit(seen);
            }
            NodeKind::While { cond, body } => {
                cond.visit(seen);
                body.visit(seen);
            }
        }
    }
}

fn visit_args<'a, 'b>(args: &'b [Arg<'a>], seen: &mut dyn FnMut(&'b Node<'a>)) {
    for arg in args {
        if let Some(value) = &arg.value {
            value.visit(seen);
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Binding powers, lowest first, in R's own order (`?Syntax`, and the
/// precedence declarations of R's `gram.y`). `?` is not in the subset.
mod bp {
    pub(super) const ASSIGN: (u8, u8) = (7, 6);
    pub(super) const EQ_ASSIGN: (u8, u8) = (9, 8);
    pub(super) const RIGHT_ASSIGN: (u8, u8) = (10, 11);
    pub(super) const TILDE: (u8, u8) = (12, 13);
    pub(super) const OR: (u8, u8) = (14, 15);
    pub(super) const AND: (u8, u8) = (16, 17);
    pub(super) const NOT_PREFIX: u8 = 18;
    pub(super) const COMPARE: (u8, u8) = (20, 21);
    pub(super) const ADD: (u8, u8) = (24, 25);
    pub(super) const MULTIPLY: (u8, u8) = (26, 27);
    pub(super) const SPECIAL: (u8, u8) = (28, 29);
    pub(super) const COLON: (u8, u8) = (32, 33);
    pub(super) const SIGN_PREFIX: u8 = 34;
    pub(super) const CARET: (u8, u8) = (37, 36);
    pub(super) const COMPONENT: (u8, u8) = (38, 39);
    pub(super) const NS_GET: (u8, u8) = (40, 41);
    pub(super) const POSTFIX: u8 = 42;
}

fn infix_binding_power(kind: TokenKind) -> Option<(u8, u8)> {
    Some(match kind {
        TokenKind::Assign => bp::ASSIGN,
        TokenKind::EqAssign => bp::EQ_ASSIGN,
        TokenKind::RightAssign => bp::RIGHT_ASSIGN,
        TokenKind::Tilde => bp::TILDE,
        TokenKind::Or => bp::OR,
        TokenKind::And => bp::AND,
        TokenKind::Compare => bp::COMPARE,
        TokenKind::Plus | TokenKind::Minus => bp::ADD,
        TokenKind::Star | TokenKind::Slash => bp::MULTIPLY,
        TokenKind::Special => bp::SPECIAL,
        TokenKind::Colon => bp::COLON,
        TokenKind::Caret => bp::CARET,
        TokenKind::Dollar | TokenKind::At => bp::COMPONENT,
        TokenKind::NsGet => bp::NS_GET,
        _ => return None,
    })
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
    /// Whether a newline currently terminates a complete expression.
    ///
    /// R eats newlines inside `(`, `[`, and `[[`, and treats them as statement
    /// separators at top level and inside `{`. The stack restores the outer
    /// rule when a group closes, so a `{` inside a call argument gets brace
    /// behaviour and the argument list gets its own back afterwards.
    newline_terminates: Vec<bool>,
}

type Parsed<T> = Result<T, Abstention>;

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<TokenKind> {
        self.tokens.get(self.position).map(|token| token.kind)
    }

    fn newlines_terminate(&self) -> bool {
        self.newline_terminates.last().copied().unwrap_or(true)
    }

    /// Skip newlines, which R does wherever an expression is still required.
    fn skip_newlines(&mut self) {
        while self.peek() == Some(TokenKind::Newline) {
            self.position += 1;
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.peek() == Some(kind) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Parsed<Token> {
        match self.tokens.get(self.position) {
            Some(token) if token.kind == kind => {
                self.position += 1;
                Ok(*token)
            }
            _ => Err(Abstention::unadmitted()),
        }
    }

    fn enter(&mut self) -> Parsed<()> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(Abstention::depth_limit());
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Parse one expression whose operators bind at least `min_bp`.
    ///
    /// `allow_eq` mirrors R's split between `expr_or_assign` and `expr`: `=` is
    /// an assignment at statement level and inside `(...)` grouping and blocks,
    /// and is not available at all inside an argument list or a condition,
    /// where it names an argument instead.
    fn expression(&mut self, min_bp: u8, allow_eq: bool) -> Parsed<Node<'a>> {
        self.enter()?;
        let parsed = self.expression_inner(min_bp, allow_eq);
        self.leave();
        parsed
    }

    fn expression_inner(&mut self, min_bp: u8, allow_eq: bool) -> Parsed<Node<'a>> {
        // An operand is required here, and R eats newlines wherever that is so.
        self.skip_newlines();
        let mut lhs = self.prefix(allow_eq)?;
        let mut comparison_used = false;
        loop {
            if !self.newlines_terminate() {
                self.skip_newlines();
            }
            let Some(kind) = self.peek() else { break };
            match kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LDoubleBracket => {
                    if bp::POSTFIX < min_bp {
                        break;
                    }
                    lhs = self.postfix(lhs, kind)?;
                }
                TokenKind::EqAssign if !allow_eq => break,
                TokenKind::Dollar | TokenKind::At | TokenKind::NsGet => {
                    let (left_bp, _) = infix_binding_power(kind).unwrap_or(bp::COMPONENT);
                    if left_bp < min_bp {
                        break;
                    }
                    self.position += 1;
                    self.skip_newlines();
                    let name = self.component_name()?;
                    lhs = Node {
                        start: lhs.start,
                        end: name.end,
                        kind: NodeKind::Binary {
                            op: kind,
                            lhs: Box::new(lhs),
                            rhs: Box::new(name),
                        },
                    };
                }
                _ => {
                    let Some((left_bp, right_bp)) = infix_binding_power(kind) else {
                        break;
                    };
                    if left_bp < min_bp {
                        break;
                    }
                    if kind == TokenKind::Compare {
                        // R declares comparison non-associative, so `a < b < c`
                        // is a syntax error rather than a nested comparison.
                        if comparison_used {
                            return Err(Abstention::unadmitted());
                        }
                        comparison_used = true;
                    }
                    self.position += 1;
                    // An operator at end of line continues the expression.
                    self.skip_newlines();
                    let rhs = self.expression(right_bp, allow_eq)?;
                    lhs = Node {
                        start: lhs.start,
                        end: rhs.end,
                        kind: NodeKind::Binary {
                            op: kind,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    };
                }
            }
        }
        Ok(lhs)
    }

    fn component_name(&mut self) -> Parsed<Node<'a>> {
        let Some(token) = self.tokens.get(self.position).copied() else {
            return Err(Abstention::unadmitted());
        };
        let kind = match token.kind {
            TokenKind::Identifier => NodeKind::Identifier(&self.text[token.start..token.end]),
            TokenKind::BacktickName => NodeKind::QuotedName(string_inner(self.text, token, false)),
            TokenKind::StringLiteral => NodeKind::Str {
                raw: false,
                inner: string_inner(self.text, token, false),
            },
            _ => return Err(Abstention::unadmitted()),
        };
        self.position += 1;
        Ok(Node {
            start: token.start,
            end: token.end,
            kind,
        })
    }

    fn postfix(&mut self, lhs: Node<'a>, opener: TokenKind) -> Parsed<Node<'a>> {
        self.position += 1;
        self.newline_terminates.push(false);
        let closer = if opener == TokenKind::LParen {
            TokenKind::RParen
        } else {
            TokenKind::RBracket
        };
        // R's `[[` closes with two separate `]` tokens, which is what lets
        // `x[[y[1]]]` nest correctly.
        let closer_count = if opener == TokenKind::LDoubleBracket {
            2
        } else {
            1
        };
        // An argument list never carries an assignment `=`; there it names an
        // argument. A nested `(` or `{` inside one restores the statement rule.
        let args = self.arguments(closer)?;
        let mut end = 0usize;
        for _ in 0..closer_count {
            self.skip_newlines();
            end = self.expect(closer)?.end;
        }
        self.newline_terminates.pop();
        Ok(Node {
            start: lhs.start,
            end,
            kind: if opener == TokenKind::LParen {
                NodeKind::Call {
                    callee: Box::new(lhs),
                    args,
                }
            } else {
                NodeKind::Index {
                    object: Box::new(lhs),
                    args,
                }
            },
        })
    }

    /// R's `sublist`: comma-separated, elements may be empty, and an element
    /// may be `name =` with or without a value.
    fn arguments(&mut self, closer: TokenKind) -> Parsed<Vec<Arg<'a>>> {
        self.enter()?;
        let parsed = self.arguments_inner(closer);
        self.leave();
        parsed
    }

    fn arguments_inner(&mut self, closer: TokenKind) -> Parsed<Vec<Arg<'a>>> {
        let mut args = Vec::new();
        self.skip_newlines();
        if self.peek() == Some(closer) {
            return Ok(args);
        }
        loop {
            self.skip_newlines();
            let named = self.argument_name();
            self.skip_newlines();
            let value = match self.peek() {
                Some(TokenKind::Comma) | None => None,
                Some(kind) if kind == closer => None,
                Some(_) => Some(self.expression(0, false)?),
            };
            args.push(Arg { named, value });
            self.skip_newlines();
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Ok(args)
    }

    /// Consume a leading `name =` when the element has one.
    fn argument_name(&mut self) -> bool {
        let Some(token) = self.tokens.get(self.position) else {
            return false;
        };
        if !matches!(
            token.kind,
            TokenKind::Identifier
                | TokenKind::StringLiteral
                | TokenKind::BacktickName
                | TokenKind::RawStringLiteral
        ) {
            return false;
        }
        let mut lookahead = self.position + 1;
        while self.tokens.get(lookahead).map(|token| token.kind) == Some(TokenKind::Newline) {
            lookahead += 1;
        }
        if self.tokens.get(lookahead).map(|token| token.kind) != Some(TokenKind::EqAssign) {
            return false;
        }
        self.position = lookahead + 1;
        true
    }

    fn prefix(&mut self, allow_eq: bool) -> Parsed<Node<'a>> {
        let Some(token) = self.tokens.get(self.position).copied() else {
            return Err(Abstention::unadmitted());
        };
        match token.kind {
            TokenKind::Identifier => {
                self.position += 1;
                Ok(Node {
                    start: token.start,
                    end: token.end,
                    kind: NodeKind::Identifier(&self.text[token.start..token.end]),
                })
            }
            TokenKind::BacktickName => {
                self.position += 1;
                Ok(Node {
                    start: token.start,
                    end: token.end,
                    kind: NodeKind::QuotedName(string_inner(self.text, token, false)),
                })
            }
            TokenKind::StringLiteral | TokenKind::RawStringLiteral => {
                self.position += 1;
                let raw = token.kind == TokenKind::RawStringLiteral;
                Ok(Node {
                    start: token.start,
                    end: token.end,
                    kind: NodeKind::Str {
                        raw,
                        inner: string_inner(self.text, token, raw),
                    },
                })
            }
            TokenKind::Number => {
                self.position += 1;
                Ok(Node {
                    start: token.start,
                    end: token.end,
                    kind: NodeKind::Number,
                })
            }
            TokenKind::Break | TokenKind::Next => {
                self.position += 1;
                Ok(Node {
                    start: token.start,
                    end: token.end,
                    kind: NodeKind::Atom,
                })
            }
            TokenKind::LParen => {
                self.position += 1;
                self.newline_terminates.push(false);
                let inner = self.expression(0, true)?;
                self.skip_newlines();
                let closer = self.expect(TokenKind::RParen)?;
                self.newline_terminates.pop();
                Ok(Node {
                    start: token.start,
                    end: closer.end,
                    kind: NodeKind::Paren(Box::new(inner)),
                })
            }
            TokenKind::LBrace => {
                self.position += 1;
                self.newline_terminates.push(true);
                let items = self.block_body()?;
                let closer = self.expect(TokenKind::RBrace)?;
                self.newline_terminates.pop();
                Ok(Node {
                    start: token.start,
                    end: closer.end,
                    kind: NodeKind::Brace(items),
                })
            }
            TokenKind::Minus | TokenKind::Plus => {
                self.position += 1;
                let operand = self.expression(bp::SIGN_PREFIX, allow_eq)?;
                Ok(Node {
                    start: token.start,
                    end: operand.end,
                    kind: NodeKind::Unary(Box::new(operand)),
                })
            }
            TokenKind::Not => {
                self.position += 1;
                let operand = self.expression(bp::NOT_PREFIX, allow_eq)?;
                Ok(Node {
                    start: token.start,
                    end: operand.end,
                    kind: NodeKind::Unary(Box::new(operand)),
                })
            }
            TokenKind::Tilde => {
                self.position += 1;
                let operand = self.expression(bp::TILDE.1, allow_eq)?;
                Ok(Node {
                    start: token.start,
                    end: operand.end,
                    kind: NodeKind::Unary(Box::new(operand)),
                })
            }
            TokenKind::Function => self.function(token),
            TokenKind::If => self.conditional(token),
            TokenKind::For => self.for_loop(token),
            TokenKind::While => self.while_loop(token),
            TokenKind::Repeat => {
                self.position += 1;
                let body = self.expression(0, true)?;
                Ok(Node {
                    start: token.start,
                    end: body.end,
                    kind: NodeKind::Repeat(Box::new(body)),
                })
            }
            _ => Err(Abstention::unadmitted()),
        }
    }

    /// R's `formlist`: a symbol, optionally with a default.
    fn function(&mut self, token: Token) -> Parsed<Node<'a>> {
        self.position += 1;
        self.skip_newlines();
        self.expect(TokenKind::LParen)?;
        self.newline_terminates.push(false);
        let mut defaults = Vec::new();
        self.skip_newlines();
        if self.peek() != Some(TokenKind::RParen) {
            loop {
                self.skip_newlines();
                self.expect(TokenKind::Identifier)?;
                self.skip_newlines();
                if self.eat(TokenKind::EqAssign) {
                    defaults.push(self.expression(0, false)?);
                }
                self.skip_newlines();
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        self.newline_terminates.pop();
        // R eats newlines between the formals and the body.
        self.skip_newlines();
        let body = self.expression(0, true)?;
        Ok(Node {
            start: token.start,
            end: body.end,
            kind: NodeKind::Function {
                defaults,
                body: Box::new(body),
            },
        })
    }

    fn conditional(&mut self, token: Token) -> Parsed<Node<'a>> {
        self.position += 1;
        let cond = self.parenthesized_condition()?;
        // R eats newlines between the condition and the body, so
        // `if (x)\n  body` is one expression.
        self.skip_newlines();
        let then_branch = self.expression(0, true)?;
        let mut end = then_branch.end;
        let else_branch = if self.take_else() {
            self.skip_newlines();
            let branch = self.expression(0, true)?;
            end = branch.end;
            Some(Box::new(branch))
        } else {
            None
        };
        Ok(Node {
            start: token.start,
            end,
            kind: NodeKind::If {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch,
            },
        })
    }

    /// Consume an `else` if one belongs to the `if` just parsed.
    ///
    /// Inside a group, R eats the newlines before `else`; at top level it does
    /// not, which is why `if (x) a` / newline / `else b` is an error there and
    /// legal inside `{ }`. Newlines are only consumed once an `else` is known
    /// to follow, so an `if` that ends a block leaves its separator alone.
    fn take_else(&mut self) -> bool {
        if self.newline_terminates.len() <= 1 {
            return self.eat(TokenKind::Else);
        }
        let mut lookahead = self.position;
        while self.tokens.get(lookahead).map(|token| token.kind) == Some(TokenKind::Newline) {
            lookahead += 1;
        }
        if self.tokens.get(lookahead).map(|token| token.kind) == Some(TokenKind::Else) {
            self.position = lookahead + 1;
            true
        } else {
            false
        }
    }

    fn for_loop(&mut self, token: Token) -> Parsed<Node<'a>> {
        self.position += 1;
        self.skip_newlines();
        self.expect(TokenKind::LParen)?;
        self.newline_terminates.push(false);
        self.skip_newlines();
        let variable = self.expect(TokenKind::Identifier)?;
        self.skip_newlines();
        self.expect(TokenKind::In)?;
        let sequence = self.expression(0, false)?;
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        self.newline_terminates.pop();
        self.skip_newlines();
        let body = self.expression(0, true)?;
        Ok(Node {
            start: token.start,
            end: body.end,
            kind: NodeKind::For {
                variable: &self.text[variable.start..variable.end],
                sequence: Box::new(sequence),
                body: Box::new(body),
            },
        })
    }

    fn while_loop(&mut self, token: Token) -> Parsed<Node<'a>> {
        self.position += 1;
        let cond = self.parenthesized_condition()?;
        self.skip_newlines();
        let body = self.expression(0, true)?;
        Ok(Node {
            start: token.start,
            end: body.end,
            kind: NodeKind::While {
                cond: Box::new(cond),
                body: Box::new(body),
            },
        })
    }

    /// R's `cond`/`ifcond`: `'(' expr ')'`, never `expr_or_assign`, so
    /// `if (x = 1)` is a syntax error rather than an assignment.
    fn parenthesized_condition(&mut self) -> Parsed<Node<'a>> {
        self.skip_newlines();
        self.expect(TokenKind::LParen)?;
        self.newline_terminates.push(false);
        let cond = self.expression(0, false)?;
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        self.newline_terminates.pop();
        Ok(cond)
    }

    /// R's `exprlist`: expressions separated by newlines or semicolons.
    fn block_body(&mut self) -> Parsed<Vec<Node<'a>>> {
        self.enter()?;
        let parsed = self.statements(TokenKind::RBrace);
        self.leave();
        parsed
    }

    /// The whole file, one top-level expression at a time.
    ///
    /// Every expression completed before an abstention is proven, so it is
    /// kept; the abstention says only that nothing *after* it was read. The
    /// parser never skips ahead to look for a next expression, so no anchor
    /// past the boundary can be invented.
    fn program(&mut self) -> (Vec<Node<'a>>, Option<Abstention>) {
        let mut items = Vec::new();
        loop {
            while matches!(
                self.peek(),
                Some(TokenKind::Newline) | Some(TokenKind::Semicolon)
            ) {
                self.position += 1;
            }
            if self.peek().is_none() {
                return (items, None);
            }
            self.depth = 0;
            match self.expression(0, true) {
                Ok(item) => {
                    let separated = matches!(
                        self.peek(),
                        None | Some(TokenKind::Newline) | Some(TokenKind::Semicolon)
                    );
                    items.push(item);
                    if !separated {
                        // Two expressions run together are not R.
                        return (items, Some(Abstention::unadmitted()));
                    }
                }
                Err(abstention) => return (items, Some(abstention)),
            }
        }
    }

    fn statements(&mut self, closer: TokenKind) -> Parsed<Vec<Node<'a>>> {
        let mut items = Vec::new();
        loop {
            while matches!(
                self.peek(),
                Some(TokenKind::Newline) | Some(TokenKind::Semicolon)
            ) {
                self.position += 1;
            }
            match self.peek() {
                None => break,
                Some(kind) if kind == closer => break,
                Some(_) => {}
            }
            let item = self.expression(0, true)?;
            items.push(item);
            // A complete expression must be followed by a separator; otherwise
            // two expressions were run together and this is not R.
            match self.peek() {
                None => break,
                Some(TokenKind::Newline) | Some(TokenKind::Semicolon) => {}
                Some(kind) if kind == closer => break,
                Some(_) => return Err(Abstention::unadmitted()),
            }
        }
        Ok(items)
    }
}

/// A string literal's own text, without its delimiters.
///
/// Both delimiter runs are ASCII by construction -- quotes, dashes, and one of
/// `()`, `[]`, `{}` -- so the slice always lands on character boundaries.
fn string_inner(text: &str, token: Token, raw: bool) -> &str {
    let slice = &text[token.start..token.end];
    if !raw {
        return slice
            .get(1..slice.len().saturating_sub(1))
            .unwrap_or_default();
    }
    let bytes = slice.as_bytes();
    let mut open = 2;
    while bytes.get(open) == Some(&b'-') {
        open += 1;
    }
    let dashes = open - 2;
    open += 1;
    let close = slice.len().saturating_sub(2 + dashes);
    if close < open {
        return "";
    }
    slice.get(open..close).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// What the tree proves
// ---------------------------------------------------------------------------

/// One admitted `test_that("...", { ... })` call.
struct TestThatBlock {
    start: usize,
    end: usize,
}

struct Program {
    anchors: Vec<TestThatBlock>,
    /// The file binds the name `test_that` itself.
    rebinds_callee: bool,
    /// An otherwise admitted call is reached only under a runtime condition.
    conditional_registration: bool,
    abstention: Option<Abstention>,
}

fn parse_program(text: &str) -> Program {
    let (tokens, lex_abstention) = lex(text);
    let mut parser = Parser {
        text,
        tokens,
        position: 0,
        depth: 0,
        newline_terminates: vec![true],
    };
    let (top_level, parse_abstention) = parser.program();
    let mut anchors = Vec::new();
    let mut rebinds_callee = false;
    let mut conditional_registration = false;
    for expression in &top_level {
        if admitted_anchor(expression) {
            anchors.push(TestThatBlock {
                start: expression.start,
                end: expression.end,
            });
        }
        if matches!(
            expression.kind,
            NodeKind::If { .. }
                | NodeKind::For { .. }
                | NodeKind::While { .. }
                | NodeKind::Repeat(_)
        ) {
            expression.visit(&mut |node| {
                conditional_registration |= admitted_anchor(node);
            });
        }
        expression.visit(&mut |node| {
            rebinds_callee |= binds_callee(node);
        });
    }

    Program {
        anchors,
        rebinds_callee,
        conditional_registration,
        abstention: parse_abstention.or(lex_abstention),
    }
}

/// The ADR-0042 D2 shape, asked of the tree.
///
/// A partially parsed or recovered node can never reach here: the parser
/// returns whole expressions or nothing.
fn admitted_anchor(node: &Node<'_>) -> bool {
    let NodeKind::Call { callee, args } = &node.kind else {
        return false;
    };
    // A bare identifier callee only: not `testthat::test_that`, not
    // `obj$test_that`, and not the backtick spelling `` `test_that` ``.
    if !matches!(callee.kind, NodeKind::Identifier(name) if name == CALLEE) {
        return false;
    }
    if args.len() != 2 || args.iter().any(|arg| arg.named) {
        return false;
    }
    let Some(description) = args[0].value.as_ref() else {
        return false;
    };
    // An empty description distinguishes nothing, and a computed or raw-string
    // one is not the source-visible literal this anchor claims.
    if !matches!(
        description.kind,
        NodeKind::Str {
            raw: false,
            inner
        } if !inner.is_empty()
    ) {
        return false;
    }
    matches!(
        args[1].value.as_ref().map(|value| &value.kind),
        Some(NodeKind::Brace(_))
    )
}

/// True when this node binds the name `test_that`.
///
/// `test_that` is an ordinary binding in R, not a keyword, so a file that
/// assigns it has changed what every call in it means. Every syntactic binding
/// form is covered: both assignment directions, `=`, the superassignments,
/// `for`'s loop variable, and a literal `assign("test_that", ...)`.
fn binds_callee(node: &Node<'_>) -> bool {
    match &node.kind {
        NodeKind::Binary {
            op: TokenKind::Assign | TokenKind::EqAssign,
            lhs,
            ..
        } => names_callee(lhs),
        NodeKind::Binary {
            op: TokenKind::RightAssign,
            rhs,
            ..
        } => names_callee(rhs),
        NodeKind::For { variable, .. } => *variable == CALLEE,
        NodeKind::Call { callee, args } => {
            matches!(callee.kind, NodeKind::Identifier(name) if name == "assign")
                && args
                    .first()
                    .and_then(|arg| arg.value.as_ref())
                    .is_some_and(names_callee)
        }
        _ => false,
    }
}

/// True when the node spells the name `test_that`, as a symbol, as the string
/// literal R also accepts on the left of an assignment, or as the backtick
/// spelling of the same symbol.
fn names_callee(node: &Node<'_>) -> bool {
    match node.kind {
        NodeKind::Identifier(name) | NodeKind::QuotedName(name) => name == CALLEE,
        NodeKind::Str { inner, .. } => inner == CALLEE,
        _ => false,
    }
}

fn anchor_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(R_TEST_THAT_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded R testthat test_that block anchor",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "r_anchor_kind=test_that_block".to_string(),
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
            format!("r_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: R_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: R_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn declared() -> ParserProjectContext {
        ParserProjectContext {
            r_declares_testthat: true,
            ..ParserProjectContext::default()
        }
    }

    fn output_with(context: &ParserProjectContext, text: &str) -> SourceParseOutput {
        RTestThatParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/testthat/test-catalog.R",
                    language: Language::R,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                context,
            )
            .expect("parse R testthat source")
    }

    fn output(text: &str) -> SourceParseOutput {
        output_with(&declared(), text)
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(R_TEST_THAT_TARGET))
            .count()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("r_unknown_kind=").map(String::from))
            .collect()
    }

    #[test]
    fn admitted_top_level_blocks_anchor() {
        let parsed = output(
            "test_that(\"loads the catalog\", {\n  expect_true(TRUE)\n})\n\n\
             test_that('filters the catalog', {\n  expect_equal(1, 1)\n})\n",
        );
        assert_eq!(anchors(&parsed), 2);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::RTestThatBlock)
                .count(),
            2
        );
    }

    #[test]
    fn without_a_declared_testthat_dependency_nothing_anchors() {
        let parsed = output_with(
            &ParserProjectContext::default(),
            "test_that(\"loads\", {\n  expect_true(TRUE)\n})\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"testthat_not_declared".to_string()));
    }

    #[test]
    fn call_text_in_comments_strings_and_raw_strings_never_anchors() {
        let parsed = output(
            "# test_that(\"commented\", { })\n\
             x <- \"test_that('quoted', { })\"\n\
             y <- r\"(test_that('raw', { }))\"\n\
             z <- r\"---[test_that('dashed raw', { })]---\"\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn nested_and_namespaced_calls_are_not_admitted() {
        for source in [
            // nested inside another call
            "wrapper(test_that(\"nested\", { }))",
            // nested inside a block
            "local({\n  test_that(\"nested\", { })\n})",
            // namespaced callee
            "testthat::test_that(\"namespaced\", { })",
            // member callee
            "obj$test_that(\"member\", { })",
            // a longer identifier that merely contains the name
            "my_test_that(\"lookalike\", { })",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn only_a_literal_description_and_a_brace_block_are_admitted() {
        for source in [
            // computed description
            "test_that(paste0(\"a\", \"b\"), { })",
            // variable description
            "test_that(description, { })",
            // empty description
            "test_that(\"\", { })",
            // second argument is not a block
            "test_that(\"desc\", expect_true(TRUE))",
            // a third argument
            "test_that(\"desc\", { }, extra)",
            // no arguments
            "test_that()",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn braces_inside_strings_do_not_close_the_block() {
        let parsed = output(
            "test_that(\"loads\", {\n  x <- \"}\"\n  expect_true(TRUE)\n})\n\n\
             test_that(\"filters\", {\n  expect_true(TRUE)\n})\n",
        );
        assert_eq!(
            anchors(&parsed),
            2,
            "a brace in a string must not end a block"
        );
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output("test_that(\"loads\", { expect_true(TRUE) })\n");
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn an_unclosed_quote_or_brace_reports_a_degraded_parse() {
        let parsed =
            output("test_that(\"loads\", {\n  expect_true(TRUE)\n})\ntest_that(\"unclosed\", {\n");
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
        // The anchor found before the violation is still real.
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn a_well_formed_test_file_reports_no_diagnostic() {
        let parsed = output("test_that(\"loads\", {\n  expect_true(TRUE)\n})\n");
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn only_the_testthat_runner_paths_are_admitted() {
        let parser = RTestThatParser;
        for path in [
            "R/catalog.R",
            "tests/test-catalog.R",
            "tests/testthat/helper-catalog.R",
            "tests/testthat/catalog.R",
            "testthat/test-catalog.R",
        ] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::R,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text: "test_that(\"x\", { })\n",
                }),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
        assert!(is_testthat_path("tests/testthat/test-catalog.R"));
        assert!(is_testthat_path("pkg/tests/testthat/test-catalog.r"));
    }

    #[test]
    fn ir_contains_edges_link_blocks_to_their_module() {
        let parsed = output("test_that(\"a\", { })\ntest_that(\"b\", { })\n");
        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert_eq!(parsed.report.ir_edges.len(), 2);
    }

    // --- ADR-0042 D2a: only a top-level *expression* that is itself the call ---

    #[test]
    fn a_call_that_is_not_a_top_level_expression_never_anchors() {
        for source in [
            // Reached only when a runtime condition holds.
            "if (interactive()) test_that(\"conditional\", { })",
            "if (Sys.getenv(\"CI\") == \"true\") test_that(\"ci only\", { })",
            // Run an unknown number of times, or none.
            "for (index in seq_len(n)) test_that(\"looped\", { })",
            "while (more()) test_that(\"looped\", { })",
            // The top-level expression is an assignment, not the call.
            "result <- test_that(\"captured\", { })",
            "test_that(\"captured\", { }) -> result",
            // The top-level expression is a function definition.
            "runner <- function() test_that(\"deferred\", { })",
            // The top-level expression is a binary operator.
            "enabled && test_that(\"guarded\", { })",
            "test_that(\"piped\", { }) |> invisible()",
            // R desugars `x |> f(y)` to `f(x, y)`, so R's own tree here is the
            // admitted one. This frontend does not desugar, so it under-anchors
            // rather than reconstructing a call the source does not spell.
            "\"piped description\" |> test_that({ })",
            // A continued expression is one expression, not two.
            "test_that(\"continued\", { }) %>%\n  invisible()",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn a_runtime_conditional_call_records_its_own_non_blocking_unknown() {
        let parsed = output("if (interactive()) test_that(\"conditional\", { })\n");
        assert_eq!(anchors(&parsed), 0);
        assert!(
            unknown_kinds(&parsed).contains(&"conditional_test_registration".to_string()),
            "{:?}",
            unknown_kinds(&parsed)
        );
    }

    #[test]
    fn a_backtick_quoted_callee_is_not_a_bare_identifier() {
        let parsed = output("`test_that`(\"quoted callee\", { })\n");
        assert_eq!(anchors(&parsed), 0);
    }

    #[test]
    fn named_arguments_are_not_the_admitted_positional_shape() {
        for source in [
            "test_that(desc = \"named\", code = { })",
            "test_that(\"named\", code = { })",
            "test_that(code = { }, desc = \"named\")",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn a_raw_string_description_is_not_admitted() {
        let parsed = output("test_that(r\"(raw description)\", { })\n");
        assert_eq!(anchors(&parsed), 0);
    }

    // --- ADR-0042 D2b: a file that rebinds the callee proves nothing ---

    #[test]
    fn a_file_that_binds_test_that_anchors_nothing() {
        for source in [
            "test_that <- function(desc, code) invisible(NULL)",
            "test_that = function(desc, code) invisible(NULL)",
            "function(desc, code) invisible(NULL) -> test_that",
            "test_that <<- function(desc, code) invisible(NULL)",
            "assign(\"test_that\", function(desc, code) invisible(NULL))",
            "for (test_that in seq_len(2)) invisible(test_that)",
        ] {
            let parsed = output(&format!(
                "{source}\n\
                 test_that(\"a\", {{ }})\ntest_that(\"b\", {{ }})\ntest_that(\"c\", {{ }})\n"
            ));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
            assert!(
                unknown_kinds(&parsed).contains(&"test_that_rebound_in_file".to_string()),
                "{source}: {:?}",
                unknown_kinds(&parsed)
            );
        }
    }

    // --- ADR-0042 D4: expression boundaries follow R's newline rule ---

    #[test]
    fn a_newline_ends_a_complete_expression() {
        // `test_that(...)` then `(x)` is two expressions. Reading them as a
        // call-of-a-call would silently lose the anchor.
        let parsed = output("test_that(\"a\", { })\n(x)\n");
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn a_trailing_operator_continues_across_a_newline() {
        // `1 -` continues, so this file holds one expression and no anchor
        // boundary is invented after it.
        let parsed = output("x <- 1 -\n  2\ntest_that(\"a\", { })\n");
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_leading_operator_starts_a_new_expression() {
        let parsed = output("x <- 1\n-2\ntest_that(\"a\", { })\n");
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn semicolon_separated_top_level_calls_both_anchor() {
        let parsed = output("test_that(\"a\", { }); test_that(\"b\", { })\n");
        assert_eq!(anchors(&parsed), 2);
    }

    // --- ADR-0042 D6: the declared subset, and abstention outside it ---

    #[test]
    fn r_4_x_syntax_inside_an_admitted_block_still_parses() {
        for body in [
            // R 4.1 lambda.
            "f <- \\(x) x + 1\n  expect_equal(f(1), 2)",
            // R 4.1 native pipe.
            "1:3 |> sum() |> expect_equal(6L)",
            // R 4.0 raw string.
            "expect_equal(r\"(a\\b)\", \"a\\\\b\")",
            // Empty arguments and named arguments.
            "m <- matrix(1:4, nrow = 2)\n  expect_equal(dim(m[, 1, drop = FALSE]), c(2L, 1L))",
            // Special operators and `if`/`else` across newlines inside a block.
            "expect_true(2 %in% 1:3)\n  if (TRUE) {\n    expect_true(TRUE)\n  } else {\n    expect_true(FALSE)\n  }",
            // Repeat/break, double-bracket indexing, formulas, namespaces.
            "repeat {\n    break\n  }\n  expect_equal(list(a = 1)[[\"a\"]], 1)\n  expect_s3_class(y ~ x, \"formula\")\n  expect_true(base::isTRUE(TRUE))",
        ] {
            let parsed = output(&format!("test_that(\"admitted\", {{\n  {body}\n}})\n"));
            assert_eq!(anchors(&parsed), 1, "must anchor: {body}");
            assert!(
                parsed.report.diagnostics.is_empty(),
                "must not degrade: {body}"
            );
        }
    }

    #[test]
    fn a_construct_outside_the_declared_subset_degrades_and_never_recovers() {
        for source in [
            // Option-selected pipe bind syntax (`_R_USE_PIPEBIND_`).
            "x => f()",
            // Chained comparison is not associative in R.
            "flag <- a < b < c",
            // Locale-dependent identifier bytes.
            "r\u{e9}sultat <- 1",
            // A stray byte that begins no R token.
            "x <- @",
        ] {
            let parsed = output(&format!(
                "test_that(\"before\", {{ }})\n{source}\ntest_that(\"after\", {{ }})\n"
            ));
            assert_eq!(
                anchors(&parsed),
                1,
                "only the proven prefix may anchor: {source}"
            );
            assert_eq!(parsed.report.diagnostics.len(), 1, "{source}");
            assert!(
                unknown_kinds(&parsed).contains(&"unadmitted_r_construct".to_string()),
                "{source}: {:?}",
                unknown_kinds(&parsed)
            );
        }
    }

    #[test]
    fn non_ascii_bytes_in_comments_and_strings_stay_admitted() {
        let parsed = output(
            "# caf\u{e9} \u{2014} a comment\n\
             note <- \"caf\u{e9}\"\n\
             test_that(\"a\", { })\n",
        );
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_dangling_else_at_top_level_degrades_rather_than_anchoring_past_it() {
        let parsed = output(
            "test_that(\"before\", { })\n\
             if (TRUE) 1\n\
             else 2\n\
             test_that(\"after\", { })\n",
        );
        assert_eq!(anchors(&parsed), 1);
        assert_eq!(parsed.report.diagnostics.len(), 1);
    }

    #[test]
    fn a_same_line_else_at_top_level_keeps_the_if_whole() {
        // `} else {` is R's ordinary spelling, and it is legal at top level
        // precisely because no newline separates the brace from the `else`.
        // Getting this wrong would split one `if` into two expressions and
        // could anchor a conditionally reached call.
        let parsed = output(
            "if (interactive()) {\n\
             \x20 test_that(\"interactive only\", { })\n\
             } else {\n\
             \x20 test_that(\"batch only\", { })\n\
             }\n\
             test_that(\"always\", { })\n",
        );
        assert!(parsed.report.diagnostics.is_empty());
        assert_eq!(anchors(&parsed), 1, "only the unconditional call anchors");
        assert!(unknown_kinds(&parsed).contains(&"conditional_test_registration".to_string()));
    }

    #[test]
    fn nesting_beyond_the_bounded_depth_degrades_instead_of_recursing() {
        let deep = format!(
            "test_that(\"before\", {{ }})\nx <- {}1{}\n",
            "(".repeat(4_096),
            ")".repeat(4_096)
        );
        let parsed = output(&deep);
        assert_eq!(anchors(&parsed), 1);
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert!(unknown_kinds(&parsed).contains(&"parser_depth_limit".to_string()));
    }

    #[test]
    fn an_ordinary_multi_line_test_file_parses_whole() {
        // Abstention is safe but it is not free: a subset too small to read the
        // shapes real testthat files use would silently report no tests.
        let parsed = output(
            "library(testthat)\n\
             \n\
             build_catalog <- function(rows = 3,\n\
             \x20                     name = \"demo\") {\n\
             \x20 data.frame(\n\
             \x20   id = seq_len(rows),\n\
             \x20   name = rep(name, rows),\n\
             \x20   stringsAsFactors = FALSE\n\
             \x20 )\n\
             }\n\
             \n\
             test_that(\"loads the catalog\", {\n\
             \x20 catalog <- build_catalog()\n\
             \x20 expect_equal(nrow(catalog), 3L)\n\
             \x20 expect_identical(catalog[[\"id\"]][1], 1L)\n\
             })\n\
             \n\
             test_that(\"filters the catalog\", {\n\
             \x20 catalog <- build_catalog(rows = 5)\n\
             \x20 kept <- catalog[catalog$id %% 2 == 1, , drop = FALSE]\n\
             \x20 if (nrow(kept) > 0) {\n\
             \x20   expect_true(all(kept$id %% 2 == 1))\n\
             \x20 } else {\n\
             \x20   fail(\"nothing kept\")\n\
             \x20 }\n\
             })\n\
             \n\
             test_that(\"summarises the catalog\", {\n\
             \x20 total <- build_catalog()$id |> sum()\n\
             \x20 expect_equal(total, 6L)\n\
             })\n",
        );
        assert!(
            parsed.report.diagnostics.is_empty(),
            "{:?}",
            parsed.report.diagnostics
        );
        assert_eq!(anchors(&parsed), 3);
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn an_escaped_quote_inside_a_description_stays_one_literal() {
        let parsed = output("test_that(\"has a \\\" quote\", { })\n");
        assert_eq!(anchors(&parsed), 1);
    }
}
