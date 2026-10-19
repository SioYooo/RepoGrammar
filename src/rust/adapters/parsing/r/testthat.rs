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
//! The syntax layer is the pinned Tree-sitter R grammar (ADR-0058), read off
//! the tree rather than scanned as text. The ADR-0042 lexer stays in front as
//! the lexical-subset gate: it refuses the bytes and constructs outside the
//! declared R subset -- unterminated quotes, malformed numbers and raw strings,
//! `?`, `=>`, `_`, and any non-ASCII byte in code position -- and everything
//! before the refusal is still read. The anchor is then a question about the
//! tree: *is this top-level expression itself the call?*
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
//! Outside the declared subset the frontend abstains: it never guesses, never
//! recovers, and never resynchronizes to a later expression. Any Tree-sitter
//! `ERROR` or `MISSING` node ends the read at its top-level expression, and the
//! grammar's wider acceptance (`:=`, `**`, chained comparison, an assignment
//! `=` inside an argument list, `x$` with no name, two expressions on one line)
//! is refused where the legacy parser stopped. Input is untrusted, so both the
//! input size and the legacy recursion depth are bounded; the tree is walked
//! with an explicit stack, never by recursion.
//!
//! Every typed `UNKNOWN` this frontend can emit is declared exactly once in
//! [`R_OBLIGATION_REGISTRY`], the lane's ADR-0020 gate 4 source-semantic
//! obligation registry: the claim each unknown scopes, whether an unmet
//! obligation blocks the family claim, and the provider-fallback policy that
//! says what could discharge it. Nothing outside the bounded parse may turn a
//! registry entry into certainty, and a count may fall only when a
//! source-backed replacement fact discharges the same obligation.

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
use tree_sitter::{Node as TsNode, Parser as TsParser};

pub const R_ANCHOR_ENGINE: &str = "repogrammar-r-testthat-parser";
pub const R_ANCHOR_METHOD: &str = "bounded_r_test_that_v2";

/// Fixed support target for the one admitted exact anchor.
pub const R_TEST_THAT_TARGET: &str = "testthat.test_that";

const CALLEE: &str = "test_that";
const MAX_BLOCK_UNITS: usize = 4_096;
/// Bound on the legacy recursive-descent nesting depth, which the Tree-sitter
/// read reproduces. Input is untrusted, and a file of
/// nothing but open parentheses must abstain rather than exhaust the stack.
const MAX_PARSE_DEPTH: usize = 128;

/// One source-semantic obligation the admitted R family claim rests on.
///
/// This is the lane's ADR-0020 gate 4 registry. Every typed `UNKNOWN` the
/// testthat frontend emits is declared here exactly once, with the claim it
/// scopes, its reason code, whether an unmet obligation blocks the family
/// claim, and the provider-fallback policy that names what could discharge it.
/// `application/family.rs` stays the authoritative claim-impact classifier;
/// this record must agree with it, and the registry tests below pin the
/// emitted vocabulary to this table so the two cannot drift silently.
pub(crate) struct RObligation {
    /// Stable affected-claim token (`affected_claim=` assumption).
    pub(crate) affected_claim: &'static str,
    /// Stable kind token (`r_unknown_kind=` assumption).
    pub(crate) kind: &'static str,
    /// Stable protocol reason code (the fact target).
    pub(crate) reason: UnknownReasonCode,
    /// A standing obligation rides on every admitted anchor because the
    /// bounded parse can never discharge it; a triggered one fires only on
    /// the construct that leaves it unmet.
    pub(crate) standing: bool,
    /// Recorded claim impact of an unmet obligation. Must agree with
    /// `r_unknown_reason_blocks_family_membership` and
    /// `r_unknown_is_non_blocking_family_subclaim` in
    /// `src/rust/application/family.rs`.
    pub(crate) blocks_family_claim: bool,
    /// Source-free, fixed human-facing evidence note.
    pub(crate) note: &'static str,
    /// Provider-fallback policy as a stable low-cardinality token, mirrored on
    /// every emitted fact as the `provider_fallback=` assumption. The prose
    /// policy each token stands for is the comment above the registry entry.
    pub(crate) fallback: &'static str,
}

/// The complete ADR-0020 gate 4 source-semantic obligation registry for the
/// `testthat.test_that` family claim.
///
/// Obligations split three ways. The identity obligations block the family
/// claim when unmet: ADR-0042 D3 makes the DESCRIPTION declaration a
/// precondition, and D2b makes an in-file rebinding an unproof of the same
/// identity. The reach obligations record registrations this frontend
/// deliberately does not anchor. The remaining obligations are residuals the
/// bounded parse can never discharge -- callee binding beyond the file, the
/// quoted block's own semantics, dispatch, and external symbols -- so they
/// ride along as non-blocking unknowns rather than being guessed away.
///
/// Fallback tokens: `repository_metadata_declaration` means only the
/// repository's own DESCRIPTION can discharge the obligation, and no provider
/// can substitute for it. `runtime_trace_not_integrated` and
/// `runtime_observation_not_integrated` mean a bounded trace would settle the
/// claim but no trace mechanism is authorized or integrated.
/// `source_inside_declared_subset_only` means the parse can only read source
/// the ADR-0042 D4 subset admits and never recovers past a refusal.
/// `bounded_input_refusal` means the untrusted-input bounds refused the file
/// and no provider is involved. `source_only_shape_widening_needs_adr` means
/// the admitted shape is a decision, and widening it needs a superseding
/// ADR rather than a provider. `no_r_provider_irreducible` means the
/// obligation needs an R semantic provider or evaluator, no R provider slot
/// exists or is registered, and ADR-0036 forbids executing R -- so the
/// residual is irreducible under current constraints.
pub(crate) const R_OBLIGATION_REGISTRY: &[RObligation] = &[
    // Fallback: recoverable by repository metadata only -- declare testthat in
    // a DESCRIPTION dependency field. No provider can substitute for the
    // repository's own declaration.
    RObligation {
        affected_claim: "r_testthat_identity",
        kind: "testthat_not_declared",
        reason: UnknownReasonCode::MissingDependency,
        standing: false,
        blocks_family_claim: true,
        note: "no DESCRIPTION in this repository declares testthat, so the framework identity is unproven",
        fallback: "repository_metadata_declaration",
    },
    // Fallback: irreducible without executing R, which ADR-0036 forbids; a
    // bounded runtime trace is the only discharge and none is integrated.
    RObligation {
        affected_claim: "r_testthat_identity",
        kind: "test_that_rebound_in_file",
        reason: UnknownReasonCode::MonkeyPatch,
        standing: false,
        blocks_family_claim: true,
        note: "this file binds the name test_that itself, so a call to it is not proven to be testthat's",
        fallback: "runtime_trace_not_integrated",
    },
    // Fallback: recoverable only by observing whether the runtime condition
    // holds. No observation mechanism is integrated, so support is
    // deliberately understated rather than guessed.
    RObligation {
        affected_claim: "r_conditional_test_registration",
        kind: "conditional_test_registration",
        reason: UnknownReasonCode::BuildVariantAmbiguity,
        standing: false,
        blocks_family_claim: false,
        note: "an otherwise admitted test_that call is reached only under a runtime condition or loop",
        fallback: "runtime_observation_not_integrated",
    },
    // Fallback: source-only obligation. The description must be read in the
    // file itself; widening the admitted description shape is a superseding
    // decision, not a provider question.
    RObligation {
        affected_claim: "r_testthat_description_literal",
        kind: "description_not_source_literal",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: false,
        blocks_family_claim: false,
        note: "a top-level test_that call's description is not a source-visible plain string literal, so the call was not anchored",
        fallback: "source_only_shape_widening_needs_adr",
    },
    // Fallback: recoverable only by source inside the declared ADR-0042 D4
    // subset. The parse never recovers past the boundary.
    RObligation {
        affected_claim: "r_test_parse",
        kind: "unadmitted_r_construct",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "R source left the declared parsed subset, so no later expression in this file was read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: bounded-input refusal on untrusted nesting. No provider is
    // involved.
    RObligation {
        affected_claim: "r_test_parse",
        kind: "parser_depth_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "R source nested past the bounded parse depth, so no later expression in this file was read",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted size. No provider is
    // involved.
    RObligation {
        affected_claim: "r_test_parse",
        kind: "source_byte_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "R source exceeded the bounded input-byte limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted unit counts. No provider is
    // involved.
    RObligation {
        affected_claim: "r_test_parse",
        kind: "parser_resource_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "R parser exceeded the bounded block limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: the anchor proves the call shape, not that the bare name
    // resolves to testthat's function. Binding by library masking, sourced
    // helpers, attach, or enclosing environments needs an evaluator, no R
    // provider slot exists or is registered, and ADR-0036 forbids executing
    // R. Irreducible under current constraints.
    RObligation {
        affected_claim: "r_testthat_callee_binding",
        kind: "callee_binding_unproven",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: true,
        blocks_family_claim: false,
        note: "the anchor proves the call shape, not that the bare name test_that resolves to testthat's function in the runtime search path",
        fallback: "no_r_provider_irreducible",
    },
    // Fallback: test_that quotes its code block, so its non-standard
    // evaluation, assertions, data masking, and skips need an evaluator
    // ADR-0036 forbids. No R provider slot exists or is registered.
    // Irreducible under current constraints.
    RObligation {
        affected_claim: "r_testthat_block_semantics",
        kind: "block_nse_not_evaluated",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: true,
        blocks_family_claim: false,
        note: "test_that quotes its code block, so non-standard evaluation, assertions, data masking, and skips inside it are not evaluated and no symbol inside the block is resolved",
        fallback: "no_r_provider_irreducible",
    },
    // Fallback: S3/S4 method tables are runtime type state. No R runtime or
    // type provider exists, so this is irreducible under current constraints.
    RObligation {
        affected_claim: "r_dispatch_target",
        kind: "s3_s4_dispatch_call",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: false,
        blocks_family_claim: false,
        note: "a parsed call registers or dispatches an S3 or S4 generic whose method selection is runtime type state",
        fallback: "no_r_provider_irreducible",
    },
    // Fallback: package attach, namespace exports, and native entry points
    // need an installed-graph provider. None exists, so this is irreducible
    // under current constraints.
    RObligation {
        affected_claim: "r_external_symbol_resolution",
        kind: "native_or_package_symbol_call",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: false,
        blocks_family_claim: false,
        note: "a parsed call resolves a package, namespace, or native symbol this frontend cannot bind",
        fallback: "no_r_provider_irreducible",
    },
];

fn registry_entry(kind: &str) -> Result<&'static RObligation, ParseError> {
    R_OBLIGATION_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind)
        .ok_or_else(|| ParseError::Internal(format!("unregistered R obligation kind {kind}")))
}

fn standing_obligations() -> impl Iterator<Item = &'static RObligation> {
    R_OBLIGATION_REGISTRY.iter().filter(|entry| entry.standing)
}

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
    build_output(document, context, parse_program)
}

fn build_output(
    document: SourceDocument<'_>,
    context: &ParserProjectContext,
    read_program: impl FnOnce(&str) -> Result<Program, ParseError>,
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
        facts.push(obligation_fact(
            &module,
            registry_entry("source_byte_limit")?,
            full_range,
        )?);
        return finish(units, facts, Vec::new());
    }

    if !context.r_declares_testthat {
        // A directory named tests/testthat in a project that does not depend on
        // testthat establishes nothing.
        facts.push(obligation_fact(
            &module,
            registry_entry("testthat_not_declared")?,
            full_range,
        )?);
        return finish(units, facts, Vec::new());
    }

    let program = read_program(document.text)?;

    // Leaving the declared subset is a decidable, source-visible event, and it
    // makes a missing anchor uninformative: fewer admitted calls after that
    // point is indistinguishable from a file that simply has fewer calls. The
    // expressions already proven are kept; the diagnostic states that the ones
    // not found prove nothing. The parser never resynchronizes past the
    // boundary, so nothing after it is guessed.
    let mut diagnostics = Vec::new();
    if let Some(abstention) = program.abstention {
        diagnostics.push(degraded(document.path, abstention.diagnostic()));
        facts.push(obligation_fact(
            &module,
            registry_entry(abstention.unknown_kind())?,
            module.range.clone(),
        )?);
    }

    if program.rebinds_callee {
        // `test_that` is an ordinary R binding, not a keyword. A file that
        // assigns the name itself has redefined what every later call in it
        // means, and RepoGrammar cannot evaluate the replacement. Nothing in
        // the file anchors.
        facts.push(obligation_fact(
            &module,
            registry_entry("test_that_rebound_in_file")?,
            full_range,
        )?);
        return finish(units, facts, diagnostics);
    }

    if program.conditional_registration {
        // `if (interactive())` and `for (...)` decide at run time whether -- and
        // how often -- a test is registered, and RepoGrammar evaluates neither.
        // The call is not admitted, and saying so understates support rather
        // than unproving the unconditional calls beside it.
        facts.push(obligation_fact(
            &module,
            registry_entry("conditional_test_registration")?,
            module.range.clone(),
        )?);
    }

    if program.description_not_literal {
        // The description is the only thing distinguishing one anchor from
        // another, so a computed or raw-string one is not source-visible and
        // the call is not anchored (ADR-0042 D2). Recording why keeps the
        // understatement honest instead of silent.
        facts.push(obligation_fact(
            &module,
            registry_entry("description_not_source_literal")?,
            module.range.clone(),
        )?);
    }

    if program.dispatch_call {
        facts.push(obligation_fact(
            &module,
            registry_entry("s3_s4_dispatch_call")?,
            module.range.clone(),
        )?);
    }

    if program.external_symbol_call {
        facts.push(obligation_fact(
            &module,
            registry_entry("native_or_package_symbol_call")?,
            module.range.clone(),
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
        // The standing obligations ride on every admitted anchor: the bounded
        // parse proves the call shape and nothing beyond it, so what the bare
        // callee binds to and what the quoted block means stay typed `UNKNOWN`
        // on the anchor itself rather than being guessed away. They are
        // recorded residuals and must never block the family claim.
        for entry in standing_obligations() {
            debug_assert!(
                !entry.blocks_family_claim,
                "a standing R obligation is a recorded residual and must not block"
            );
            facts.push(obligation_fact(&unit, entry, range.clone())?);
        }
        units.push(unit);
    }

    if limit_hit {
        facts.push(obligation_fact(
            &module,
            registry_entry("parser_resource_limit")?,
            module.range.clone(),
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
    /// A top-level two-positional-argument call whose description is not a
    /// source-visible plain string literal.
    description_not_literal: bool,
    /// A parsed call registers or dispatches an S3/S4 generic.
    dispatch_call: bool,
    /// A parsed call resolves a package, namespace, or native symbol.
    external_symbol_call: bool,
    abstention: Option<Abstention>,
}

fn refuse<T>() -> Result<T, Abstention> {
    Err(Abstention::unadmitted())
}

/// Everything one top-level expression contributes to the file's [`Program`].
#[derive(Default)]
struct ItemFacts {
    anchor: bool,
    description_not_literal: bool,
    conditional_registration: bool,
    rebinds_callee: bool,
    dispatch_call: bool,
    external_symbol_call: bool,
}

fn parse_program(text: &str) -> Result<Program, ParseError> {
    // The lexer is the lexical-subset gate (ADR-0042 D4): bytes and constructs
    // outside the subset end the parse where the lexer stopped, and everything
    // before that point is still read. The proven prefix is exactly the text up
    // to the last accepted token.
    let (tokens, lex_abstention) = lex(text);
    let proven = match lex_abstention {
        Some(_) => tokens.last().map_or(0, |token| token.end),
        None => text.len(),
    };
    let source = &text[..proven];

    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_r::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load R grammar: {error}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter R parse failed".to_string()))?;

    let mut program = Program {
        anchors: Vec::new(),
        rebinds_callee: false,
        conditional_registration: false,
        description_not_literal: false,
        dispatch_call: false,
        external_symbol_call: false,
        abstention: None,
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    // The last item read is merged one step late, because a following token can
    // show that it never completed (see `continues_expression`).
    let mut pending: Option<(usize, usize, ItemFacts)> = None;
    for item in root.children(&mut cursor) {
        if item.kind() == "comment" {
            continue;
        }
        // A bare token at the top is error-recovery debris: the grammar placed
        // no expression there, so everything from it on is the refused region.
        let reached = if item.is_named() {
            item.end_byte()
        } else {
            root.end_byte()
        };
        // R needs a newline or `;` between two expressions; the grammar does
        // not, so the gap is checked here.
        if let Some((start, ..)) = pending
            .as_ref()
            .filter(|(_, end, _)| !separated(source, *end, item.start_byte()))
        {
            let start = *start;
            program.abstention = Some(depth_precedence(
                Abstention::unadmitted(),
                &tokens,
                start,
                reached,
            ));
            if continues_expression(&tokens, item.start_byte()) {
                // The legacy parse took this token as part of the previous
                // expression and failed inside it, so that expression is lost.
                pending = None;
            }
            break;
        }
        if !item.is_named() {
            program.abstention = Some(depth_precedence(
                Abstention::unadmitted(),
                &tokens,
                item.start_byte(),
                reached,
            ));
            break;
        }
        match analyze_item(source, item) {
            Ok(facts) => {
                if let Some((start, end, earlier)) = pending.take() {
                    program.merge(start, end, earlier);
                }
                pending = Some((item.start_byte(), item.end_byte(), facts));
            }
            Err(abstention) => {
                program.abstention = Some(error_precedence(abstention, item, &tokens));
                break;
            }
        }
    }
    if let Some((start, end, facts)) = pending.take() {
        program.merge(start, end, facts);
    }
    if program.abstention.is_none() {
        program.abstention = lex_abstention;
    }
    Ok(program)
}

impl Program {
    fn merge(&mut self, start: usize, end: usize, facts: ItemFacts) {
        if facts.anchor {
            self.anchors.push(TestThatBlock { start, end });
        }
        self.description_not_literal |= facts.description_not_literal;
        self.conditional_registration |= facts.conditional_registration;
        self.rebinds_callee |= facts.rebinds_callee;
        self.dispatch_call |= facts.dispatch_call;
        self.external_symbol_call |= facts.external_symbol_call;
    }
}

/// Whether the legacy parser would have taken the token at `at` as a
/// continuation of the expression before it: a postfix opener or any infix
/// operator. Tree-sitter closes an unfinished expression early and leaves such
/// a token as debris; the legacy parse failed inside the expression instead of
/// completing it, so the earlier expression contributes nothing.
fn continues_expression(tokens: &[Token], at: usize) -> bool {
    let Ok(index) = tokens.binary_search_by_key(&at, |token| token.start) else {
        return false;
    };
    matches!(
        tokens[index].kind,
        TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LDoubleBracket
            | TokenKind::Dollar
            | TokenKind::At
            | TokenKind::NsGet
            | TokenKind::Assign
            | TokenKind::EqAssign
            | TokenKind::RightAssign
            | TokenKind::Tilde
            | TokenKind::Or
            | TokenKind::And
            | TokenKind::Compare
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Special
            | TokenKind::Colon
            | TokenKind::Caret
    )
}

/// Keep the legacy precedence between the two refusal classes: the first one
/// in source order wins. Tree-sitter error recovery flattens unclosed nesting
/// into an `ERROR` node, so when an item is refused for an error alone, the
/// bracket nesting before the error is checked against the legacy depth bound.
fn error_precedence(abstention: Abstention, item: TsNode<'_>, tokens: &[Token]) -> Abstention {
    if abstention.reason != AbstainReason::UnadmittedConstruct || !item.has_error() {
        return abstention;
    }
    let mut stack = vec![item];
    let mut first_error = None;
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            // An `ERROR` node spans the tokens the grammar could not place.
            first_error = Some(node.end_byte());
            break;
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    match first_error {
        Some(until) => depth_precedence(abstention, tokens, item.start_byte(), until),
        None => abstention,
    }
}

fn depth_precedence(
    abstention: Abstention,
    tokens: &[Token],
    from: usize,
    until: usize,
) -> Abstention {
    if brackets_exceed_depth(tokens, from, until) {
        Abstention::depth_limit()
    } else {
        abstention
    }
}

/// One open bracket while estimating legacy depth.
struct BracketFrame {
    /// Depth the bracket itself adds.
    added: usize,
    /// Closers still owed; `[[` owes two `]`.
    owed: usize,
    /// A function's formals, whose `=` default enters one more level.
    formals: bool,
    default_open: bool,
}

/// Whether bracket nesting alone drives the legacy depth past
/// [`MAX_PARSE_DEPTH`] among the tokens in `from..until`: a call or index bracket
/// enters twice (arguments, then an element), a block twice (body, then a
/// statement), a grouping paren once, and a formal's default once.
fn brackets_exceed_depth(tokens: &[Token], from: usize, until: usize) -> bool {
    let mut frames: Vec<BracketFrame> = Vec::new();
    let mut depth = 1usize;
    let mut previous: Option<TokenKind> = None;
    let mut element_due = false;
    for token in tokens
        .iter()
        .filter(|token| token.start >= from && token.end <= until)
    {
        match token.kind {
            TokenKind::Newline | TokenKind::Semicolon => continue,
            TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                if let Some(frame) = frames.last_mut() {
                    if frame.default_open {
                        frame.default_open = false;
                        depth -= 1;
                    }
                }
                if token.kind == TokenKind::Comma {
                    element_due = !frames.is_empty();
                    previous = Some(token.kind);
                    continue;
                }
                if let Some(frame) = frames.pop() {
                    if frame.owed > 1 {
                        frames.push(BracketFrame {
                            owed: frame.owed - 1,
                            ..frame
                        });
                    } else {
                        depth -= frame.added;
                    }
                }
                element_due = false;
            }
            TokenKind::EqAssign if frames.last().is_some_and(|frame| frame.formals) => {
                if depth + 1 > MAX_PARSE_DEPTH {
                    return true;
                }
                if let Some(frame) = frames.last_mut() {
                    frame.default_open = true;
                }
                depth += 1;
                element_due = false;
            }
            TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LDoubleBracket
            | TokenKind::LBrace => {
                if element_due && depth > MAX_PARSE_DEPTH {
                    return true;
                }
                let operand_before = matches!(
                    previous,
                    Some(
                        TokenKind::Identifier
                            | TokenKind::BacktickName
                            | TokenKind::StringLiteral
                            | TokenKind::RawStringLiteral
                            | TokenKind::Number
                            | TokenKind::RParen
                            | TokenKind::RBracket
                            | TokenKind::RBrace
                    )
                );
                let formals = previous == Some(TokenKind::Function);
                let (added, owed) = match token.kind {
                    _ if formals => (0, 1),
                    TokenKind::LParen if !operand_before => (1, 1),
                    TokenKind::LDoubleBracket => (2, 2),
                    _ => (2, 1),
                };
                if !formals && depth + 1 > MAX_PARSE_DEPTH {
                    return true;
                }
                depth += added;
                frames.push(BracketFrame {
                    added,
                    owed,
                    formals,
                    default_open: false,
                });
                element_due = added == 2;
            }
            _ => {
                if element_due && depth > MAX_PARSE_DEPTH {
                    return true;
                }
                element_due = false;
            }
        }
        previous = Some(token.kind);
    }
    false
}

/// True when a newline or `;` separates two expressions. The gap holds only
/// whitespace and comments, and a comment always ends at a newline.
fn separated(text: &str, from: usize, to: usize) -> bool {
    let mut in_comment = false;
    for byte in text.as_bytes()[from..to].iter().copied() {
        match byte {
            b'\n' => return true,
            b';' if !in_comment => return true,
            b'#' => in_comment = true,
            _ => {}
        }
    }
    false
}

/// A node plus the legacy recursion depth and `=` rule at its position.
type Placed<'t> = (TsNode<'t>, usize, bool);

/// Read one top-level expression off the tree, iteratively.
///
/// Each node carries the recursion depth and the `=` rule the legacy parser had
/// at the same point: `depth` is the legacy `enter` count, and `allow_eq`
/// mirrors R's split between `expr_or_assign` and `expr`, so a Tree-sitter shape
/// the declared subset never parsed (an assignment `=` inside an argument list,
/// a chained comparison, `:=`, `**`, a `$` with no name) is refused here exactly
/// where the legacy parser stopped.
fn analyze_item(text: &str, item: TsNode<'_>) -> Result<ItemFacts, Abstention> {
    let mut facts = ItemFacts::default();
    let conditional_root = matches!(
        item.kind(),
        "if_statement" | "for_statement" | "while_statement" | "repeat_statement"
    );
    if item.kind() == "call" {
        if let Some(slots) = admitted_call_slots(text, item) {
            let brace_body = has_brace_body(slots[1]);
            facts.anchor = description_is_plain_literal(text, slots[0]) && brace_body;
            facts.description_not_literal =
                brace_body && !description_is_plain_literal(text, slots[0]);
        }
    }

    let mut stack: Vec<Placed<'_>> = vec![(item, 1, true)];
    while let Some((node, depth, allow_eq)) = stack.pop() {
        if node.is_error() || node.is_missing() {
            return refuse();
        }
        if !node.is_named() || node.kind() == "comment" {
            continue;
        }
        let kind = node.kind();
        if !matches!(kind, "argument" | "parameters" | "parameter") && depth > MAX_PARSE_DEPTH {
            return Err(Abstention::depth_limit());
        }
        // The legacy `block_body` enters one level below the brace itself.
        if kind == "braced_expression" && depth + 1 > MAX_PARSE_DEPTH {
            return Err(Abstention::depth_limit());
        }
        if kind == "string" && node.has_error() {
            return refuse();
        }
        // The grammar lets whitespace and comments sit inside `1 L` and `1 i`;
        // R's lexer does not, and the declared subset reads two tokens there.
        if matches!(kind, "integer" | "complex")
            && node_text(text, node)
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte == b'#')
        {
            return refuse();
        }
        if kind == "identifier" && is_reserved_word(node_text(text, node)) {
            return refuse();
        }
        match kind {
            "binary_operator" => {
                let operator = node.child_by_field_name("operator").map(|op| op.kind());
                match operator {
                    Some(":=" | "**" | "?") | None => return refuse(),
                    Some("=") if !allow_eq => return refuse(),
                    // R declares comparison non-associative, so `a < b < c` is a
                    // syntax error rather than a nested comparison.
                    Some(op) if is_comparison(op) => {
                        let chained = node
                            .child_by_field_name("lhs")
                            .filter(|lhs| lhs.kind() == "binary_operator")
                            .and_then(|lhs| lhs.child_by_field_name("operator"))
                            .is_some_and(|op| is_comparison(op.kind()));
                        if chained {
                            return refuse();
                        }
                    }
                    Some(_) => {}
                }
                match operator {
                    Some("<-" | "<<-" | "=") => {
                        facts.rebinds_callee |= node
                            .child_by_field_name("lhs")
                            .is_some_and(|lhs| names_callee(text, lhs));
                    }
                    Some("->" | "->>") => {
                        facts.rebinds_callee |= node
                            .child_by_field_name("rhs")
                            .is_some_and(|rhs| names_callee(text, rhs));
                    }
                    _ => {}
                }
            }
            "unary_operator" => {
                if node.child_by_field_name("operator").map(|op| op.kind()) == Some("?") {
                    return refuse();
                }
            }
            // `x$` and `pkg::` with the name missing parse in the grammar and
            // never in the declared subset.
            "extract_operator" | "namespace_operator" => {
                if node.child_by_field_name("rhs").is_none() {
                    return refuse();
                }
                if kind == "namespace_operator" {
                    facts.external_symbol_call = true;
                }
            }
            "for_statement" => {
                if let Some(variable) = node.child_by_field_name("variable") {
                    let name = node_text(text, variable);
                    // The loop variable is a bare symbol, never backticked.
                    if name.starts_with('`') {
                        return refuse();
                    }
                    facts.rebinds_callee |= name == CALLEE;
                }
            }
            "parameter" => {
                // A formal is a bare symbol, never backticked.
                if node
                    .child_by_field_name("name")
                    .is_some_and(|name| node_text(text, name).starts_with('`'))
                {
                    return refuse();
                }
            }
            "call" => {
                if let Some(function) = node
                    .child_by_field_name("function")
                    .filter(|function| function.kind() == "identifier")
                {
                    match node_text(text, function) {
                        "UseMethod" | "NextMethod" | "callGeneric" | "callNextMethod"
                        | "setMethod" | "setGeneric" | "setClass" | "setRefClass" => {
                            facts.dispatch_call = true;
                        }
                        "library" | "require" | ".Call" | ".C" | ".Fortran" | ".External" => {
                            facts.external_symbol_call = true;
                        }
                        "assign" => {
                            facts.rebinds_callee |= call_slots(node)
                                .first()
                                .copied()
                                .flatten()
                                .and_then(|slot| slot.child_by_field_name("value"))
                                .is_some_and(|value| names_callee(text, value));
                        }
                        _ => {}
                    }
                }
                if conditional_root {
                    if let Some(slots) = admitted_call_slots(text, node) {
                        facts.conditional_registration |=
                            description_is_plain_literal(text, slots[0])
                                && has_brace_body(slots[1]);
                    }
                }
            }
            _ => {}
        }

        // The role each child plays: the legacy depth it sits at and whether
        // `=` is an assignment there. Children are pushed in reverse so the
        // stack pops them in source order.
        let mut children: Vec<Placed<'_>> = Vec::new();
        let mut previous_statement: Option<usize> = None;
        for index in 0..node.child_count() {
            let Some(child) = node.child(index) else {
                continue;
            };
            let field = node.field_name_for_child(index);
            // Names and operands the grammar reads as plain identifiers but the
            // lexer reserves (`x$in`, `f(in = 1)`, `function(if)`) are refused
            // wherever they sit, including in name positions with no role.
            if child.kind() == "identifier" && is_reserved_word(node_text(text, child)) {
                return refuse();
            }
            // R needs a newline or `;` between two statements of a block too.
            if kind == "braced_expression" && field == Some("body") && child.kind() != "comment" {
                if previous_statement.is_some_and(|end| !separated(text, end, child.start_byte())) {
                    return refuse();
                }
                previous_statement = Some(child.end_byte());
            }
            let role = match (kind, field) {
                ("binary_operator", Some("lhs")) => Some((depth, allow_eq)),
                ("binary_operator" | "unary_operator", Some("rhs")) => Some((depth + 1, allow_eq)),
                // `x$name` and `pkg::name` name a component and read no value;
                // the `lhs` of `$`/`@` is the chain's own expression.
                ("extract_operator", Some("lhs")) => Some((depth, allow_eq)),
                ("call" | "subset" | "subset2", Some("function")) => Some((depth, allow_eq)),
                ("call" | "subset" | "subset2", Some("arguments")) => Some((depth + 1, false)),
                ("arguments", _) if child.kind() == "argument" => Some((depth, false)),
                ("argument", Some("value")) => Some((depth + 1, false)),
                ("parenthesized_expression", Some("body")) => Some((depth + 1, true)),
                ("braced_expression", Some("body")) => Some((depth + 2, true)),
                ("function_definition", Some("parameters")) => Some((depth, false)),
                ("function_definition", Some("body")) => Some((depth + 1, true)),
                ("parameters", _) if child.kind() == "parameter" => Some((depth, false)),
                ("parameter", Some("default")) => Some((depth + 1, false)),
                ("if_statement", Some("condition")) => Some((depth + 1, false)),
                ("if_statement", Some("consequence" | "alternative")) => Some((depth + 1, true)),
                ("for_statement", Some("sequence")) => Some((depth + 1, false)),
                ("while_statement", Some("condition")) => Some((depth + 1, false)),
                ("for_statement" | "while_statement" | "repeat_statement", Some("body")) => {
                    Some((depth + 1, true))
                }
                _ => None,
            };
            match role {
                Some((child_depth, child_eq)) => children.push((child, child_depth, child_eq)),
                // An error or missing token anywhere under a node refuses it,
                // even where the child carries no role.
                None if child.is_error() || child.is_missing() => {
                    children.push((child, depth, allow_eq));
                }
                None => {}
            }
        }
        stack.extend(children.into_iter().rev());
    }
    Ok(facts)
}

/// Words the lexer reserves, so none of them is a symbol in the declared subset.
fn is_reserved_word(word: &str) -> bool {
    matches!(
        word,
        "if" | "else" | "for" | "in" | "while" | "repeat" | "function" | "break" | "next"
    )
}

fn is_comparison(operator: &str) -> bool {
    matches!(operator, "<" | "<=" | ">" | ">=" | "==" | "!=")
}

fn node_text<'a>(text: &'a str, node: TsNode<'_>) -> &'a str {
    text.get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}

/// R's `sublist` slots of a call: comma-separated, any slot may be empty, and
/// `f()` has none. Each slot is the `argument` node, or `None` when empty.
fn call_slots(call: TsNode<'_>) -> Vec<Option<TsNode<'_>>> {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    let mut current = None;
    let mut separated = false;
    let mut cursor = arguments.walk();
    for child in arguments.children(&mut cursor) {
        match child.kind() {
            "comma" => {
                slots.push(current.take());
                separated = true;
            }
            "argument" => current = Some(child),
            _ => {}
        }
    }
    if current.is_some() || separated {
        slots.push(current);
    }
    slots
}

/// The slots of a call that is the bare-identifier, exactly-two-positional
/// `test_that` shape; `None` for every other node.
///
/// A bare identifier callee only: not `testthat::test_that`, not
/// `obj$test_that`, and not the backtick spelling `` `test_that` ``.
fn admitted_call_slots<'t>(text: &str, node: TsNode<'t>) -> Option<[Option<TsNode<'t>>; 2]> {
    let function = node.child_by_field_name("function")?;
    if function.kind() != "identifier" || node_text(text, function) != CALLEE {
        return None;
    }
    let slots = call_slots(node);
    let [first, second] = slots.as_slice() else {
        return None;
    };
    if [first, second]
        .iter()
        .any(|slot| slot.is_some_and(|argument| argument.child_by_field_name("name").is_some()))
    {
        return None;
    }
    Some([*first, *second])
}

fn has_brace_body(slot: Option<TsNode<'_>>) -> bool {
    slot.and_then(|slot| slot.child_by_field_name("value"))
        .is_some_and(|code| code.kind() == "braced_expression")
}

/// The description is the one part of the shape that must be read in the
/// source itself: a non-empty plain string literal, so neither a computed nor
/// a variable nor an R 4.0 raw-string description is source-visible.
fn description_is_plain_literal(text: &str, slot: Option<TsNode<'_>>) -> bool {
    let Some(description) = slot
        .and_then(|slot| slot.child_by_field_name("value"))
        .filter(|description| description.kind() == "string")
    else {
        return false;
    };
    let (Some(open), Some(close)) = (
        description.child_by_field_name("open"),
        description.child_by_field_name("close"),
    ) else {
        return false;
    };
    let raw = matches!(node_text(text, open).as_bytes().first(), Some(b'r' | b'R'));
    !raw && close.start_byte() > open.end_byte()
}

/// True when the node spells the name `test_that`, as a symbol, as the string
/// literal R also accepts on the left of an assignment, or as the backtick
/// spelling of the same symbol.
fn names_callee(text: &str, node: TsNode<'_>) -> bool {
    match node.kind() {
        "identifier" => {
            let name = node_text(text, node);
            name.strip_prefix('`')
                .and_then(|quoted| quoted.strip_suffix('`'))
                .unwrap_or(name)
                == CALLEE
        }
        "string" => match (
            node.child_by_field_name("open"),
            node.child_by_field_name("close"),
        ) {
            (Some(open), Some(close)) => {
                text.get(open.end_byte()..close.start_byte()) == Some(CALLEE)
            }
            _ => false,
        },
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Legacy hand-written parser (ADR-0058 differential oracle)
// ---------------------------------------------------------------------------

/// The pre-ADR-0058 recursive-descent parser, kept under `cfg(test)` as the
/// differential oracle until the coordinator's fixture-wide capture deletes it.
#[cfg(test)]
mod legacy {
    use super::{
        lex, Abstention, Program, TestThatBlock, Token, TokenKind, CALLEE, MAX_PARSE_DEPTH,
    };

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
                TokenKind::BacktickName => {
                    NodeKind::QuotedName(string_inner(self.text, token, false))
                }
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

    pub(super) fn parse_program(text: &str) -> Program {
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
        let mut description_not_literal = false;
        let mut dispatch_call = false;
        let mut external_symbol_call = false;
        for expression in &top_level {
            if admitted_anchor(expression) {
                anchors.push(TestThatBlock {
                    start: expression.start,
                    end: expression.end,
                });
            }
            if let Some(args) = admitted_call_args(expression) {
                let brace_body = args[1]
                    .value
                    .as_ref()
                    .is_some_and(|code| matches!(code.kind, NodeKind::Brace(_)));
                description_not_literal |= brace_body && !description_is_plain_literal(&args[0]);
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
                dispatch_call |= is_dispatch_call(node);
                external_symbol_call |= is_external_symbol_node(node);
            });
        }

        Program {
            anchors,
            rebinds_callee,
            conditional_registration,
            description_not_literal,
            dispatch_call,
            external_symbol_call,
            abstention: parse_abstention.or(lex_abstention),
        }
    }

    /// The ADR-0042 D2 shape, asked of the tree.
    ///
    /// A partially parsed or recovered node can never reach here: the parser
    /// returns whole expressions or nothing.
    fn admitted_anchor(node: &Node<'_>) -> bool {
        let Some(args) = admitted_call_args(node) else {
            return false;
        };
        description_is_plain_literal(&args[0])
            && args[1]
                .value
                .as_ref()
                .is_some_and(|code| matches!(code.kind, NodeKind::Brace(_)))
    }

    /// The arguments of a call that is the bare-identifier, exactly-two-positional
    /// `test_that` shape; `None` for every other node.
    fn admitted_call_args<'a>(node: &'a Node<'a>) -> Option<&'a [Arg<'a>]> {
        let NodeKind::Call { callee, args } = &node.kind else {
            return None;
        };
        // A bare identifier callee only: not `testthat::test_that`, not
        // `obj$test_that`, and not the backtick spelling `` `test_that` ``.
        if !matches!(callee.kind, NodeKind::Identifier(name) if name == CALLEE) {
            return None;
        }
        if args.len() != 2 || args.iter().any(|arg| arg.named) {
            return None;
        }
        Some(args)
    }

    /// The description is the one part of the shape that must be read in the
    /// source itself: a non-empty plain string literal, so neither a computed nor
    /// a variable nor an R 4.0 raw-string description is source-visible.
    fn description_is_plain_literal(arg: &Arg<'_>) -> bool {
        arg.value.as_ref().is_some_and(|description| {
            matches!(
                description.kind,
                NodeKind::Str {
                    raw: false,
                    inner
                } if !inner.is_empty()
            )
        })
    }

    /// True when this node registers or dispatches an S3/S4 generic.
    ///
    /// `UseMethod`/`NextMethod` are the S3 entry points, `setClass`/`setMethod`/
    /// `setGeneric`/`setRefClass` define S4 types and methods, and `callGeneric`/
    /// `callNextMethod` dispatch them. Which method runs is runtime type state.
    fn is_dispatch_call(node: &Node<'_>) -> bool {
        matches!(&node.kind, NodeKind::Call { callee, .. }
        if matches!(
            callee.kind,
            NodeKind::Identifier(
                "UseMethod"
                    | "NextMethod"
                    | "callGeneric"
                    | "callNextMethod"
                    | "setMethod"
                    | "setGeneric"
                    | "setClass"
                    | "setRefClass"
            )
        ))
    }

    /// True when this node resolves a symbol the bounded parse cannot bind:
    /// `library`/`require` attach packages to the search path, the dotted
    /// entries call native code, and `::`/`:::` name a package export directly.
    fn is_external_symbol_node(node: &Node<'_>) -> bool {
        match &node.kind {
            NodeKind::Call { callee, .. } => matches!(
                callee.kind,
                NodeKind::Identifier(
                    "library" | "require" | ".Call" | ".C" | ".Fortran" | ".External"
                )
            ),
            NodeKind::Binary {
                op: TokenKind::NsGet,
                ..
            } => true,
            _ => false,
        }
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

fn obligation_fact(
    unit: &CodeUnit,
    entry: &RObligation,
    range: SourceRange,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(entry.reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), entry.note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={}", entry.affected_claim),
            format!("r_unknown_kind={}", entry.kind),
            // The provider-fallback policy rides on the fact itself so the
            // recorded registry policy is machine-visible per obligation, the
            // same way provider unknowns carry their recovery guidance.
            format!("provider_fallback={}", entry.fallback),
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

    fn unknown_kind_count(parsed: &SourceParseOutput, kind: &str) -> usize {
        unknown_kinds(parsed)
            .iter()
            .filter(|emitted| emitted == &kind)
            .count()
    }

    fn affected_claims(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("affected_claim=").map(String::from))
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

    // --- ADR-0020 gate 4: the source-semantic obligation registry ---

    #[test]
    fn a_non_literal_description_records_its_own_unknown() {
        for source in [
            // Computed description.
            "test_that(paste0(\"a\", \"b\"), { })",
            // Variable description.
            "test_that(description, { })",
            // Raw-string description: a literal, but not the plain
            // source-visible spelling the anchor claims.
            "test_that(r\"(raw description)\", { })",
            // Empty literal: a literal that distinguishes nothing.
            "test_that(\"\", { })",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
            assert!(
                unknown_kinds(&parsed).contains(&"description_not_source_literal".to_string()),
                "{source}: {:?}",
                unknown_kinds(&parsed)
            );
            assert!(
                affected_claims(&parsed).contains(&"r_testthat_description_literal".to_string()),
                "{source}: {:?}",
                affected_claims(&parsed)
            );
        }
        // A shape problem elsewhere in the call is an admission decision, not
        // the description obligation, so it must not fire this unknown.
        let not_brace = output("test_that(\"desc\", expect_true(TRUE))\n");
        assert!(!unknown_kinds(&not_brace).contains(&"description_not_source_literal".to_string()));
    }

    #[test]
    fn a_conditionally_reached_computed_description_stays_silent() {
        // The conditional bucket owns calls under `if`; a nested computed
        // description is not a top-level call and understates silently.
        let parsed = output("if (interactive()) test_that(paste0(\"a\"), { })\n");
        assert_eq!(anchors(&parsed), 0);
        assert!(!unknown_kinds(&parsed).contains(&"description_not_source_literal".to_string()));
    }

    #[test]
    fn s3_s4_dispatch_calls_record_their_own_unknown_without_blocking() {
        for call in [
            "setClass(\"shape\", slots = c(id = \"integer\"))",
            "setMethod(\"plot\", signature(x = \"shape\"), function(x) invisible(NULL))",
            "UseMethod(\"handler\")",
            "NextMethod(\"handler\")",
        ] {
            let parsed = output(&format!("test_that(\"dispatches\", {{\n  {call}\n}})\n"));
            assert_eq!(
                anchors(&parsed),
                1,
                "dispatch must not block the anchor: {call}"
            );
            assert!(
                unknown_kinds(&parsed).contains(&"s3_s4_dispatch_call".to_string()),
                "{call}: {:?}",
                unknown_kinds(&parsed)
            );
            assert!(affected_claims(&parsed).contains(&"r_dispatch_target".to_string()));
        }
        let plain = output("test_that(\"plain\", { })\n");
        assert!(!unknown_kinds(&plain).contains(&"s3_s4_dispatch_call".to_string()));
    }

    #[test]
    fn native_package_and_namespace_calls_record_their_own_unknown() {
        for call in [
            "library(utils)",
            "require(stats)",
            "result <- .Call(\"c_entry\", 1L)",
            "value <- .C(\"c_entry\", 1L)",
            "value <- .Fortran(\"f_entry\", 1L)",
            "value <- .External(\"ext_entry\", 1L)",
            "value <- utils::head(1:3)",
            "value <- base:::unlisted(1:3)",
        ] {
            let parsed = output(&format!("test_that(\"interop\", {{\n  {call}\n}})\n"));
            assert_eq!(
                anchors(&parsed),
                1,
                "external symbols must not block: {call}"
            );
            assert!(
                unknown_kinds(&parsed).contains(&"native_or_package_symbol_call".to_string()),
                "{call}: {:?}",
                unknown_kinds(&parsed)
            );
            assert!(affected_claims(&parsed).contains(&"r_external_symbol_resolution".to_string()));
        }
        let plain = output("test_that(\"plain\", { value <- head(1:3) })\n");
        assert!(!unknown_kinds(&plain).contains(&"native_or_package_symbol_call".to_string()));
    }

    #[test]
    fn standing_obligations_ride_on_every_admitted_anchor_and_only_there() {
        let parsed = output("test_that(\"a\", { })\ntest_that(\"b\", { })\n");
        assert_eq!(anchors(&parsed), 2);
        for entry in R_OBLIGATION_REGISTRY.iter().filter(|entry| entry.standing) {
            assert_eq!(
                unknown_kind_count(&parsed, entry.kind),
                2,
                "standing obligation {} must ride on every anchor",
                entry.kind
            );
        }
        // Each standing fact is scoped to the anchor unit it rides on, so the
        // unknown stays claim-scoped rather than file-global.
        let block_units = parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::RTestThatBlock)
            .map(|unit| unit.id.as_str().to_string())
            .collect::<Vec<_>>();
        let standing_kinds = R_OBLIGATION_REGISTRY
            .iter()
            .filter(|entry| entry.standing)
            .map(|entry| format!("r_unknown_kind={}", entry.kind))
            .collect::<Vec<_>>();
        for fact in &parsed.report.semantic_facts {
            if fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| standing_kinds.contains(assumption))
            {
                assert!(
                    block_units.contains(&fact.subject),
                    "standing unknown must be anchored to a block unit"
                );
            }
        }
        // With no anchors there is nothing for a standing obligation to ride
        // on, and no other unknown fires either.
        let empty = output("value <- 1\n");
        assert!(
            unknown_kinds(&empty).is_empty(),
            "{:?}",
            unknown_kinds(&empty)
        );
    }

    #[test]
    fn byte_and_block_bounds_are_exact_then_fail_at_plus_one() {
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let exact = format!("# {}\n", "x".repeat(limit - 3));
        assert!(exact.len() <= limit);
        let parsed = output(&exact);
        assert_eq!(anchors(&parsed), 0);
        assert!(!unknown_kinds(&parsed).contains(&"source_byte_limit".to_string()));
        let over = format!("# {}\n", "x".repeat(limit - 2));
        assert!(over.len() > limit);
        let parsed = output(&over);
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"source_byte_limit".to_string()));

        let exact_blocks = "test_that(\"fills the file\", { })\n".repeat(MAX_BLOCK_UNITS);
        let parsed = output(&exact_blocks);
        assert_eq!(anchors(&parsed), MAX_BLOCK_UNITS);
        assert!(!unknown_kinds(&parsed).contains(&"parser_resource_limit".to_string()));
        let over_blocks = "test_that(\"fills the file\", { })\n".repeat(MAX_BLOCK_UNITS + 1);
        let parsed = output(&over_blocks);
        assert_eq!(anchors(&parsed), MAX_BLOCK_UNITS);
        assert!(unknown_kinds(&parsed).contains(&"parser_resource_limit".to_string()));
    }

    #[test]
    fn every_registry_entry_fires_on_its_trigger_and_every_emission_is_registered() {
        // One corpus per registry entry: each trigger below must make its
        // entry fire, so the registry cannot list an obligation nothing
        // records. Standing entries fire on any admitted anchor.
        let depth_source = format!(
            "test_that(\"before\", {{ }})\nx <- {}1{}\n",
            "(".repeat(4_096),
            ")".repeat(4_096)
        );
        let declared_triggers: &[(&str, &str)] = &[
            (
                "test_that_rebound_in_file",
                "test_that <- identity\ntest_that(\"x\", { })\n",
            ),
            (
                "conditional_test_registration",
                "if (interactive()) test_that(\"x\", { })\n",
            ),
            (
                "description_not_source_literal",
                "test_that(paste0(\"a\"), { })\n",
            ),
            (
                "unadmitted_r_construct",
                "test_that(\"before\", { })\nx => f()\n",
            ),
            ("parser_depth_limit", depth_source.as_str()),
            ("callee_binding_unproven", "test_that(\"x\", { })\n"),
            ("block_nse_not_evaluated", "test_that(\"x\", { })\n"),
            (
                "s3_s4_dispatch_call",
                "test_that(\"x\", { UseMethod(\"h\") })\n",
            ),
            (
                "native_or_package_symbol_call",
                "test_that(\"x\", { library(utils) })\n",
            ),
        ];
        for (kind, source) in declared_triggers {
            let declared = output(source);
            assert!(
                unknown_kinds(&declared).contains(&kind.to_string()),
                "{kind} must fire on its trigger: {:?}",
                unknown_kinds(&declared)
            );
        }
        // The undeclared identity trigger needs a context without the
        // DESCRIPTION declaration.
        let undeclared = output_with(&ParserProjectContext::default(), "test_that(\"x\", { })\n");
        assert!(
            unknown_kinds(&undeclared).contains(&"testthat_not_declared".to_string()),
            "{:?}",
            unknown_kinds(&undeclared)
        );
        // The byte and block bounds get their own triggers because their
        // inputs are too large to inline above.
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let over_bytes = format!("# {}\n", "x".repeat(limit - 2));
        assert!(unknown_kinds(&output(&over_bytes)).contains(&"source_byte_limit".to_string()));
        let over_blocks = "test_that(\"fills the file\", { })\n".repeat(MAX_BLOCK_UNITS + 1);
        assert!(unknown_kinds(&output(&over_blocks)).contains(&"parser_resource_limit".to_string()));

        // And the reverse direction: nothing the frontend emits is outside
        // the registry, and the registry's (claim, kind) pairs are unique.
        let mut pairs = R_OBLIGATION_REGISTRY
            .iter()
            .map(|entry| (entry.affected_claim, entry.kind))
            .collect::<Vec<_>>();
        pairs.sort_unstable();
        let count = pairs.len();
        pairs.dedup();
        assert_eq!(
            pairs.len(),
            count,
            "registry (claim, kind) pairs must be unique"
        );
        for (kind, source) in declared_triggers {
            for emitted in unknown_kinds(&output(source)) {
                assert!(
                    R_OBLIGATION_REGISTRY
                        .iter()
                        .any(|entry| entry.kind == emitted),
                    "emitted kind {emitted} from {kind} trigger is not registered"
                );
            }
        }
    }

    #[test]
    fn obligation_output_is_deterministic_and_counts_are_stable() {
        let source = "test_that(\"a\", { library(utils) })\n\
                      test_that(\"b\", { })\n\
                      test_that(paste0(\"c\"), { })\n\
                      if (interactive()) test_that(\"d\", { })\n";
        let first = output(source);
        let second = output(source);
        assert_eq!(
            format!("{:?}", first.report.semantic_facts),
            format!("{:?}", second.report.semantic_facts),
            "obligation facts must be deterministic"
        );
        assert_eq!(
            format!("{:?}", first.report.units),
            format!("{:?}", second.report.units)
        );
        // Exact, stable counts: two anchors, two standing obligations each,
        // one triggered external-symbol unknown, one description unknown, and
        // one conditional-registration unknown. Nothing else fires.
        assert_eq!(anchors(&first), 2);
        assert_eq!(unknown_kind_count(&first, "callee_binding_unproven"), 2);
        assert_eq!(unknown_kind_count(&first, "block_nse_not_evaluated"), 2);
        assert_eq!(
            unknown_kind_count(&first, "native_or_package_symbol_call"),
            1
        );
        assert_eq!(
            unknown_kind_count(&first, "description_not_source_literal"),
            1
        );
        assert_eq!(
            unknown_kind_count(&first, "conditional_test_registration"),
            1
        );
        assert_eq!(
            unknown_kinds(&first).len(),
            7,
            "{:?}",
            unknown_kinds(&first)
        );
    }

    #[test]
    fn registry_strings_satisfy_the_stored_assumption_content_rules() {
        // Assumptions are persisted verbatim and the storage layer rejects
        // values that look like source snippets (an `=` together with a `;`,
        // braces, `=>`), absolute paths, or URL schemes. Every registry entry
        // must keep its emitted assumption strings inside those rules.
        for entry in R_OBLIGATION_REGISTRY {
            for value in [
                format!("affected_claim={}", entry.affected_claim),
                format!("r_unknown_kind={}", entry.kind),
                format!("provider_fallback={}", entry.fallback),
                entry.note.to_string(),
            ] {
                assert!(!value.contains("=>"), "{value}");
                assert!(!(value.contains('=') && value.contains(';')), "{value}");
                assert!(!value.contains('{') && !value.contains('}'), "{value}");
                assert!(!value.contains("://"), "{value}");
                assert!(!value.contains('\0'), "{value}");
            }
        }
    }

    #[test]
    fn registry_blocking_records_match_the_authoritative_classifier() {
        // `application/family.rs` is the authoritative claim-impact classifier
        // and this crate's adapters must not depend on it, so this test pins
        // the recorded split by re-deriving the two rules it must agree with:
        // an unmet identity obligation blocks the family claim, and every
        // other recorded obligation is non-blocking. The rules live verbatim
        // in `r_unknown_reason_blocks_family_membership` and
        // `r_unknown_is_non_blocking_family_subclaim`.
        for entry in R_OBLIGATION_REGISTRY {
            if entry.blocks_family_claim {
                assert_eq!(
                    entry.affected_claim, "r_testthat_identity",
                    "only the identity claim may block the R family"
                );
                assert!(
                    matches!(
                        entry.reason,
                        UnknownReasonCode::MissingDependency | UnknownReasonCode::MonkeyPatch
                    ),
                    "{} blocks with a reason the classifier does not block on",
                    entry.kind
                );
                assert!(!entry.standing);
            } else {
                assert_ne!(
                    entry.affected_claim, "r_testthat_identity",
                    "an identity obligation that does not block contradicts ADR-0042 D3/D2b"
                );
            }
        }
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
        // The plain admitted case fires no triggered obligation except the
        // one its own `library(testthat)` line earns; the only other
        // unknowns are the two standing residuals riding on every anchor.
        assert_eq!(unknown_kind_count(&parsed, "callee_binding_unproven"), 3);
        assert_eq!(unknown_kind_count(&parsed, "block_nse_not_evaluated"), 3);
        assert_eq!(
            unknown_kind_count(&parsed, "native_or_package_symbol_call"),
            1
        );
        assert_eq!(
            unknown_kinds(&parsed).len(),
            7,
            "{:?}",
            unknown_kinds(&parsed)
        );
    }

    #[test]
    fn an_escaped_quote_inside_a_description_stays_one_literal() {
        let parsed = output("test_that(\"has a \\\" quote\", { })\n");
        assert_eq!(anchors(&parsed), 1);
    }

    // -----------------------------------------------------------------------
    // ADR-0058 differential: legacy parser (oracle) versus Tree-sitter.
    // -----------------------------------------------------------------------

    type Both = (
        Result<SourceParseOutput, ParseError>,
        Result<SourceParseOutput, ParseError>,
    );

    fn parse_both(context: &ParserProjectContext, text: &str) -> Both {
        let document = |text| SourceDocument {
            path: "tests/testthat/test-catalog.R",
            language: Language::R,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        };
        (
            build_output(document(text), context, |t| Ok(legacy::parse_program(t))),
            parse_output(document(text), context),
        )
    }

    fn assert_equivalent(text: &str) {
        let (old, new) = parse_both(&declared(), text);
        assert_eq!(
            old.expect("legacy"),
            new.expect("tree-sitter"),
            "legacy and Tree-sitter diverge on:\n{text}"
        );
    }

    #[test]
    fn differential_matches_every_committed_fixture_source() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/r");
        let mut compared = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read fixture dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.to_string_lossy().to_string();
                if let Some(at) = name.find("tests/testthat/") {
                    if is_testthat_path(&name[at..]) {
                        assert_equivalent(&std::fs::read_to_string(&path).expect("fixture"));
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared >= 3, "fixture sources compared: {compared}");
    }

    fn branch_corpus() -> Vec<String> {
        let mut corpus: Vec<String> = [
            // Admitted anchors and variations.
            "",
            "\n\n",
            "# only a comment\n",
            "test_that(\"a\", { expect_true(TRUE) })\n",
            "test_that('single', {\n  expect_equal(1, 1)\n})\n",
            "test_that(\"a\", {})\ntest_that(\"b\", {})\n",
            "test_that(\"a\", {}); test_that(\"b\", {})\n",
            "test_that(\"a\", {})",
            "test_that(\"a\",\n  {\n  1\n  }\n)\n",
            "test_that(\n  \"a\",\n  {\n  1\n  }\n)\n",
            "test_that(\"a\", { 1 }) # trailing\n",
            "test_that(\"a\", { 1 })\n# c\n\ntest_that(\"b\", { 2 })\n",
            "test_that(\"with { brace\", { x <- \"}\" })\n",
            "test_that(\"has a \\\" quote\", { })\n",
            "test_that(\"esc \\n \\t \\\\ \\u00e9 \\x41 \\101\", { })\n",
            "test_that('it\\'s', { })\n",
            "test_that(\"multi\nline\", { })\n",
            "test_that(\"unicode é 日本\", { })\n",
            "test_that(\"a\", { # c\n  1 # d\n})\n",
            "test_that(\"a\", { r\"(raw)\" ; 'x' })\n",
            "test_that(\"a\", { function(x, y = 2, ...) x + y })\n",
            "test_that(\"a\", { \\(x) x })\n",
            "test_that(\"a\", { if (TRUE) 1 else 2 })\n",
            "test_that(\"a\", {\n if (TRUE) 1\n else 2\n})\n",
            "test_that(\"a\", { for (i in 1:3) print(i) })\n",
            "test_that(\"a\", { while (FALSE) next })\n",
            "test_that(\"a\", { repeat break })\n",
            "test_that(\"a\", { x[1]; x[[1]]; x[[y[1]]]; x[, 1]; m[1, ] })\n",
            "test_that(\"a\", { x$y; x@y; x$`y`; x$'y'; a::b; a:::b })\n",
            "test_that(\"a\", { 1L; 1i; 0x10; 0x1.8p3; 1e5; 1.5e-3; .5; 5. })\n",
            "test_that(\"a\", { -1; +1; !x; ~y; y ~ x })\n",
            "test_that(\"a\", { a %in% b; a %>% b; a |> f() })\n",
            "test_that(\"a\", { x <- 1; y <<- 2; 3 -> z; 4 ->> w; v = 5 })\n",
            "test_that(\"a\", { a && b || c; a & b | c })\n",
            "test_that(\"a\", { a < b; a <= b; a == b & c != d })\n",
            "test_that(\"a\", { a < (b < c) })\n",
            "test_that(\"a\", { f(a = 1, b = , c) })\n",
            "test_that(\"a\", { f(,) ; f(a,) ; f(,a) ; f() })\n",
            "test_that(\"a\", { f(\"name\" = 1, `n` = 2, 'q' = 3, r\"(raw)\" = 4) })\n",
            "test_that(\"a\", { f(NULL = 1) })\n",
            "test_that(\"a\", { f(function(x) x, (function(y) y)(1)) })\n",
            "test_that(\"a\", { f(\n a,\n b\n) })\n",
            "test_that(\"a\", { x <-\n 1 })\n",
            "test_that(\"a\", { x +\n 1 })\n",
            "test_that(\"a\", {\n x\n + 1\n})\n",
            "test_that(\"a\", { a^b^c; -a^b; a:b:c; 2^-1 })\n",
            "test_that(\"a\", { a$b$c; a$b(1)$c; a[[1]]$b })\n",
            "test_that(\"a\", { ...; ..1; .x; x.y; x_y })\n",
            "test_that(\"a\", { TRUE; FALSE; NULL; NA; NA_integer_; Inf; NaN })\n",
            // Near misses that must not anchor.
            "test_that(\"a\", 1)\n",
            "test_that(\"a\")\n",
            "test_that()\n",
            "test_that(\"a\", {}, extra)\n",
            "test_that(\"\", {})\n",
            "test_that('', {})\n",
            "test_that(desc, {})\n",
            "test_that(paste(\"a\", \"b\"), {})\n",
            "test_that(r\"(raw)\", {})\n",
            "test_that(R\"-(raw)-\", {})\n",
            "test_that(desc = \"a\", { })\n",
            "test_that(\"a\", code = { })\n",
            "test_that(\"a\", { }, )\n",
            "test_that(, { })\n",
            "test_that(\"a\", )\n",
            "testthat::test_that(\"a\", { })\n",
            "testthat:::test_that(\"a\", { })\n",
            "obj$test_that(\"a\", { })\n",
            "obj@test_that(\"a\", { })\n",
            "`test_that`(\"a\", { })\n",
            "\"test_that\"(\"a\", { })\n",
            "(test_that)(\"a\", { })\n",
            "(test_that(\"a\", { }))\n",
            "f(test_that(\"a\", { }))\n",
            "x <- test_that(\"a\", { })\n",
            "x[test_that(\"a\", { })]\n",
            "test_that(\"a\", { })(1)\n",
            "test_that(\"a\", { })$x\n",
            "function() test_that(\"a\", { })\n",
            "{ test_that(\"a\", { }) }\n",
            "if (interactive()) test_that(\"a\", { })\n",
            "if (a) {\n  test_that(\"a\", { })\n} else {\n  test_that(\"b\", { })\n}\n",
            "for (i in 1:3) test_that(\"a\", { })\n",
            "while (TRUE) test_that(\"a\", { })\n",
            "repeat test_that(\"a\", { })\n",
            "TEST_THAT(\"a\", { })\n",
            "test_that_x(\"a\", { })\n",
            "mytest_that(\"a\", { })\n",
            "test_that\n(\"a\", { })\n",
            "# test_that(\"a\", { })\n",
            "x <- \"test_that(\\\"a\\\", { })\"\n",
            "x <- r\"(test_that(\"a\", { }))\"\n",
            "x <- 'test_that(\"a\", { })'\n",
            // Rebinding forms.
            "test_that <- function(a, b) NULL\ntest_that(\"a\", { })\n",
            "test_that = 1\ntest_that(\"a\", { })\n",
            "test_that <<- 1\ntest_that(\"a\", { })\n",
            "1 -> test_that\ntest_that(\"a\", { })\n",
            "1 ->> test_that\ntest_that(\"a\", { })\n",
            "`test_that` <- 1\ntest_that(\"a\", { })\n",
            "\"test_that\" <- 1\ntest_that(\"a\", { })\n",
            "r\"(test_that)\" <- 1\ntest_that(\"a\", { })\n",
            "'test_that' = 1\ntest_that(\"a\", { })\n",
            "assign(\"test_that\", 1)\ntest_that(\"a\", { })\n",
            "assign('test_that', 1)\ntest_that(\"a\", { })\n",
            "assign(x = \"test_that\", 1)\ntest_that(\"a\", { })\n",
            "assign(,\"test_that\")\ntest_that(\"a\", { })\n",
            "assign(\"other\", 1)\ntest_that(\"a\", { })\n",
            "for (test_that in 1:2) 1\ntest_that(\"a\", { })\n",
            "for (i in 1:2) test_that <- i\ntest_that(\"a\", { })\n",
            "f <- function() { test_that <- 1 }\ntest_that(\"a\", { })\n",
            "test_that$x <- 1\ntest_that(\"a\", { })\n",
            "x$test_that <- 1\ntest_that(\"a\", { })\n",
            "(test_that) <- 1\ntest_that(\"a\", { })\n",
            "x <- test_that\ntest_that(\"a\", { })\n",
            "test_that <- test_that <- 1\n",
            "a <- b <- test_that\n",
            "test_that == 1\ntest_that(\"a\", { })\n",
            "f(test_that = 1)\ntest_that(\"a\", { })\n",
            // Conditional registration.
            "if (a) test_that(\"a\", { }) else test_that(\"b\", { })\n",
            "for (i in 1:2) { test_that(\"a\", { }) }\n",
            "while (a) test_that(\"a\", { })\n",
            "repeat { test_that(\"a\", { }); break }\n",
            "if (a) { f(test_that(\"a\", { })) }\n",
            "if (a) 1\ntest_that(\"b\", { })\n",
            "if (a) test_that(desc, { })\n",
            "if (a) function() test_that(\"a\", { })\n",
            // Computed / non-literal descriptions.
            "test_that(paste0(\"a\", i), { })\n",
            "test_that(r\"(raw)\", { })\ntest_that(\"b\", { })\n",
            "test_that(x, { })\n",
            "test_that(x, y)\n",
            "test_that(x, { }, z)\n",
            "test_that(1, { })\n",
            "test_that(\"a\" , { })\ntest_that(x, { })\n",
            // Dispatch and external symbols.
            "UseMethod(\"print\")\n",
            "print.foo <- function(x) NextMethod()\n",
            "setClass(\"A\", representation())\n",
            "setMethod(\"show\", \"A\", function(object) 1)\n",
            "setGeneric(\"g\", function(x) standardGeneric(\"g\"))\n",
            "setRefClass(\"R\")\n",
            "callGeneric(1)\n",
            "callNextMethod()\n",
            "obj$setClass(1)\n",
            "library(testthat)\n",
            "require(stats)\n",
            ".Call(\"f\")\n.C(\"f\")\n.Fortran(\"f\")\n.External(\"f\")\n",
            "testthat::expect_true(TRUE)\n",
            "pkg:::hidden()\n",
            "x <- stats::sd(1:3)\n",
            "library(testthat)\ntest_that(\"a\", { pkg::f() })\n",
            "f <- function() { library(x) }\n",
            "if (a) library(x)\n",
            // Lexical-gate refusals.
            "test_that(\"a\", { })\n x <- \"unterminated\n",
            "test_that(\"a\", { })\nx <- 'unterminated\n",
            "test_that(\"a\", { })\nx <- `unterminated\n",
            "test_that(\"a\", { })\nx <- r\"(unterminated\n",
            "test_that(\"a\", { })\nx <- r\"[bad\"\n",
            "test_that(\"a\", { })\nx <- r\"x\"\n",
            "test_that(\"a\", { })\nx <- R'-(a)-'\n",
            "test_that(\"a\", { })\nx <- r\"-(a)\"\n",
            "test_that(\"a\", { })\nx <- 1e\n",
            "test_that(\"a\", { })\nx <- 1abc\n",
            "test_that(\"a\", { })\nx <- 0x\n",
            "test_that(\"a\", { })\nx <- 1e+\n",
            "test_that(\"a\", { })\nx <- 0x1p\n",
            "test_that(\"a\", { })\nx <- 1\u{e9}\n",
            "test_that(\"a\", { })\nx <- \u{e9}\n",
            "test_that(\"a\", { })\n?help\n",
            "test_that(\"a\", { })\nx ? y\n",
            "test_that(\"a\", { })\nx => y\n",
            "test_that(\"a\", { })\n_ <- 1\n",
            "test_that(\"a\", { })\nx <- _y\n",
            "test_that(\"a\", { })\nx <- a %b\nc%\n",
            "test_that(\"a\", { })\nx <- a %\n",
            "test_that(\"a\", { }) ?\n",
            "test_that(\"a\", { }) \u{e9}\n",
            "?\n",
            "x <- \"ok\" ?\n",
            // Parse refusals.
            "test_that(\"a\", { 1 }\n",
            "test_that(\"a\", { 1 }))\n",
            "test_that(\"a\", { 1 })\n)\n",
            "test_that(\"a\", { 1 }) )\n",
            "test_that(\"a\", { 1 }) }\n",
            "test_that(\"a\", { 1 }\ntest_that(\"b\", {})\n",
            "test_that(\"a\", { })\n}\n",
            "test_that(\"a\", { })\n]\n",
            "test_that(\"a\", { })\nx <-\n",
            "test_that(\"a\", { })\nx <- (1\n",
            "test_that(\"a\", { })\nf(1\n",
            "test_that(\"a\", { })\nf(1 2)\n",
            "test_that(\"a\", { })\nx y\n",
            "test_that(\"a\", { })\nx <- 1 y <- 2\n",
            "test_that(\"a\", { }) test_that(\"b\", { })\n",
            "test_that(\"a\", { }) x\n",
            "test_that(\"a\", { })x\n",
            "{ a b }\ntest_that(\"c\", { })\n",
            "test_that(\"a\", { 1 2 })\n",
            "test_that(\"a\", { a b; c })\n",
            "test_that(\"a\", { })\nif (x) a\nelse b\n",
            "test_that(\"a\", { })\nif (x = 1) a\n",
            "test_that(\"a\", { })\nwhile (x = 1) a\n",
            "test_that(\"a\", { })\nfor (i in x = 1) a\n",
            "test_that(\"a\", { })\nf(a = b = 1)\n",
            "test_that(\"a\", { })\nf(a$b = 1)\n",
            "test_that(\"a\", { })\nx[a = b = 1]\n",
            "test_that(\"a\", { })\nfunction(x = a = 1) x\n",
            "test_that(\"a\", { })\na < b < c\n",
            "test_that(\"a\", { })\na == b != c\n",
            "test_that(\"a\", { })\na < b + c < d\n",
            "test_that(\"a\", { })\n!a < b < c\n",
            "test_that(\"a\", { })\na & b < c < d\n",
            "test_that(\"a\", { })\na < b & c < d\n",
            "test_that(\"a\", { })\na := b\n",
            "test_that(\"a\", { })\na ** b\n",
            "test_that(\"a\", { })\nx$\n",
            "test_that(\"a\", { })\nx@\n",
            "test_that(\"a\", { })\nx$ \n y\n",
            "test_that(\"a\", { })\nx$(1)\n",
            "test_that(\"a\", { })\nx$1\n",
            "test_that(\"a\", { })\na::\n",
            "test_that(\"a\", { })\na:::\n",
            "test_that(\"a\", { })\n::a\n",
            "test_that(\"a\", { })\nfunction(`x`) x\n",
            "test_that(\"a\", { })\nfunction(x y) x\n",
            "test_that(\"a\", { })\nfunction(x = ) x\n",
            "test_that(\"a\", { })\nfunction(1) x\n",
            "test_that(\"a\", { })\nfunction(\"x\") x\n",
            "test_that(\"a\", { })\nfunction x\n",
            "test_that(\"a\", { })\nfunction()\n",
            "test_that(\"a\", { })\nfor (`i` in x) 1\n",
            "test_that(\"a\", { })\nfor (i x) 1\n",
            "test_that(\"a\", { })\nfor (i in) 1\n",
            "test_that(\"a\", { })\nfor i in x 1\n",
            "test_that(\"a\", { })\nif x 1\n",
            "test_that(\"a\", { })\nif (x)\n",
            "test_that(\"a\", { })\nwhile (x)\n",
            "test_that(\"a\", { })\nrepeat\n",
            "test_that(\"a\", { })\nelse 1\n",
            "test_that(\"a\", { })\nin\n",
            "test_that(\"a\", { })\nbreak\nnext\n",
            "test_that(\"a\", { })\nx <- break\n",
            "test_that(\"a\", { })\n,\n",
            "test_that(\"a\", { })\n;\n;;\n",
            "test_that(\"a\", { })\n= 1\n",
            "test_that(\"a\", { })\n<- 1\n",
            "test_that(\"a\", { })\n* 1\n",
            "test_that(\"a\", { })\n-\n",
            "test_that(\"a\", { })\n!\n",
            "test_that(\"a\", { })\n~\n",
            "test_that(\"a\", { })\nx[[1]\n",
            "test_that(\"a\", { })\nx[1]]\n",
            "test_that(\"a\", { })\nx[[1]]]\n",
            "test_that(\"a\", { })\nf(a = , b = )\n",
            "test_that(\"a\", { })\nf(a b = 1)\n",
            "test_that(\"a\", { })\nf(,,)\n",
            "test_that(\"a\", { })\nf(\n)\n",
            "test_that(\"a\", { })\n\\x x\n",
            "test_that(\"a\", { })\n\\(x) x\n",
            "test_that(\"a\", { })\n(1 2)\n",
            "test_that(\"a\", { })\n()\n",
            "test_that(\"a\", { })\n{}\n",
            "test_that(\"a\", { })\n{ ; }\n",
            "test_that(\"a\", { })\n1 -> \n",
            "test_that(\"a\", { })\na |> f\n",
            "test_that(\"a\", { })\na %o% b %o% c\n",
            "test_that(\"a\", { })\n1.5.2\n",
            "test_that(\"a\", { })\n1..2\n",
            "test_that(\"a\", { })\n.1.2\n",
            "test_that(\"a\", { })\nx..1\n",
            "test_that(\"a\", { })\n0x1.8\n",
            "test_that(\"a\", { })\n0X1P-2L\n",
            "test_that(\"a\", { })\n1e5L\n",
            "test_that(\"a\", { })\n1.5i\n",
            "test_that(\"a\", { })\n100000000000L\n",
            "test_that(\"a\", { })\n1 L\n",
            "test_that(\"a\", { })\n1 i\n",
            "test_that(\"a\", { })\n\"a\" \"b\"\n",
            "test_that(\"a\", { })\n\"a\"\n\"b\"\n",
            "test_that(\"a\", { })\n`a` `b`\n",
            "test_that(\"a\", { })\r\ntest_that(\"b\", { })\r\n",
            "test_that(\"a\", { })\x0b\x0ctest_that(\"b\", { })\n",
        ]
        .iter()
        .map(|source| source.to_string())
        .collect();
        // Depth bounds: both sides of 128 for every nesting construct.
        for n in [100usize, 126, 127, 128, 129, 130, 200] {
            corpus.push(format!("x <- {}1{}\n", "(".repeat(n), ")".repeat(n)));
            corpus.push(format!("{}1\n", "(".repeat(n)));
            corpus.push(format!("{}x\n", "f(".repeat(n)));
            corpus.push(format!("{}x\n", "x[".repeat(n)));
            corpus.push(format!("{}x\n", "x[[".repeat(n)));
            corpus.push(format!("{}x\n", "{".repeat(n)));
            corpus.push(format!("{}\n", "{".repeat(n)));
            corpus.push(format!("{}\n", "f(".repeat(n)));
            corpus.push(format!("{}x){}\n", "f(".repeat(n), ")".repeat(3)));
            corpus.push(format!("{}x{} )\n", "f(".repeat(n), ")".repeat(n)));
            corpus.push(format!("{}x{} }}\n", "(".repeat(n), ")".repeat(n)));
            corpus.push(format!("{}x{}\n", "f(".repeat(n), ")".repeat(n)));
            corpus.push(format!("{}x{}\n", "x[".repeat(n), "]".repeat(n)));
            corpus.push(format!("{}x{}\n", "{".repeat(n), "}".repeat(n)));
            corpus.push(format!("{}x\n", "-".repeat(n)));
            corpus.push(format!("{}x\n", "!".repeat(n)));
            corpus.push(format!("{}x\n", "a <- ".repeat(n)));
            corpus.push(format!("{}x\n", "a^".repeat(n)));
            corpus.push(format!("{}x\n", "a + ".repeat(n)));
            corpus.push(format!("{}x\n", "a$".repeat(n)));
            corpus.push(format!("{}x\n", "function(a) ".repeat(n)));
            corpus.push(format!("{}x\n", "if (a) ".repeat(n)));
            corpus.push(format!("{}x\n", "repeat ".repeat(n)));
            corpus.push(format!("{}x\n", "while (a) ".repeat(n)));
            corpus.push(format!("{}x\n", "for (i in a) ".repeat(n)));
            corpus.push(format!("{}x\n", "function(a = ".repeat(n)));
            corpus.push(format!(
                "test_that(\"a\", {}1{})\n",
                "{".repeat(n),
                "}".repeat(n)
            ));
            corpus.push(format!(
                "test_that(\"a\", {{ }})\n{}1{}\ntest_that(\"b\", {{ }})\n",
                "(".repeat(n),
                ")".repeat(n)
            ));
            corpus.push(format!("test_that(\"a\", {{ }})\n{}1\n", "(".repeat(n)));
        }
        // A left-associative chain is flat in the legacy parser, so it never nears the limit.
        corpus.push(format!("x <- {}1\n", "1 + ".repeat(1_000)));
        corpus.push(format!("{}x\n", "a$".repeat(1_000)));
        corpus.push(format!("f{}\n", "(1)".repeat(1_000)));
        corpus.push("test_that(\"a\", { }) ; ".repeat(50));
        corpus
    }

    #[test]
    fn differential_matches_the_branch_corpus() {
        let corpus = branch_corpus();
        let mut diverging = Vec::new();
        for text in &corpus {
            for context in [declared(), ParserProjectContext::default()] {
                let (old, new) = parse_both(&context, text);
                let (old, new) = (old.expect("legacy"), new.expect("tree-sitter"));
                if old != new {
                    diverging.push(format!(
                        "{:?} len={} legacy={:?}/{} new={:?}/{}",
                        text.chars().take(24).collect::<String>(),
                        text.len(),
                        unknown_kinds(&old),
                        anchors(&old),
                        unknown_kinds(&new),
                        anchors(&new)
                    ));
                    break;
                }
            }
        }
        assert!(
            diverging.is_empty(),
            "diverging sources:\n{}",
            diverging.join("\n")
        );
        println!("branch sources compared: {}", corpus.len());
        assert!(
            corpus.len() > 300,
            "branch sources compared: {}",
            corpus.len()
        );
    }

    #[test]
    fn a_very_long_left_associative_chain_is_walked_without_recursion() {
        // The legacy recursive `visit` overflowed the stack on a chain this
        // long; the tree is walked with an explicit stack and the chain, being
        // flat in legacy terms, never nears the depth bound.
        let text = format!(
            "x <- 1{}\ntest_that(\"a\", {{ }})\n",
            " + 1".repeat(200_000)
        );
        let parsed = output(&text);
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn grammar_stricter_than_the_legacy_parser_is_pinned() {
        // Tree-sitter-r rejects these spellings that the legacy parser read:
        // R closes `[[` with two separate `]` tokens even across a space or a
        // newline, the grammar requires hex digits in `\x` escapes and rejects
        // `\9`, `NULL`/`TRUE` are not argument or component names, and `::`
        // takes no newline and a symbol or string on its left. Each is a
        // refusal of the file after the proven expression, never an anchor.
        for text in [
            "test_that(\"a\", { })\nx[[1] ]\n",
            "test_that(\"a\", { })\nx <- \"\\x\"\n",
            "test_that(\"a\", { })\nf(TRUE = 1)\n",
            "test_that(\"a\", { })\nx$NULL\n",
            "test_that(\"a\", { })\nx <- \"\\9\"\n",
            "test_that(\"a\", { })\nx <- a::\nb\n",
            "test_that(\"a\", { })\nx <- v$a::b\n",
            "test_that(\"a\", { })\nf(x[[1]\n])\n",
        ] {
            let (old, new) = parse_both(&declared(), text);
            let (old, new) = (old.expect("legacy"), new.expect("tree-sitter"));
            assert_eq!(
                unknown_kind_count(&old, "unadmitted_r_construct"),
                0,
                "{text}"
            );
            assert_eq!(
                unknown_kind_count(&new, "unadmitted_r_construct"),
                1,
                "{text}"
            );
            assert_eq!((anchors(&old), anchors(&new)), (1, 1), "{text}");
        }
    }

    #[test]
    #[ignore = "set REPOGRAMMAR_R_DIFFERENTIAL_CORPUS to a directory of R test files"]
    fn external_differential_reports_every_difference() {
        let Ok(root) = std::env::var("REPOGRAMMAR_R_DIFFERENTIAL_CORPUS") else {
            return;
        };
        let mut files = Vec::new();
        let mut stack: Vec<_> = root.split(':').map(std::path::PathBuf::from).collect();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "R" || ext == "r") {
                    files.push(path);
                }
            }
        }
        files.sort();
        let (mut compared, mut differing, mut legacy_panics) = (0usize, 0usize, 0usize);
        let (mut refused, mut with_anchors, mut anchor_total) = (0usize, 0usize, 0usize);
        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            compared += 1;
            let Ok((legacy, new)) = std::panic::catch_unwind(|| parse_both(&declared(), &text))
            else {
                legacy_panics += 1;
                println!("PANIC {}", path.display());
                continue;
            };
            let (Ok(old), Ok(new)) = (legacy, new) else {
                println!("ERROR {}", path.display());
                continue;
            };
            refused += usize::from(!old.report.diagnostics.is_empty());
            with_anchors += usize::from(anchors(&old) > 0);
            anchor_total += anchors(&old);
            if old != new {
                differing += 1;
                let kinds = |output: &SourceParseOutput| {
                    let mut kinds = unknown_kinds(output);
                    kinds.sort();
                    kinds.join(",")
                };
                println!(
                    "DIFF {} anchors {}->{} unknowns [{}]->[{}] diagnostics {}->{}",
                    path.display(),
                    anchors(&old),
                    anchors(&new),
                    kinds(&old),
                    kinds(&new),
                    old.report.diagnostics.len(),
                    new.report.diagnostics.len(),
                );
            }
        }
        println!(
            "SUMMARY compared={compared} differing={differing} panics={legacy_panics} refused={refused} files_with_anchors={with_anchors} anchors={anchor_total}"
        );
    }

    #[test]
    #[ignore = "set REPOGRAMMAR_R_DIFFERENTIAL_CORPUS to a directory of R test files"]
    fn external_mutation_differential_reports_every_difference() {
        let Ok(root) = std::env::var("REPOGRAMMAR_R_DIFFERENTIAL_CORPUS") else {
            return;
        };
        let mut files = Vec::new();
        let mut stack: Vec<_> = root.split(':').map(std::path::PathBuf::from).collect();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "R" || ext == "r") {
                    files.push(path);
                }
            }
        }
        files.sort();
        let inserts: &[&str] = &[
            "(", ")", "{", "}", "[", "]", "\"", "'", "#", ",", ";", "\n", "$", ":", "=", "<", "-",
            "+", " %", "if ", "else", "function", "`", "\\", "L", "1", "!", "&", "|", "?", "_",
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut compared, mut differing) = (0usize, 0usize);
        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            if !text.is_ascii() || text.len() > 6_000 || text.is_empty() {
                continue;
            }
            for round in 0..12 {
                let mut mutated = text.clone();
                let at = (next() as usize) % mutated.len();
                match next() % 3 {
                    0 => {
                        let end = (at + 1 + (next() as usize) % 3).min(mutated.len());
                        mutated.replace_range(at..end, "");
                    }
                    1 => mutated.insert_str(at, inserts[(next() as usize) % inserts.len()]),
                    _ => mutated.truncate(at),
                }
                compared += 1;
                let (old, new) = parse_both(&declared(), &mutated);
                let (Ok(old), Ok(new)) = (old, new) else {
                    continue;
                };
                if old != new {
                    differing += 1;
                    if let Ok(dir) = std::env::var("REPOGRAMMAR_R_DIFFERENTIAL_DUMP") {
                        let _ = std::fs::write(format!("{dir}/m{differing}.R"), &mutated);
                    }
                    let around = mutated.get(at.saturating_sub(40)..(at + 40).min(mutated.len()));
                    println!(
                        "MDIFF {} round {round} anchors {}->{} unknowns {:?}->{:?} near {:?}",
                        path.display(),
                        anchors(&old),
                        anchors(&new),
                        unknown_kinds(&old),
                        unknown_kinds(&new),
                        around
                    );
                }
            }
        }
        println!("MSUMMARY compared={compared} differing={differing}");
    }
}
