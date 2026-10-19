//! Bounded Fortran test-drive frontend for the ADR-0051 admitted shape.
//!
//! Only free-form Fortran source paths (`.f90`, `.f95`, `.f03`, `.f08` under
//! ADR-0034's lowercase rule) reach this parser. The exact anchor is the one
//! ADR-0051 D2 admits: a module-scope or top-level `subroutine test_<name>`
//! whose first dummy argument is declared `type(error_type)` with
//! `intent(out)`, and whose `error_type` accessibility is proven by a
//! `use testdrive` statement in the subroutine's own scope or the enclosing
//! module's. That in-file proof mirrors ADR-0042's DESCRIPTION-declares-
//! testthat identity fact: a test-shaped subroutine without it records a
//! blocking typed `UNKNOWN` instead of an anchor.
//!
//! Nothing here invokes fpm, gfortran, flang, a preprocessor, an include
//! processor, repository code, or a child process. ADR-0034's execution
//! prohibitions all name running Fortran tooling, and nothing runs.
//!
//! The declaration structure is read from the pinned Tree-sitter Fortran
//! grammar (ADR-0058); the ADR-0051 lexer stays the lexical-subset gate and
//! every refusal class is reproduced from the token stream or the tree.
//!
//! Outside the declared subset the parser abstains for the whole file: it
//! never guesses, never recovers, and never resynchronizes past a refusal.
//! Input is untrusted, so bytes, block depth, module nesting, and unit counts
//! are bounded.
//!
//! Every typed `UNKNOWN` this frontend can emit is declared exactly once in
//! [`FORTRAN_OBLIGATION_REGISTRY`], the lane's ADR-0020 gate 4 obligation
//! registry: the claim each unknown scopes, whether an unmet obligation
//! blocks the family claim, and the provider-fallback policy that names what
//! could discharge it. Nothing outside the bounded parse may turn a registry
//! entry into certainty, and a count may fall only when a source-backed
//! replacement fact discharges the same obligation.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
#[cfg(test)]
use super::free_form::lower;
use super::free_form::{self, ident_is, LexAbstention, Token, TokenKind};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::BTreeMap;
use tree_sitter::{Node, Parser as TsParser};

pub const FORTRAN_ANCHOR_ENGINE: &str = "repogrammar-fortran-testdrive-parser";
pub const FORTRAN_ANCHOR_METHOD: &str = "bounded_fortran_testdrive_v1";

/// Fixed support target for the one admitted exact anchor.
pub const FORTRAN_TEST_TARGET: &str = "testdrive.test_subroutine";

const TESTDRIVE_MODULE: &str = "testdrive";
const ERROR_TYPE_NAME: &str = "error_type";
const MAX_UNITS: usize = 4_096;
/// Bound on construct nesting inside a skipped body. Input is untrusted, and a
/// file of nothing but open constructs must abstain rather than loop unbounded.
const MAX_BLOCK_DEPTH: usize = 256;

/// One source-semantic obligation the admitted Fortran family claim rests on.
///
/// This is the lane's ADR-0020 gate 4 registry, mirroring
/// `R_OBLIGATION_REGISTRY`. Every typed `UNKNOWN` the test-drive frontend
/// emits is declared here exactly once, with the claim it scopes, its reason
/// code, whether an unmet obligation blocks the family claim, and the
/// provider-fallback policy that names what could discharge it.
/// `application/family.rs` stays the authoritative claim-impact classifier;
/// this record must agree with it, and the registry tests below pin the
/// emitted vocabulary to this table so the two cannot drift silently.
pub(crate) struct FortranObligation {
    /// Stable affected-claim token (`affected_claim=` assumption).
    pub(crate) affected_claim: &'static str,
    /// Stable kind token (`fortran_unknown_kind=` assumption).
    pub(crate) kind: &'static str,
    /// Stable protocol reason code (the fact target).
    pub(crate) reason: UnknownReasonCode,
    /// A standing obligation rides on every admitted anchor because the
    /// bounded parse can never discharge it; a triggered one fires only on
    /// the construct that leaves it unmet.
    pub(crate) standing: bool,
    /// Recorded claim impact of an unmet obligation. Must agree with the
    /// authoritative classifier in `src/rust/application/family.rs`.
    pub(crate) blocks_family_claim: bool,
    /// Source-free, fixed human-facing evidence note.
    pub(crate) note: &'static str,
    /// Provider-fallback policy as a stable low-cardinality token, mirrored on
    /// every emitted fact as the `provider_fallback=` assumption. The prose
    /// policy each token stands for is the comment above the registry entry.
    pub(crate) fallback: &'static str,
}

/// The complete ADR-0020 gate 4 source-semantic obligation registry for the
/// `fortran.testdrive.test_subroutine` family claim.
///
/// Fallback tokens: `repository_source_declaration` means only the file's own
/// `use testdrive` statement can discharge the obligation, and no provider can
/// substitute for it. `no_fortran_provider_irreducible` means the obligation
/// needs a Fortran semantic provider or build graph, no provider slot exists or
/// is registered, and ADR-0034 forbids executing Fortran tooling — so the
/// residual is irreducible under current constraints.
/// `source_inside_declared_subset_only` means the parse can only read source
/// the ADR-0051 D1 subset admits and never recovers past a refusal.
/// `bounded_input_refusal` means the untrusted-input bounds refused the file
/// and no provider is involved.
pub(crate) const FORTRAN_OBLIGATION_REGISTRY: &[FortranObligation] = &[
    // Fallback: recoverable by repository source only — a `use testdrive`
    // statement proven in the candidate's scope. No provider can substitute
    // for the file's own declaration.
    FortranObligation {
        affected_claim: "fortran_testdrive_identity",
        kind: "testdrive_use_not_proven",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: false,
        blocks_family_claim: true,
        note: "a test-shaped subroutine appears without a proven use testdrive statement in scope, so the framework identity is unproven",
        fallback: "repository_source_declaration",
    },
    // Fallback: recoverable only by the Fortran module/build graph, which
    // needs a compiler-adjacent provider. None exists or is registered and
    // ADR-0034 forbids executing Fortran tooling. Irreducible under current
    // constraints.
    FortranObligation {
        affected_claim: "fortran_testdrive_identity",
        kind: "testdrive_module_binding",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: true,
        blocks_family_claim: false,
        note: "the anchor proves the declaration shape and an in-scope use statement, not which module file satisfies use testdrive in this project",
        fallback: "no_fortran_provider_irreducible",
    },
    // Fallback: registration wiring is runtime state. Only observing the
    // collect/new_unittest call graph could discharge it, no provider is
    // integrated, and ADR-0034 forbids executing repository code. Irreducible
    // under current constraints.
    FortranObligation {
        affected_claim: "fortran_testdrive_registration",
        kind: "test_registration_unproven",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: true,
        blocks_family_claim: false,
        note: "the anchor proves the declaration shape, not that a collect subroutine registers this test with new_unittest",
        fallback: "no_fortran_provider_irreducible",
    },
    // Fallback: source-only obligation. The preprocessor is not evaluated and
    // never will be inside this lane; removing the directive is a source
    // change, not a provider question.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "preprocessor_directive",
        reason: UnknownReasonCode::MacroOrPreprocessor,
        standing: false,
        blocks_family_claim: false,
        note: "a preprocessor line appears in free-form Fortran source, and this frontend does not evaluate the preprocessor, so the file was not read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: source-only obligation. INCLUDE text lives in another file
    // this frontend does not read; removing the statement is a source change.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "include_statement",
        reason: UnknownReasonCode::MacroOrPreprocessor,
        standing: false,
        blocks_family_claim: false,
        note: "an INCLUDE statement pulls text from another file, so the visible source is incomplete and the file was not read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: source-only obligation. The source form is fixed by the file
    // itself; the admitted subset is free form only by ADR-0051 D1.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "fixed_form_signature",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a fixed-form column signature appears in a free-form Fortran file, so the source form is not the admitted one and the file was not read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: source-only obligation. Widening the admitted subset is a
    // superseding ADR decision, not a provider question.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "unadmitted_fortran_construct",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Fortran source left the declared free-form subset, so no later statement in this file was read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: bounded-input refusal on untrusted nesting. No provider is
    // involved.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "parser_depth_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Fortran construct nesting exceeded the bounded block depth, so no later statement in this file was read",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted size. No provider is
    // involved.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "source_byte_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Fortran source exceeded the bounded input-byte limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted unit counts. No provider
    // is involved.
    FortranObligation {
        affected_claim: "fortran_test_parse",
        kind: "parser_resource_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Fortran parser exceeded the bounded unit limit",
        fallback: "bounded_input_refusal",
    },
];

fn registry_entry(kind: &str) -> Result<&'static FortranObligation, ParseError> {
    FORTRAN_OBLIGATION_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind)
        .ok_or_else(|| ParseError::Internal(format!("unregistered Fortran obligation kind {kind}")))
}

fn standing_obligations() -> impl Iterator<Item = &'static FortranObligation> {
    FORTRAN_OBLIGATION_REGISTRY
        .iter()
        .filter(|entry| entry.standing)
}

/// True for the only paths this frontend may read: ADR-0034's lowercase
/// free-form suffix set. Fixed-form suffixes stay inventory upstream and are
/// refused here rather than guessed at.
pub fn is_free_form_fortran_path(path: &str) -> bool {
    let Some(name) = path.rsplit('/').next() else {
        return false;
    };
    ["f90", "f95", "f03", "f08"].iter().any(|suffix| {
        name.ends_with(suffix)
            && name.len() > suffix.len()
            && name[..name.len() - suffix.len()].ends_with('.')
    })
}

#[derive(Debug, Default)]
pub struct FortranTestDriveParser;

impl SourceParser for FortranTestDriveParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
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
    parse_output_with(document, parse_tokens)
}

/// The syntax layer is injected so the ADR-0058 differential can run the
/// retained legacy parser through the identical lexical gate and fact builder.
fn parse_output_with(
    document: SourceDocument<'_>,
    parse_syntax: fn(&str, &[Token]) -> Result<FileParse, ParseError>,
) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Fortran || !is_free_form_fortran_path(document.path) {
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
            "unit:{}#fortran_file:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Fortran,
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

    let (tokens, lex_flags, lex_abstention) = free_form::lex(document.text);
    let mut diagnostics = Vec::new();
    // A lexical well-formedness violation makes a missing anchor uninformative
    // rather than meaningful, so it is reported instead of passed silently.
    if lex_flags.unterminated_string {
        diagnostics.push(degraded(
            document.path,
            "a Fortran quoted literal is left open at end of line, so the following token boundaries in that statement are unreliable",
        ));
    }

    // A lexical refusal refuses the whole file before any anchor can form:
    // the remaining bytes of a preprocessed or fixed-form file do not mean
    // what this frontend would read into them.
    let parsed = match lex_abstention {
        Some(abstention) => FileParse {
            abstention: Some(match abstention {
                LexAbstention::PreprocessorDirective => AbstainKind::PreprocessorDirective,
                LexAbstention::FixedFormSignature => AbstainKind::FixedFormSignature,
                LexAbstention::UnadmittedConstruct => AbstainKind::Unadmitted,
            }),
            ..FileParse::default()
        },
        None => parse_syntax(document.text, &tokens)?,
    };

    if let Some(kind) = parsed.abstention {
        let entry = registry_entry(kind.unknown_kind())?;
        diagnostics.push(degraded(document.path, entry.note));
        facts.push(obligation_fact(&module, entry, module.range.clone())?);
        return finish(units, facts, diagnostics);
    }

    if parsed.test_shape_without_use {
        // The exact spelling of the anchor is present but the import that
        // gives it meaning is not, so the identity is unproven and nothing in
        // the file anchors (ADR-0051 D2 condition 4).
        facts.push(obligation_fact(
            &module,
            registry_entry("testdrive_use_not_proven")?,
            module.range.clone(),
        )?);
    }

    let mut limit_hit = false;
    for (ordinal, anchor) in parsed.anchors.into_iter().enumerate() {
        if ordinal >= MAX_UNITS {
            limit_hit = true;
            break;
        }
        let range = SourceRange::new(anchor.start, anchor.end).map_err(ParseError::Internal)?;
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}",
                document.path,
                CodeUnitKind::FortranTestDriveSubroutine.as_str(),
                anchor.start,
                anchor.end
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Fortran,
            kind: CodeUnitKind::FortranTestDriveSubroutine,
            range: range.clone(),
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(&unit)?);
        // The standing obligations ride on every admitted anchor: the bounded
        // parse proves the declaration shape and nothing beyond it, so which
        // module file satisfies `use testdrive` and whether a collector
        // registers the test stay typed `UNKNOWN` on the anchor itself rather
        // than being guessed away. They are recorded residuals and must never
        // block the family claim.
        for entry in standing_obligations() {
            debug_assert!(
                !entry.blocks_family_claim,
                "a standing Fortran obligation is a recorded residual and must not block"
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

fn anchor_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(FORTRAN_TEST_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded Fortran test-drive test subroutine declaration",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "fortran_anchor_kind=testdrive_test_subroutine".to_string(),
        ],
    })
}

fn obligation_fact(
    unit: &CodeUnit,
    entry: &FortranObligation,
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
            format!("fortran_unknown_kind={}", entry.kind),
            format!("provider_fallback={}", entry.fallback),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: FORTRAN_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: FORTRAN_ANCHOR_METHOD.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Structural parse
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct FileParse {
    anchors: Vec<Anchor>,
    /// A test-shaped subroutine (name, first-dummy interface, everything but
    /// the import) appeared with no proven `use testdrive` in scope.
    test_shape_without_use: bool,
    abstention: Option<AbstainKind>,
}

#[derive(Debug, PartialEq, Eq)]
struct Anchor {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbstainKind {
    PreprocessorDirective,
    FixedFormSignature,
    IncludeStatement,
    Unadmitted,
    DepthLimit,
}

impl AbstainKind {
    fn unknown_kind(self) -> &'static str {
        match self {
            Self::PreprocessorDirective => "preprocessor_directive",
            Self::FixedFormSignature => "fixed_form_signature",
            Self::IncludeStatement => "include_statement",
            Self::Unadmitted => "unadmitted_fortran_construct",
            Self::DepthLimit => "parser_depth_limit",
        }
    }
}

#[cfg(test)]
type Parsed<T> = Result<T, AbstainKind>;

/// The scoping unit a subroutine is declared in. Only module subprograms and
/// external (top-level) subroutines can be test-drive tests; internal
/// procedures and interface bodies never anchor (ADR-0051 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Module,
    TopLevel,
}

/// One dummy-argument declaration record. Only the exact ADR-0051 interface
/// shape sets `error_interface` with `intent_out` and no other attribute.
#[derive(Debug, Default, Clone, Copy)]
struct DeclInfo {
    error_interface: bool,
    intent_out: bool,
    #[allow(dead_code)]
    allocatable: bool,
    other_attr: bool,
}

fn parse_tokens(text: &str, tokens: &[Token]) -> Result<FileParse, ParseError> {
    let source = grammar_source(text, tokens);
    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_fortran::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load Fortran grammar: {error}")))?;
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter Fortran parse failed".to_string()))?;
    Ok(read_file(&source, tokens, tree.root_node()))
}

/// Statement keywords that open a block construct and therefore mark the start
/// of a subroutine's executable body, where declarations can no longer follow.
const BODY_OPENER_KEYWORDS: [&str; 8] = [
    "if",
    "do",
    "select",
    "where",
    "forall",
    "associate",
    "block",
    "critical",
];

/// Tree node kinds that open a nested construct counted against
/// [`MAX_BLOCK_DEPTH`]. `if`, `where`, and `forall` count only in their block
/// form, which is the form that owns an `end_*_statement` child.
const BLOCK_NODE_KINDS: [&str; 10] = [
    "subroutine",
    "function",
    "interface",
    "derived_type_definition",
    "do_loop",
    "select_case_statement",
    "select_type_statement",
    "associate_statement",
    "block_construct",
    "coarray_critical_statement",
];
const BLOCK_FORM_KINDS: [&str; 3] = ["if_statement", "where_statement", "forall_statement"];

/// True for each token that begins a statement: the first token, and the token
/// after a newline or `;` at parenthesis depth zero.
fn statement_starts(tokens: &[Token]) -> Vec<bool> {
    let mut starts = Vec::with_capacity(tokens.len());
    let mut at_start = true;
    let mut paren_depth = 0usize;
    for token in tokens {
        starts.push(at_start);
        match token.kind {
            TokenKind::Newline | TokenKind::Semicolon if paren_depth == 0 => at_start = true,
            TokenKind::LParen => {
                paren_depth += 1;
                at_start = false;
            }
            TokenKind::RParen => {
                paren_depth = paren_depth.saturating_sub(1);
                at_start = false;
            }
            _ => at_start = false,
        }
    }
    starts
}

/// Same-length text handed to the grammar. The grammar cannot read three
/// spellings the ADR-0051 lexical subset admits, so each is rewritten in place
/// without moving any byte offset: a name or number split across a `&`
/// continuation is glued into one token, an unterminated quoted literal is
/// closed at its line end, and the `abstract` of `abstract interface` is
/// blanked. Fact ranges always come from the lexer tokens, never from here.
fn grammar_source(text: &str, tokens: &[Token]) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let starts = statement_starts(tokens);
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Ident | TokenKind::Number
                if bytes[token.start..token.end].contains(&b'&') =>
            {
                let mut glued = Vec::with_capacity(token.end - token.start);
                let mut cursor = token.start;
                while cursor < token.end {
                    match free_form::continuation_end(bytes, cursor) {
                        Some(resume) if bytes[cursor] == b'&' => cursor = resume,
                        _ => {
                            glued.push(bytes[cursor]);
                            cursor += 1;
                        }
                    }
                }
                glued.resize(token.end - token.start, b' ');
                out[token.start..token.end].copy_from_slice(&glued);
            }
            TokenKind::Str if !free_form::string_end(bytes, token.start).1 => {
                if token.end - token.start >= 2 {
                    out[token.start + 1..token.end - 1].fill(b' ');
                    out[token.end - 1] = bytes[token.start];
                } else {
                    // A lone quote cannot be closed in place; any operand
                    // keeps the statement well-formed.
                    out[token.start] = b'0';
                }
            }
            TokenKind::Ident
                if starts[index]
                    && ident_is(text, *token, "abstract")
                    && tokens
                        .get(index + 1)
                        .is_some_and(|next| ident_is(text, *next, "interface")) =>
            {
                out[token.start..token.end].fill(b' ');
            }
            _ => {}
        }
    }
    // Only ASCII bytes inside tokens are rewritten, and the lexer admits no
    // non-ASCII byte in code position; a rewritten string literal keeps its
    // multi-byte content out of the replaced range only when it is blanked.
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// Index of the first token that starts at or after `byte`.
fn token_at(tokens: &[Token], byte: usize) -> usize {
    tokens.partition_point(|token| token.start < byte)
}

struct Reader<'a> {
    /// The grammar-normalized text, so node ranges index it; token ranges
    /// index the original text, which has identical offsets.
    source: &'a str,
    tokens: &'a [Token],
    /// Refusals as `(position, kind)`; the earliest position wins and, at one
    /// position, the earlier-recorded token refusal wins.
    refusals: Vec<(usize, AbstainKind)>,
    result: FileParse,
}

/// Declarations are read off the tree; every ADR-0051 refusal is reproduced
/// from the token stream and the tree. As in a left-to-right parse the earliest
/// refusal wins, and a bare `ERROR` or `MISSING` node refuses the file as the
/// generic unadmitted class only when no more specific refusal applies.
fn read_file(source: &str, tokens: &[Token], root: Node<'_>) -> FileParse {
    let mut reader = Reader {
        source,
        tokens,
        refusals: Vec::new(),
        result: FileParse::default(),
    };
    reader.token_refusals(root);
    reader.depth_refusal(root);
    for child in reader.children(root) {
        match child.kind() {
            "module" => reader.module(child),
            "program" => reader.program(child),
            "subroutine" => reader.subroutine(child, Scope::TopLevel, false),
            "function" => reader.generic_procedure(child),
            _ => reader.refuse(child.start_byte(), AbstainKind::Unadmitted),
        }
    }
    reader.refusals.sort_by_key(|(position, _)| *position);
    let abstention = match reader.refusals.first() {
        Some((_, kind)) => Some(*kind),
        None => first_error(root).map(|_| AbstainKind::Unadmitted),
    };
    let mut result = reader.result;
    if abstention.is_some() {
        result.anchors.clear();
        result.test_shape_without_use = false;
        result.abstention = abstention;
    }
    result
}

/// The first `ERROR` or `MISSING` node in document order.
fn first_error(root: Node<'_>) -> Option<usize> {
    if !root.has_error() {
        return None;
    }
    let mut cursor = root.walk();
    let mut node = root;
    loop {
        if node.is_error() || node.is_missing() {
            return Some(node.start_byte());
        }
        let mut next = None;
        for child in node.children(&mut cursor) {
            if child.has_error() {
                next = Some(child);
                break;
            }
        }
        match next {
            Some(child) => node = child,
            None => return Some(node.start_byte()),
        }
    }
}

impl<'a> Reader<'a> {
    fn refuse(&mut self, position: usize, kind: AbstainKind) {
        self.refusals.push((position, kind));
    }

    /// Named children without comments, which the grammar parses as nodes.
    fn children<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter(|child| child.kind() != "comment")
            .collect()
    }

    fn node_text(&self, node: Node<'_>) -> String {
        self.source[node.byte_range()]
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase()
    }

    /// Lowercased text of a node's `name`-like child, when present.
    fn name_of(&self, node: Node<'_>, kind: &str) -> Option<String> {
        self.children(node)
            .into_iter()
            .find(|child| child.kind() == kind)
            .map(|name| self.node_text(name))
    }

    fn first_token(&self, node: Node<'_>) -> Option<Token> {
        self.tokens
            .get(token_at(self.tokens, node.start_byte()))
            .copied()
    }

    // -- token-stream refusals ------------------------------------------------

    /// `include 'file'` and a labeled `do` at a statement start, found in the
    /// token stream so tree error recovery cannot move them.
    fn token_refusals(&mut self, root: Node<'_>) {
        let starts = statement_starts(self.tokens);
        for (index, token) in self.tokens.iter().enumerate() {
            if !starts[index] || token.kind != TokenKind::Ident {
                continue;
            }
            let next = self.tokens.get(index + 1).map(|next| next.kind);
            let text = self.source_text_of(*token);
            if text.eq_ignore_ascii_case("include") && next == Some(TokenKind::Str) {
                let kind = include_refusal_kind(root, token.start);
                self.refuse(token.start, kind);
            } else if text.eq_ignore_ascii_case("do") && next == Some(TokenKind::Number) {
                // A labeled DO terminates at its label rather than an
                // `end do`, which this frontend does not track.
                self.refuse(token.start, AbstainKind::Unadmitted);
            }
        }
    }

    fn source_text_of(&self, token: Token) -> String {
        self.source[token.start..token.end]
            .chars()
            .filter(|character| !matches!(character, '&' | ' ' | '\t' | '\r' | '\n'))
            .collect()
    }

    /// Nesting depth of constructs, counted from the outermost procedure,
    /// interface, or type body as the legacy skipper did.
    fn depth_refusal(&mut self, root: Node<'_>) {
        let mut stack = vec![(root, 0usize, 0usize)];
        while let Some((node, count, offset)) = stack.pop() {
            let mut count = count;
            let mut offset = offset;
            let counts = BLOCK_NODE_KINDS.contains(&node.kind())
                || (BLOCK_FORM_KINDS.contains(&node.kind())
                    && self
                        .children(node)
                        .iter()
                        .any(|child| child.kind().starts_with("end_")));
            if counts {
                count += 1;
                if count == 1 {
                    offset = usize::from(BLOCK_NODE_KINDS[..4].contains(&node.kind()));
                }
                if count - offset > MAX_BLOCK_DEPTH {
                    self.refuse(node.start_byte(), AbstainKind::DepthLimit);
                    return;
                }
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.named_children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, count, offset));
            }
        }
    }

    // -- program units ----------------------------------------------------

    /// A procedure's first token must be `subroutine`/`function` after only
    /// `pure`/`impure`/`elemental`/`recursive` prefixes; a typed function or a
    /// `module` prefix is outside the subset.
    fn procedure_prefix_is_admitted(&self, node: Node<'_>) -> bool {
        let mut index = token_at(self.tokens, node.start_byte());
        while let Some(token) = self.tokens.get(index) {
            if token.kind != TokenKind::Ident {
                return false;
            }
            let word = self.source_text_of(*token).to_ascii_lowercase();
            match word.as_str() {
                "pure" | "impure" | "elemental" | "recursive" => index += 1,
                "subroutine" | "function" => return true,
                _ => return false,
            }
        }
        false
    }

    /// The closing `end` statement of a unit or spec construct, if the grammar
    /// found one.
    fn end_statement<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        self.children(node)
            .into_iter()
            .rev()
            .find(|child| child.kind().starts_with("end_"))
    }

    /// Reproduce the legacy closing-`end` rule: after `end`, a qualifier from
    /// `qualifiers` (with any trailing name), or the construct's own name.
    fn check_end(&mut self, node: Node<'_>, qualifiers: &[&str], name: Option<&str>) {
        let Some(end) = self.end_statement(node) else {
            return;
        };
        let index = token_at(self.tokens, end.start_byte());
        let Some(next) = self.tokens.get(index + 1).copied() else {
            return;
        };
        if next.kind != TokenKind::Ident {
            return;
        }
        let qualifier = self.source_text_of(next).to_ascii_lowercase();
        if !qualifiers.contains(&qualifier.as_str()) && Some(qualifier.as_str()) != name {
            self.refuse(next.start, AbstainKind::Unadmitted);
        }
    }

    /// Byte offset just past the closing `end` statement's last token.
    fn end_offset(&self, node: Node<'_>) -> Option<usize> {
        let end = self.end_statement(node)?;
        let mut index = token_at(self.tokens, end.start_byte());
        let mut paren_depth = 0usize;
        let mut last = self.tokens.get(index)?.end;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::Newline | TokenKind::Semicolon if paren_depth == 0 => break,
                _ => {}
            }
            last = token.end;
            index += 1;
        }
        Some(last)
    }

    fn module(&mut self, node: Node<'_>) {
        let name = self.name_of_statement(node, "module_statement");
        let children = self.children(node);
        let mut use_proven = false;
        for child in &children {
            if child.kind() == "use_statement" && self.use_proves(*child) {
                use_proven = true;
            }
        }
        for child in children {
            match child.kind() {
                "module_statement" => {}
                "end_module_statement" => {}
                "internal_procedures" => {
                    for procedure in self.children(child) {
                        match procedure.kind() {
                            "contains_statement" => {}
                            "subroutine" => {
                                self.subroutine(procedure, Scope::Module, use_proven);
                            }
                            "function" => self.generic_procedure(procedure),
                            _ => self.refuse(procedure.start_byte(), AbstainKind::Unadmitted),
                        }
                    }
                }
                "module" => self.refuse(child.start_byte(), AbstainKind::Unadmitted),
                _ => self.spec_statement(child, true),
            }
        }
        self.check_end(node, &["module"], name.as_deref());
    }

    fn program(&mut self, node: Node<'_>) {
        let mut in_spec = true;
        for child in self.children(node) {
            match child.kind() {
                "program_statement" | "end_program_statement" => {}
                "internal_procedures" => {
                    in_spec = false;
                    for procedure in self.children(child) {
                        match procedure.kind() {
                            "contains_statement" => {}
                            "subroutine" | "function" => self.generic_procedure(procedure),
                            _ => self.refuse(procedure.start_byte(), AbstainKind::Unadmitted),
                        }
                    }
                }
                _ if !in_spec => {}
                _ => {
                    if self.starts_body(child) {
                        in_spec = false;
                    } else {
                        self.spec_statement(child, true);
                    }
                }
            }
        }
        self.check_end(node, &["program"], None);
    }

    fn starts_body(&self, node: Node<'_>) -> bool {
        self.first_token(node).is_some_and(|token| {
            token.kind == TokenKind::Ident
                && BODY_OPENER_KEYWORDS
                    .contains(&self.source_text_of(token).to_ascii_lowercase().as_str())
        })
    }

    /// A spec-part statement that is not analysed for declarations: a
    /// non-keyword first token refuses in module and program scope, and an
    /// `interface` or type definition must close with its own `end`.
    fn spec_statement(&mut self, node: Node<'_>, refuse_non_keyword: bool) {
        match node.kind() {
            "interface" => self.check_end(node, &["interface"], None),
            "derived_type_definition" => self.check_end(node, &["type"], None),
            _ => {
                if refuse_non_keyword
                    && self
                        .first_token(node)
                        .is_some_and(|token| token.kind != TokenKind::Ident)
                {
                    self.refuse(node.start_byte(), AbstainKind::Unadmitted);
                }
            }
        }
    }

    fn name_of_statement(&self, unit: Node<'_>, statement_kind: &str) -> Option<String> {
        self.children(unit)
            .into_iter()
            .find(|child| child.kind() == statement_kind)
            .and_then(|statement| self.name_of(statement, "name"))
    }

    fn generic_procedure(&mut self, node: Node<'_>) {
        if !self.procedure_prefix_is_admitted(node) {
            self.refuse(node.start_byte(), AbstainKind::Unadmitted);
            return;
        }
        let statement_kind = if node.kind() == "function" {
            "function_statement"
        } else {
            "subroutine_statement"
        };
        let name = self.name_of_statement(node, statement_kind);
        self.check_end(node, &["subroutine", "function"], name.as_deref());
    }

    /// One subroutine at module or top-level scope: decide the ADR-0051 D2
    /// anchor from its dummy list, its declarations, and its `use` proof.
    fn subroutine(&mut self, node: Node<'_>, scope: Scope, module_use_proven: bool) {
        if !self.procedure_prefix_is_admitted(node) {
            self.refuse(node.start_byte(), AbstainKind::Unadmitted);
            return;
        }
        let children = self.children(node);
        let Some(statement) = children
            .iter()
            .find(|child| child.kind() == "subroutine_statement")
        else {
            return;
        };
        let name = self.name_of(*statement, "name").unwrap_or_default();
        let dummies = self.dummy_list(*statement);
        let mut own_use_proven = false;
        let mut declarations = BTreeMap::<String, DeclInfo>::new();
        for child in &children {
            match child.kind() {
                "subroutine_statement" => continue,
                "internal_procedures" => break,
                kind if kind.starts_with("end_") => break,
                _ => {}
            }
            // A non-keyword first token or a block opener ends the declaration
            // part; the rest of the body is skipped.
            let Some(first) = self.first_token(*child) else {
                break;
            };
            if first.kind != TokenKind::Ident || self.starts_body(*child) {
                break;
            }
            match child.kind() {
                "use_statement" => {
                    if self.use_proves(*child) {
                        own_use_proven = true;
                    }
                }
                "interface" | "derived_type_definition" => self.spec_statement(*child, false),
                "include_statement" => {}
                "variable_declaration" if self.is_type_declaration(*child) => {
                    self.type_declaration(*child, &mut declarations);
                }
                _ => self.generic_declaration(*child, &mut declarations),
            }
        }
        self.check_end(node, &["subroutine"], Some(name.as_str()));

        let Some(first_dummy) = dummies.first() else {
            return;
        };
        if !name.starts_with("test_") {
            return;
        }
        let Some(decl) = declarations.get(first_dummy) else {
            return;
        };
        if !(decl.error_interface && decl.intent_out && !decl.other_attr) {
            return;
        }
        if own_use_proven || (scope == Scope::Module && module_use_proven) {
            let (Some(first), Some(end)) = (self.first_token(node), self.end_offset(node)) else {
                return;
            };
            self.result.anchors.push(Anchor {
                start: first.start,
                end,
            });
        } else {
            self.result.test_shape_without_use = true;
        }
    }

    /// The dummy-argument names of a procedure statement; empty when the list
    /// is absent, empty, or outside the admitted all-identifier shape.
    fn dummy_list(&self, statement: Node<'_>) -> Vec<String> {
        let Some(parameters) = self
            .children(statement)
            .into_iter()
            .find(|child| child.kind() == "parameters")
        else {
            return Vec::new();
        };
        let mut dummies = Vec::new();
        let mut cursor = parameters.walk();
        for child in parameters.children(&mut cursor) {
            match child.kind() {
                "identifier" => dummies.push(self.node_text(child)),
                "(" | ")" | "," | "comment" => {}
                _ => return Vec::new(),
            }
        }
        dummies
    }

    // -- use statements and declarations ---------------------------------------

    /// True when a `use` statement proves `error_type` accessible from the
    /// `testdrive` module: a plain `use testdrive`, or an `only` list naming
    /// `error_type` bare and never renamed. Intrinsic, renamed-without-`only`,
    /// and other-module forms prove nothing.
    fn use_proves(&self, node: Node<'_>) -> bool {
        let children = self.children(node);
        let Some(module) = children.iter().find(|child| child.kind() == "module_name") else {
            return false;
        };
        if self.node_text(*module) != TESTDRIVE_MODULE {
            return false;
        }
        let index = token_at(self.tokens, node.start_byte());
        if self.tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::Comma) {
            let attribute = self
                .tokens
                .get(index + 2)
                .map(|token| self.source_text_of(*token).to_ascii_lowercase());
            // `use, intrinsic ::` never reaches the test-drive module, and an
            // unknown attribute is unmodelled; only `non_intrinsic` may prove.
            if attribute.as_deref() != Some("non_intrinsic") {
                return false;
            }
        }
        let only = children
            .iter()
            .find(|child| child.kind() == "included_items")
            .filter(|items| {
                self.first_token(**items)
                    .and_then(|comma| self.tokens.get(token_at(self.tokens, comma.end)))
                    .is_some_and(|token| self.source_text_of(*token).eq_ignore_ascii_case("only"))
            });
        let renames = children.iter().any(|child| child.kind() == "use_alias");
        match only {
            Some(items) => {
                let mut bare = false;
                let mut renamed = renames;
                for item in self.children(*items) {
                    match item.kind() {
                        "identifier" => bare |= self.node_text(item) == ERROR_TYPE_NAME,
                        "use_alias" => {
                            renamed |= self.children(item).iter().any(|part| {
                                part.kind() == "identifier"
                                    && self.node_text(*part) == ERROR_TYPE_NAME
                            });
                        }
                        _ => {}
                    }
                }
                bare && !renamed
            }
            // A rename list without `only:` depends on rename details this
            // frontend does not model.
            None => !renames,
        }
    }

    /// A `type(...)` declaration, as opposed to `class(...)` or an intrinsic
    /// type, which the grammar also parses as `variable_declaration`.
    fn is_type_declaration(&self, node: Node<'_>) -> bool {
        self.children(node).first().is_some_and(|spec| {
            spec.kind() == "derived_type"
                && self
                    .first_token(*spec)
                    .is_some_and(|token| self.source_text_of(token).eq_ignore_ascii_case("type"))
        })
    }

    /// Record a `type(...)` declaration. Only the exact ADR-0051 interface
    /// shape sets `error_interface`; every other spelling records an ordinary
    /// declaration so a first dummy declared elsewhere disqualifies it.
    fn type_declaration(&self, node: Node<'_>, declarations: &mut BTreeMap<String, DeclInfo>) {
        let children = self.children(node);
        let spec = children[0];
        let names_in_spec: Vec<String> = self.tokens[token_at(self.tokens, spec.start_byte())..]
            .iter()
            .take_while(|token| token.start < spec.end_byte())
            .filter(|token| token.kind == TokenKind::Ident)
            .skip(1)
            .map(|token| self.source_text_of(*token).to_ascii_lowercase())
            .collect();
        let mut info = DeclInfo {
            error_interface: names_in_spec == [ERROR_TYPE_NAME],
            ..DeclInfo::default()
        };
        for child in &children[1..] {
            if child.kind() == "type_qualifier" {
                self.apply_qualifier(*child, &mut info);
            }
        }
        for child in &children[1..] {
            let name = match child.kind() {
                "type_qualifier" => continue,
                "identifier" => Some(self.node_text(*child)),
                _ => self
                    .children(*child)
                    .into_iter()
                    .find(|part| part.kind() == "identifier")
                    .map(|part| self.node_text(part)),
            };
            if let Some(name) = name {
                record_name(declarations, name, info);
            }
        }
    }

    fn apply_qualifier(&self, qualifier: Node<'_>, info: &mut DeclInfo) {
        let index = token_at(self.tokens, qualifier.start_byte());
        let word = self
            .tokens
            .get(index)
            .map(|token| self.source_text_of(*token).to_ascii_lowercase())
            .unwrap_or_default();
        match word.as_str() {
            "intent" => {
                if self.tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::LParen) {
                    if let Some(value) = self
                        .tokens
                        .get(index + 2)
                        .filter(|token| token.kind == TokenKind::Ident)
                    {
                        info.intent_out |= self.source_text_of(*value).eq_ignore_ascii_case("out");
                    }
                } else {
                    info.other_attr = true;
                }
            }
            "allocatable" => info.allocatable = true,
            _ => info.other_attr = true,
        }
    }

    /// A specification statement other than a `type(...)` declaration: when it
    /// carries a `::` at parenthesis depth zero, its entity names are declared
    /// with some other type and disqualify a first dummy.
    fn generic_declaration(&self, node: Node<'_>, declarations: &mut BTreeMap<String, DeclInfo>) {
        let first = token_at(self.tokens, node.start_byte());
        let keyword = self
            .tokens
            .get(first)
            .map(|token| self.source_text_of(*token).to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(
            keyword.as_str(),
            "implicit" | "public" | "private" | "import"
        ) {
            return;
        }
        let mut paren_depth = 0usize;
        let mut index = first;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::DoubleColon if paren_depth == 0 => {
                    self.record_names_from(index + 1, declarations);
                    return;
                }
                TokenKind::Newline | TokenKind::Semicolon => return,
                _ => {}
            }
            index += 1;
        }
    }

    fn record_names_from(&self, mut index: usize, declarations: &mut BTreeMap<String, DeclInfo>) {
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::Ident => {
                    record_name(
                        declarations,
                        self.source_text_of(*token).to_ascii_lowercase(),
                        DeclInfo::default(),
                    );
                    index += 1;
                    if self.tokens.get(index).map(|next| next.kind) == Some(TokenKind::LParen) {
                        let mut paren_depth = 0usize;
                        while let Some(inner) = self.tokens.get(index) {
                            index += 1;
                            match inner.kind {
                                TokenKind::LParen => paren_depth += 1,
                                TokenKind::RParen => {
                                    paren_depth = paren_depth.saturating_sub(1);
                                    if paren_depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                TokenKind::Comma => index += 1,
                _ => return,
            }
        }
    }
}

/// A name declared twice is illegal Fortran; the conflict disqualifies rather
/// than deciding which declaration wins.
fn record_name(declarations: &mut BTreeMap<String, DeclInfo>, name: String, info: DeclInfo) {
    match declarations.get(&name) {
        Some(_) => {
            declarations.insert(name, DeclInfo::default());
        }
        None => {
            declarations.insert(name, info);
        }
    }
}

/// An `include` is a file-inclusion refusal wherever the legacy parse walked
/// statements, and a plain unadmitted construct at program-unit level and
/// among a module's or program's contained procedures.
fn include_refusal_kind(root: Node<'_>, position: usize) -> AbstainKind {
    let mut node = root
        .descendant_for_byte_range(position, position + 1)
        .unwrap_or(root);
    loop {
        match node.kind() {
            "translation_unit" => return AbstainKind::Unadmitted,
            "internal_procedures"
                if matches!(
                    node.parent().map(|parent| parent.kind()),
                    Some("module") | Some("program")
                ) =>
            {
                return AbstainKind::Unadmitted;
            }
            "module"
            | "program"
            | "subroutine"
            | "function"
            | "interface"
            | "derived_type_definition"
            | "internal_procedures" => {
                return AbstainKind::IncludeStatement;
            }
            _ => {}
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return AbstainKind::Unadmitted,
        }
    }
}

#[cfg(test)]
struct LegacyParser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
}

#[cfg(test)]
fn legacy_parse_tokens(text: &str, tokens: &[Token]) -> FileParse {
    let mut parser = LegacyParser {
        text,
        tokens: tokens.to_vec(),
        position: 0,
    };
    let mut result = FileParse::default();
    if let Err(kind) = parser.parse_program_units(&mut result) {
        result.anchors.clear();
        result.test_shape_without_use = false;
        result.abstention = Some(kind);
    }
    result
}

#[cfg(test)]
impl<'a> LegacyParser<'a> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<Token> {
        self.tokens.get(self.position + offset).copied()
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.peek();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn skip_separators(&mut self) {
        while matches!(
            self.peek().map(|token| token.kind),
            Some(TokenKind::Newline) | Some(TokenKind::Semicolon)
        ) {
            self.position += 1;
        }
    }

    fn ident_is_at(&self, offset: usize, expected: &str) -> bool {
        self.peek_at(offset)
            .is_some_and(|token| ident_is(self.text, token, expected))
    }

    /// True at a derived-type definition statement (`type [::] name`), as
    /// opposed to a `type(...)` type-spec in a declaration.
    fn at_type_definition(&self) -> bool {
        match self.peek_at(1).map(|token| token.kind) {
            Some(TokenKind::Ident) => !self.ident_is_at(1, "is"),
            Some(TokenKind::DoubleColon) => self
                .peek_at(2)
                .is_some_and(|token| token.kind == TokenKind::Ident),
            _ => false,
        }
    }

    /// Tokens in the type-definition header (`type`, the optional `::`, the
    /// name) that the caller consumes before skipping to `end type`.
    fn type_definition_header_len(&self) -> usize {
        if self.peek_at(1).map(|token| token.kind) == Some(TokenKind::DoubleColon) {
            3
        } else {
            2
        }
    }

    /// Advance past the remainder of the current statement: up to a `;`, a
    /// newline, or end of file, all at parenthesis depth zero.
    fn skip_statement(&mut self) {
        let mut paren_depth = 0usize;
        while let Some(token) = self.peek() {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::Newline | TokenKind::Semicolon if paren_depth == 0 => break,
                _ => {}
            }
            self.position += 1;
        }
    }

    fn skip_balanced_group(&mut self) {
        let mut paren_depth = 0usize;
        while let Some(token) = self.peek() {
            self.position += 1;
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => {
                    paren_depth = paren_depth.saturating_sub(1);
                    if paren_depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // -- file-level program units ------------------------------------------

    fn parse_program_units(&mut self, result: &mut FileParse) -> Parsed<()> {
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Ok(());
            };
            if token.kind != TokenKind::Ident {
                return Err(AbstainKind::Unadmitted);
            }
            let keyword = lower(self.text, token);
            match keyword.as_str() {
                "module" => {
                    if ["procedure", "function", "subroutine"]
                        .iter()
                        .any(|next| self.ident_is_at(1, next))
                    {
                        // A `module procedure`-style separate-module prefix
                        // belongs to submodules, which are outside the
                        // admitted subset.
                        return Err(AbstainKind::Unadmitted);
                    }
                    self.parse_module(result)?;
                }
                "program" => self.parse_program()?,
                "subroutine" => {
                    let start = token.start;
                    self.parse_subroutine(Scope::TopLevel, false, start, result)?;
                }
                "function" => {
                    self.parse_generic_procedure()?;
                }
                "pure" | "impure" | "elemental" | "recursive" => {
                    let start = token.start;
                    if let Some(kind) = self.consume_prefixes()? {
                        if kind == "subroutine" {
                            self.parse_subroutine(Scope::TopLevel, false, start, result)?;
                        } else {
                            self.parse_generic_procedure()?;
                        }
                    } else {
                        // Prefix words that name no procedure are a
                        // statement outside the admitted subset.
                        return Err(AbstainKind::Unadmitted);
                    }
                }
                _ => return Err(AbstainKind::Unadmitted),
            }
        }
    }

    /// Consume procedure prefix keywords. Returns the `subroutine`/
    /// `function` keyword that follows, or `None` when the statement ends
    /// without one.
    fn consume_prefixes(&mut self) -> Parsed<Option<String>> {
        while let Some(token) = self.peek() {
            if token.kind == TokenKind::Ident {
                let keyword = lower(self.text, token);
                match keyword.as_str() {
                    "pure" | "impure" | "elemental" | "recursive" => {
                        self.position += 1;
                    }
                    "subroutine" | "function" => return Ok(Some(keyword)),
                    _ => return Ok(None),
                }
            } else {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn parse_module(&mut self, result: &mut FileParse) -> Parsed<()> {
        self.position += 1; // `module`
        let Some(name) = self.peek().filter(|token| token.kind == TokenKind::Ident) else {
            return Err(AbstainKind::Unadmitted);
        };
        let name = lower(self.text, name);
        self.position += 1;
        self.skip_statement();
        let mut use_proven = false;
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Err(AbstainKind::Unadmitted);
            };
            if token.kind != TokenKind::Ident {
                return Err(AbstainKind::Unadmitted);
            }
            match lower(self.text, token).as_str() {
                "use" => {
                    if self.parse_use()?.proves {
                        use_proven = true;
                    }
                }
                "implicit" | "public" | "private" | "import" => self.skip_statement(),
                "include" if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::Str) => {
                    return Err(AbstainKind::IncludeStatement);
                }
                "interface" => {
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "abstract" if self.ident_is_at(1, "interface") => {
                    self.position += 2;
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "type" => {
                    if self.at_type_definition() {
                        self.position += self.type_definition_header_len();
                        self.skip_statement();
                        self.skip_to_construct_end(&["type"], None)?;
                    } else {
                        self.skip_statement();
                    }
                }
                "contains" => {
                    self.position += 1;
                    self.parse_module_subprograms(use_proven, &name, result)?;
                    return Ok(());
                }
                "end" => {
                    self.expect_construct_end(&["module"], Some(&name))?;
                    return Ok(());
                }
                // A module nested inside a module is illegal Fortran and
                // outside the one-level nesting budget either way.
                "module" => return Err(AbstainKind::Unadmitted),
                _ => self.skip_statement(),
            }
        }
    }

    fn parse_module_subprograms(
        &mut self,
        module_use_proven: bool,
        module_name: &str,
        result: &mut FileParse,
    ) -> Parsed<()> {
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Err(AbstainKind::Unadmitted);
            };
            if token.kind != TokenKind::Ident {
                return Err(AbstainKind::Unadmitted);
            }
            match lower(self.text, token).as_str() {
                "subroutine" => {
                    let start = token.start;
                    self.parse_subroutine(Scope::Module, module_use_proven, start, result)?;
                }
                "function" => self.parse_generic_procedure()?,
                "pure" | "impure" | "elemental" | "recursive" => {
                    let start = token.start;
                    if let Some(kind) = self.consume_prefixes()? {
                        if kind == "subroutine" {
                            self.parse_subroutine(Scope::Module, module_use_proven, start, result)?;
                        } else {
                            self.parse_generic_procedure()?;
                        }
                    } else {
                        return Err(AbstainKind::Unadmitted);
                    }
                }
                "end" => {
                    self.expect_construct_end(&["module"], Some(module_name))?;
                    return Ok(());
                }
                _ => return Err(AbstainKind::Unadmitted),
            }
        }
    }

    fn parse_program(&mut self) -> Parsed<()> {
        self.position += 1; // `program`
        self.skip_statement();
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Err(AbstainKind::Unadmitted);
            };
            if token.kind != TokenKind::Ident {
                return Err(AbstainKind::Unadmitted);
            }
            match lower(self.text, token).as_str() {
                "use" | "implicit" | "public" | "private" | "import" => self.skip_statement(),
                "include" if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::Str) => {
                    return Err(AbstainKind::IncludeStatement);
                }
                "interface" => {
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "abstract" if self.ident_is_at(1, "interface") => {
                    self.position += 2;
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "type" => {
                    if self.at_type_definition() {
                        self.position += self.type_definition_header_len();
                        self.skip_statement();
                        self.skip_to_construct_end(&["type"], None)?;
                    } else {
                        self.skip_statement();
                    }
                }
                "contains" => {
                    self.position += 1;
                    // Internal procedures of a program can never be passed as
                    // test-drive entries, so their bodies are skipped without
                    // candidate analysis (ADR-0051 D2).
                    loop {
                        self.skip_separators();
                        let Some(token) = self.peek() else {
                            return Err(AbstainKind::Unadmitted);
                        };
                        if token.kind != TokenKind::Ident {
                            return Err(AbstainKind::Unadmitted);
                        }
                        match lower(self.text, token).as_str() {
                            "subroutine" | "function" => self.parse_generic_procedure()?,
                            "pure" | "impure" | "elemental" | "recursive" => {
                                if self.consume_prefixes()?.is_none() {
                                    return Err(AbstainKind::Unadmitted);
                                }
                                self.parse_generic_procedure()?;
                            }
                            "end" => {
                                self.expect_construct_end(&["program"], None)?;
                                return Ok(());
                            }
                            _ => return Err(AbstainKind::Unadmitted),
                        }
                    }
                }
                "end" => {
                    self.expect_construct_end(&["program"], None)?;
                    return Ok(());
                }
                keyword if BODY_OPENER_KEYWORDS.contains(&keyword) => {
                    // The executable body of the program began; the balanced
                    // skipper owns everything from here to its `end`.
                    self.skip_to_construct_end(&["program"], None)?;
                    return Ok(());
                }
                _ => self.skip_statement(),
            }
        }
    }

    /// Parse one subroutine whose statement begins at `start`, deciding the
    /// ADR-0051 D2 anchor and consuming through its closing `end`.
    fn parse_subroutine(
        &mut self,
        scope: Scope,
        module_use_proven: bool,
        start: usize,
        result: &mut FileParse,
    ) -> Parsed<()> {
        self.position += 1; // `subroutine`
        let Some(name_token) = self.peek().filter(|token| token.kind == TokenKind::Ident) else {
            return Err(AbstainKind::Unadmitted);
        };
        let name = lower(self.text, name_token);
        self.position += 1;
        let dummies = self.parse_dummy_list();
        self.skip_statement();

        let mut own_use_proven = false;
        let mut declarations = BTreeMap::<String, DeclInfo>::new();
        let end = loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Err(AbstainKind::Unadmitted);
            };
            // A non-keyword first token (a statement label's number, an
            // assignment) means the executable body has begun; the balanced
            // skipper owns everything from here to the closing `end`.
            let keyword = if token.kind == TokenKind::Ident {
                lower(self.text, token)
            } else {
                break self.skip_to_construct_end(&["subroutine"], Some(&name))?;
            };
            match keyword.as_str() {
                "use" => {
                    if self.parse_use()?.proves {
                        own_use_proven = true;
                    }
                }
                "implicit" | "public" | "private" | "import" => self.skip_statement(),
                "include" if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::Str) => {
                    return Err(AbstainKind::IncludeStatement);
                }
                "interface" => {
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "abstract" if self.ident_is_at(1, "interface") => {
                    self.position += 2;
                    self.skip_statement();
                    self.skip_to_construct_end(&["interface"], None)?;
                }
                "type" => {
                    if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::LParen) {
                        self.parse_type_declaration(&mut declarations);
                    } else if self.at_type_definition() {
                        self.position += self.type_definition_header_len();
                        self.skip_statement();
                        self.skip_to_construct_end(&["type"], None)?;
                    } else {
                        self.skip_statement();
                    }
                }
                "contains" => {
                    // Internal procedures follow; they never anchor, and the
                    // shared block skip keeps their own nesting balanced.
                    self.position += 1;
                    break self.skip_to_construct_end(&["subroutine"], Some(&name))?;
                }
                "end" => {
                    break self.expect_construct_end(&["subroutine"], Some(&name))?;
                }
                keyword if BODY_OPENER_KEYWORDS.contains(&keyword) => {
                    break self.skip_to_construct_end(&["subroutine"], Some(&name))?;
                }
                _ => {
                    self.scan_generic_declaration(&mut declarations);
                }
            }
        };

        let Some(first_dummy) = dummies.first() else {
            return Ok(());
        };
        if !name.starts_with("test_") {
            return Ok(());
        }
        let Some(decl) = declarations.get(first_dummy) else {
            return Ok(());
        };
        let interface_shape = decl.error_interface && decl.intent_out && !decl.other_attr;
        if !interface_shape {
            return Ok(());
        }
        if own_use_proven || (scope == Scope::Module && module_use_proven) {
            result.anchors.push(Anchor { start, end });
        } else {
            result.test_shape_without_use = true;
        }
        Ok(())
    }

    /// The dummy-argument name list of a procedure statement, lowercased.
    /// Empty when the list is absent, unparseable, or empty.
    fn parse_dummy_list(&mut self) -> Vec<String> {
        if self.peek().map(|token| token.kind) != Some(TokenKind::LParen) {
            return Vec::new();
        }
        self.position += 1;
        let mut dummies = Vec::new();
        loop {
            match self.peek().map(|token| token.kind) {
                Some(TokenKind::Ident) => {
                    let token = self.bump().expect("checked");
                    dummies.push(lower(self.text, token));
                }
                Some(TokenKind::Comma) => {
                    self.position += 1;
                }
                Some(TokenKind::RParen) => {
                    self.position += 1;
                    return dummies;
                }
                _ => {
                    // A dummy list outside the admitted shape (alternate
                    // returns, `*` lengths) yields no candidate facts.
                    self.skip_statement();
                    return Vec::new();
                }
            }
        }
    }

    /// Skip a non-candidate procedure through its closing `end`.
    fn parse_generic_procedure(&mut self) -> Parsed<()> {
        self.position += 1; // `subroutine` or `function`
        let name = self
            .peek()
            .filter(|token| token.kind == TokenKind::Ident)
            .map(|token| lower(self.text, token));
        if name.is_some() {
            self.position += 1;
        }
        self.skip_statement();
        self.skip_to_construct_end(&["subroutine", "function"], name.as_deref())?;
        Ok(())
    }

    // -- use statements -----------------------------------------------------

    fn parse_use(&mut self) -> Parsed<UseProof> {
        self.position += 1; // `use`
        let mut intrinsic = false;
        if self.peek().map(|token| token.kind) == Some(TokenKind::Comma) {
            self.position += 1;
            if self.ident_is_at(0, "intrinsic") {
                intrinsic = true;
                self.position += 1;
            } else if self.ident_is_at(0, "non_intrinsic") {
                self.position += 1;
            } else {
                self.skip_statement();
                return Ok(UseProof { proves: false });
            }
            if self.peek().map(|token| token.kind) != Some(TokenKind::DoubleColon) {
                self.skip_statement();
                return Ok(UseProof { proves: false });
            }
            self.position += 1;
        } else if self.peek().map(|token| token.kind) == Some(TokenKind::DoubleColon) {
            self.position += 1;
        }
        let Some(module) = self.peek().filter(|token| token.kind == TokenKind::Ident) else {
            self.skip_statement();
            return Ok(UseProof { proves: false });
        };
        let module = lower(self.text, module);
        self.position += 1;

        let mut proof = UseProof { proves: false };
        if self.peek().map(|token| token.kind) == Some(TokenKind::Comma) {
            self.position += 1;
            if self.ident_is_at(0, "only") {
                self.position += 1;
                if self.peek().map(|token| token.kind) == Some(TokenKind::Colon) {
                    self.position += 1;
                }
                let mut bare_error_type = false;
                let mut renamed_error_type = false;
                loop {
                    match self.peek().map(|token| token.kind) {
                        Some(TokenKind::Newline) | Some(TokenKind::Semicolon) | None => break,
                        Some(TokenKind::Comma) => {
                            self.position += 1;
                        }
                        Some(TokenKind::Ident) => {
                            let token = self.bump().expect("checked");
                            if matches!(
                                self.peek().map(|next| next.kind),
                                Some(TokenKind::FatArrow)
                            ) {
                                self.position += 1;
                                if let Some(right) =
                                    self.peek().filter(|next| next.kind == TokenKind::Ident)
                                {
                                    if lower(self.text, right) == ERROR_TYPE_NAME {
                                        renamed_error_type = true;
                                    }
                                    self.position += 1;
                                }
                            } else if lower(self.text, token) == ERROR_TYPE_NAME {
                                bare_error_type = true;
                            }
                        }
                        Some(TokenKind::LParen) => {
                            // `operator(...)`-style items carry no entity name.
                            self.skip_balanced_group();
                        }
                        _ => {
                            self.skip_statement();
                            break;
                        }
                    }
                }
                proof.proves = bare_error_type && !renamed_error_type;
            } else {
                // A rename list without `only:`: whether `error_type` stays
                // accessible depends on rename-list details this frontend
                // does not model, so the statement proves nothing.
                self.skip_statement();
            }
        } else {
            proof.proves = true;
        }
        self.skip_statement();
        if module != TESTDRIVE_MODULE || intrinsic {
            proof.proves = false;
        }
        Ok(proof)
    }

    // -- declarations -------------------------------------------------------

    /// Parse one `type(...)` declaration statement, recording the declared
    /// names. Only the exact ADR-0051 interface shape sets
    /// `error_interface`; every other spelling records an ordinary
    /// declaration so a first dummy declared elsewhere disqualifies the
    /// candidate.
    fn parse_type_declaration(&mut self, declarations: &mut BTreeMap<String, DeclInfo>) {
        self.position += 1; // `type`
        self.position += 1; // `(`
        let mut content = Vec::new();
        let mut paren_depth = 1usize;
        while let Some(token) = self.peek() {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        self.position += 1;
                        break;
                    }
                }
                TokenKind::Newline | TokenKind::Semicolon => break,
                TokenKind::Ident => content.push(lower(self.text, token)),
                _ => {}
            }
            self.position += 1;
        }
        let is_error_interface = content.len() == 1 && content[0] == ERROR_TYPE_NAME;

        let mut info = DeclInfo {
            error_interface: is_error_interface,
            ..DeclInfo::default()
        };
        let mut has_attributes = false;
        loop {
            match self.peek().map(|token| token.kind) {
                Some(TokenKind::Comma) => {
                    has_attributes = true;
                    self.position += 1;
                    let Some(token) = self.peek() else {
                        break;
                    };
                    if token.kind != TokenKind::Ident {
                        self.skip_statement();
                        return;
                    }
                    match lower(self.text, token).as_str() {
                        "intent" => {
                            self.position += 1;
                            if self.peek().map(|next| next.kind) == Some(TokenKind::LParen) {
                                self.position += 1;
                                if let Some(value) =
                                    self.peek().filter(|next| next.kind == TokenKind::Ident)
                                {
                                    info.intent_out |= lower(self.text, value) == "out";
                                    self.position += 1;
                                }
                                while let Some(next) = self.peek() {
                                    if matches!(
                                        next.kind,
                                        TokenKind::Newline | TokenKind::Semicolon
                                    ) {
                                        break;
                                    }
                                    self.position += 1;
                                    if next.kind == TokenKind::RParen {
                                        break;
                                    }
                                }
                            } else {
                                info.other_attr = true;
                            }
                        }
                        "allocatable" => {
                            info.allocatable = true;
                            self.position += 1;
                        }
                        _ => {
                            info.other_attr = true;
                            self.position += 1;
                            // An attribute may carry a `(...)` group.
                            if self.peek().map(|next| next.kind) == Some(TokenKind::LParen) {
                                self.skip_balanced_group();
                            }
                        }
                    }
                }
                Some(TokenKind::DoubleColon) => {
                    self.position += 1;
                    self.record_names(declarations, info);
                    self.skip_statement();
                    return;
                }
                _ => {
                    if !has_attributes {
                        // Old-style declaration without `::`: the entity
                        // names follow the type spec directly.
                        self.record_names(declarations, info);
                    }
                    self.skip_statement();
                    return;
                }
            }
        }
        self.skip_statement();
    }

    fn record_names(&mut self, declarations: &mut BTreeMap<String, DeclInfo>, info: DeclInfo) {
        loop {
            match self.peek().map(|token| token.kind) {
                Some(TokenKind::Ident) => {
                    let token = self.bump().expect("checked");
                    let name = lower(self.text, token);
                    match declarations.get(&name) {
                        Some(_) => {
                            // A name declared twice is illegal Fortran; treat
                            // the conflict as disqualifying rather than
                            // deciding which declaration wins.
                            declarations.insert(name, DeclInfo::default());
                        }
                        None => {
                            declarations.insert(name, info);
                        }
                    }
                    // Entity shapes like `(…)` or `*n` and initializers are
                    // skipped by the caller's statement skip.
                    if self.peek().map(|next| next.kind) == Some(TokenKind::LParen) {
                        self.skip_balanced_group();
                    }
                }
                Some(TokenKind::Comma) => {
                    self.position += 1;
                }
                _ => return,
            }
        }
    }

    /// A non-`type` specification statement: when it carries a `::` at
    /// parenthesis depth zero, its entity names are declared with some other
    /// type and must disqualify a first dummy. Statements without `::` are
    /// skipped without recording.
    fn scan_generic_declaration(&mut self, declarations: &mut BTreeMap<String, DeclInfo>) {
        let start = self.position;
        let mut paren_depth = 0usize;
        let mut double_colon = None;
        let mut cursor = self.position;
        while let Some(token) = self.tokens.get(cursor).copied() {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::DoubleColon if paren_depth == 0 => {
                    double_colon = Some(cursor);
                    break;
                }
                TokenKind::Newline | TokenKind::Semicolon => break,
                _ => {}
            }
            cursor += 1;
        }
        let Some(double_colon) = double_colon else {
            self.skip_statement();
            return;
        };
        self.position = double_colon + 1;
        self.record_names(declarations, DeclInfo::default());
        self.position = start;
        self.skip_statement();
    }

    // -- balanced construct skipping ---------------------------------------

    /// Skip statements until the `end` that closes the current construct.
    ///
    /// `qualifiers` names the `end`-keyword spellings that may close it (for
    /// example `end interface`); `name` allows the `end <name>` form. Returns
    /// the byte offset just past the closing `end` statement.
    fn skip_to_construct_end(&mut self, qualifiers: &[&str], name: Option<&str>) -> Parsed<usize> {
        let mut depth = 0usize;
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Err(AbstainKind::Unadmitted);
            };
            if token.kind != TokenKind::Ident {
                self.skip_statement();
                continue;
            }
            match lower(self.text, token).as_str() {
                "end" => {
                    if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::Equals) {
                        // An assignment to a variable spelled `end`.
                        self.skip_statement();
                        continue;
                    }
                    if depth == 0 {
                        return self.expect_construct_end(qualifiers, name);
                    }
                    depth -= 1;
                    self.skip_statement();
                }
                // `endif`/`enddo` are legal free-form closers.
                "endif" | "enddo" => {
                    if depth == 0 {
                        return Err(AbstainKind::Unadmitted);
                    }
                    depth -= 1;
                    self.skip_statement();
                }
                "include" if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::Str) => {
                    // INCLUDE pulls text from another file, so the visible
                    // source is incomplete and the whole file abstains.
                    return Err(AbstainKind::IncludeStatement);
                }
                "subroutine" | "function" => {
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH {
                        return Err(AbstainKind::DepthLimit);
                    }
                    self.skip_statement();
                }
                "pure" | "impure" | "elemental" | "recursive" => {
                    let save = self.position;
                    if self.consume_prefixes()?.is_some() {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                        self.skip_statement();
                    } else {
                        self.position = save;
                        self.skip_statement();
                    }
                }
                "do" => match self.peek_at(1).map(|next| next.kind) {
                    Some(TokenKind::Equals) => self.skip_statement(),
                    Some(TokenKind::Number) => {
                        // A labeled DO terminates at its label rather than an
                        // `end do`, which this frontend does not track.
                        return Err(AbstainKind::Unadmitted);
                    }
                    _ => {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                        self.skip_statement();
                    }
                },
                "if" => {
                    if self.peek_at(1).map(|next| next.kind) != Some(TokenKind::Equals)
                        && self.statement_has_trailing_then(1)
                    {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "else" => {
                    if self.ident_is_at(1, "if") && self.statement_has_trailing_then(2) {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "select" => {
                    if ["case", "type", "default"]
                        .iter()
                        .any(|next| self.ident_is_at(1, next))
                    {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "where" | "forall" => {
                    if self.group_is_followed_by_statement_end(1) {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "associate" => {
                    if self.peek_at(1).map(|next| next.kind) == Some(TokenKind::LParen) {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "block" | "critical" => {
                    if matches!(
                        self.peek_at(1).map(|next| next.kind),
                        Some(TokenKind::Newline) | Some(TokenKind::Semicolon) | None
                    ) {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                "interface" | "abstract" if token.kind == TokenKind::Ident => {
                    if lower(self.text, token) == "abstract" && !self.ident_is_at(1, "interface") {
                        // A name spelled `abstract` that opens no interface.
                        self.skip_statement();
                        continue;
                    }
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH {
                        return Err(AbstainKind::DepthLimit);
                    }
                    self.skip_statement();
                }
                "module" => {
                    if self
                        .peek_at(1)
                        .is_some_and(|next| next.kind == TokenKind::Ident)
                        && !["procedure", "function", "subroutine"]
                            .iter()
                            .any(|next| self.ident_is_at(1, next))
                    {
                        // A module nested inside a one-level construct is
                        // illegal Fortran and exceeds the nesting budget.
                        return Err(AbstainKind::Unadmitted);
                    }
                    self.skip_statement();
                }
                "type" => {
                    if self.at_type_definition() {
                        depth += 1;
                        if depth > MAX_BLOCK_DEPTH {
                            return Err(AbstainKind::DepthLimit);
                        }
                    }
                    self.skip_statement();
                }
                _ => self.skip_statement(),
            }
        }
    }

    /// True when the `if` statement starting `offset` tokens ahead ends with
    /// `then`, making it a block-opening `if`.
    fn statement_has_trailing_then(&self, offset: usize) -> bool {
        let mut cursor = self.position + offset;
        let mut paren_depth = 0usize;
        let mut last_was_then = false;
        while let Some(token) = self.tokens.get(cursor).copied() {
            match token.kind {
                TokenKind::LParen => {
                    paren_depth += 1;
                    last_was_then = false;
                }
                TokenKind::RParen => {
                    paren_depth = paren_depth.saturating_sub(1);
                    last_was_then = false;
                }
                TokenKind::Newline | TokenKind::Semicolon => break,
                TokenKind::Ident => {
                    last_was_then = paren_depth == 0 && ident_is(self.text, token, "then");
                }
                _ => last_was_then = false,
            }
            cursor += 1;
        }
        last_was_then
    }

    /// True when the parenthesis group starting `offset` tokens ahead is the
    /// whole statement, the block-construct form of `where`/`forall`.
    fn group_is_followed_by_statement_end(&self, offset: usize) -> bool {
        let mut cursor = self.position + offset;
        if self.tokens.get(cursor).map(|token| token.kind) != Some(TokenKind::LParen) {
            return false;
        }
        let mut paren_depth = 0usize;
        while let Some(token) = self.tokens.get(cursor).copied() {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        return matches!(
                            self.tokens.get(cursor + 1).map(|next| next.kind),
                            Some(TokenKind::Newline) | Some(TokenKind::Semicolon) | None
                        );
                    }
                }
                TokenKind::Newline | TokenKind::Semicolon => return false,
                _ => {}
            }
            cursor += 1;
        }
        false
    }

    /// Consume a closing `end` statement and return the byte offset just past
    /// its last token.
    fn expect_construct_end(&mut self, qualifiers: &[&str], name: Option<&str>) -> Parsed<usize> {
        let end_token = self.bump().expect("caller checked `end`");
        let mut last_end = end_token.end;
        if let Some(token) = self.peek() {
            if token.kind == TokenKind::Ident {
                let qualifier = lower(self.text, token);
                if qualifiers.contains(&qualifier.as_str()) {
                    self.position += 1;
                    last_end = token.end;
                    // The construct's own name may follow the qualifier.
                    if let Some(trailing) = self.peek() {
                        if trailing.kind == TokenKind::Ident {
                            self.position += 1;
                            last_end = trailing.end;
                        }
                    }
                } else if Some(qualifier.as_str()) == name {
                    // `end <name>` closing the named construct itself.
                    self.position += 1;
                    last_end = token.end;
                } else {
                    return Err(AbstainKind::Unadmitted);
                }
            }
        }
        self.skip_statement();
        if let Some(previous) = self.tokens.get(self.position.saturating_sub(1)) {
            if previous.end > last_end {
                last_end = previous.end;
            }
        }
        Ok(last_end)
    }
}

#[cfg(test)]
#[derive(Debug)]
struct UseProof {
    proves: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    const CANONICAL_SUITE: &str = "\
module test_suite
  use testdrive, only : new_unittest, unittest_type, error_type, check
  implicit none
  private

  public :: collect_suite

contains

  subroutine collect_suite(testsuite)
    type(unittest_type), allocatable, intent(out) :: testsuite(:)

    testsuite = [ &
      new_unittest(\"sizes\", test_sizes), &
      new_unittest(\"bounds\", test_bounds, should_fail=.true.) &
    ]

  end subroutine collect_suite

  subroutine test_sizes(error)
    type(error_type), allocatable, intent(out) :: error

    call check(error, 1 + 2 == 3)
    if (allocated(error)) return

  end subroutine test_sizes

  subroutine test_bounds(error, first, last)
    type(error_type), intent(out), allocatable :: error
    integer, intent(in) :: first, last

    if (first > last) then
      call test_failed(error, \"bounds inverted\")
      return
    end if

  end subroutine test_bounds

end module test_suite
";

    const TOP_LEVEL_TESTS: &str = "\
program tester
  use, intrinsic :: iso_fortran_env, only : error_unit
  implicit none
contains

  subroutine helper()
  end subroutine helper

end program tester

subroutine test_first(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_first

subroutine test_second(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_second

subroutine test_third(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_third
";

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::Fortran,
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn output(text: &str) -> SourceParseOutput {
        FortranTestDriveParser
            .parse_with_context_output(
                document("test/main.f90", text),
                &ParserProjectContext::default(),
            )
            .expect("parse")
    }

    fn fixture(path: &str) -> SourceParseOutput {
        let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        FortranTestDriveParser
            .parse_with_context_output(document(path, &text), &ParserProjectContext::default())
            .expect("parse")
    }

    fn anchor_units(parsed: &SourceParseOutput) -> Vec<&CodeUnit> {
        parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::FortranTestDriveSubroutine)
            .collect()
    }

    fn standing_kinds_only(parsed: &SourceParseOutput, anchors: usize) {
        let kinds = unknown_kinds(parsed);
        assert_eq!(kinds.len(), anchors * 2);
        assert!(kinds.iter().all(|kind| matches!(
            *kind,
            "testdrive_module_binding" | "test_registration_unproven"
        )));
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<&str> {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .filter_map(|fact| {
                fact.assumptions
                    .iter()
                    .find(|assumption| assumption.starts_with("fortran_unknown_kind="))
                    .map(|assumption| &assumption["fortran_unknown_kind=".len()..])
            })
            .collect()
    }

    const FIXTURE_ROOT: &str = "src/fixtures/fortran/release/v0_2";

    #[test]
    fn admitted_module_wrapped_tests_anchor_including_extra_dummies() {
        let parsed = output(CANONICAL_SUITE);
        let anchors = anchor_units(&parsed);
        assert_eq!(anchors.len(), 2, "test_sizes and test_bounds anchor");
        assert!(parsed.report.diagnostics.is_empty());
        standing_kinds_only(&parsed, 2);
        // The collect subroutine is not a test despite registering them.
        assert!(anchors
            .iter()
            .all(|unit| unit.language == Language::Fortran));
        // Module unit plus two anchors, and each anchor nests under it.
        assert_eq!(parsed.report.units.len(), 3);
        assert!(parsed
            .report
            .ir_edges
            .iter()
            .all(|edge| edge.label == crate::core::model::IrEdgeLabel::Contains));
        assert_eq!(
            parsed
                .report
                .semantic_facts
                .iter()
                .filter(|fact| fact.kind == SemanticFactKind::Symbol
                    && fact
                        .target
                        .as_ref()
                        .is_some_and(|target| target.as_str() == FORTRAN_TEST_TARGET))
                .count(),
            2
        );
        // Each anchor carries the two standing residuals.
        assert_eq!(
            parsed
                .report
                .semantic_facts
                .iter()
                .filter(|fact| fact.kind == SemanticFactKind::Unknown)
                .count(),
            4
        );
    }

    #[test]
    fn admitted_top_level_tests_with_their_own_use_anchor() {
        let parsed = output(TOP_LEVEL_TESTS);
        assert_eq!(anchor_units(&parsed).len(), 3);
        standing_kinds_only(&parsed, 3);
    }

    #[test]
    fn case_insensitive_spellings_anchor() {
        let parsed = output(
            "MODULE Suite\nUSE Testdrive\nIMPLICIT NONE\nCONTAINS\n\
             SUBROUTINE Test_Upper(ERROR)\nTYPE(ERROR_TYPE), ALLOCATABLE, INTENT(OUT) :: ERROR\n\
             END SUBROUTINE Test_Upper\nEND MODULE Suite\n",
        );
        assert_eq!(anchor_units(&parsed).len(), 1);
    }

    #[test]
    fn a_subroutine_without_the_error_interface_never_anchors() {
        let parsed = output(
            "module m\n  use testdrive\ncontains\n\
             subroutine test_plain(error)\n  integer, intent(out) :: error\nend subroutine test_plain\n\
             subroutine test_none()\nend subroutine test_none\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn a_function_or_polymorphic_or_extra_attribute_shape_never_anchors() {
        let parsed = output(
            "module m\n  use testdrive\ncontains\n\
             function test_fn(error) result(ok)\n  type(error_type), allocatable, intent(out) :: error\n  logical :: ok\nend function test_fn\n\
             subroutine test_class(error)\n  class(error_type), allocatable, intent(out) :: error\nend subroutine test_class\n\
             subroutine test_optional(error)\n  type(error_type), allocatable, intent(out), optional :: error\nend subroutine test_optional\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn without_a_proven_use_nothing_anchors_and_identity_stays_unproven() {
        let parsed = output(
            "module m\n  use testdrive, only : new_unittest, unittest_type\ncontains\n\
             subroutine test_missing(error)\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_missing\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert_eq!(unknown_kinds(&parsed), vec!["testdrive_use_not_proven"]);
        let rendered = format!("{:?}", parsed.report.semantic_facts);
        assert!(rendered.contains("affected_claim=fortran_testdrive_identity"));
    }

    #[test]
    fn a_renamed_error_type_binding_never_proves_the_import() {
        let parsed = output(
            "module m\n  use testdrive, only : err => error_type\ncontains\n\
             subroutine test_renamed(err)\n  type(error_type), allocatable, intent(out) :: err\nend subroutine test_renamed\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert_eq!(unknown_kinds(&parsed), vec!["testdrive_use_not_proven"]);
    }

    #[test]
    fn an_intrinsic_use_statement_never_proves_the_import() {
        let parsed = output(
            "module m\n  use, intrinsic :: testdrive\ncontains\n\
             subroutine test_intrinsic(error)\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_intrinsic\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert_eq!(unknown_kinds(&parsed), vec!["testdrive_use_not_proven"]);
    }

    #[test]
    fn internal_procedures_and_type_bound_sections_never_anchor() {
        let parsed = output(
            "module m\n  use testdrive\n  implicit none\n  type :: suite_t\n    integer :: n\n\
             contains\n    procedure :: test_bound => impl_bound\n  end type suite_t\ncontains\n\
             subroutine impl_bound(self)\n  class(suite_t) :: self\nend subroutine impl_bound\n\
             subroutine runner()\n\
             contains\n\
             subroutine test_internal(error)\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_internal\n\
             end subroutine runner\n\
             end module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn interface_bodies_never_anchor() {
        let parsed = output(
            "module m\n  use testdrive\n  implicit none\n  interface\n\
             subroutine test_iface(error)\n  import :: error_type\n\
             type(error_type), allocatable, intent(out) :: error\nend subroutine test_iface\n\
             end interface\nend module m\n",
        );
        assert!(anchor_units(&parsed).is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn continuation_split_statements_and_semicolons_still_anchor() {
        let parsed = output(
            "module m\n  use testdrive; implicit none\ncontains\n\
             subroutine test_spl&\n                 &it(error)\n\
             type(error_type), intent(out), &\n    allocatable :: error\n\
             end subroutine test_spl&\n                 &it\nend module m\n",
        );
        assert_eq!(anchor_units(&parsed).len(), 1);
    }

    #[test]
    fn module_accessibility_does_not_refuse_the_canonical_pattern() {
        // test-drive's README keeps tests module-private behind a public
        // collector; the anchor is deliberately accessibility-agnostic.
        let parsed = output(
            "module m\n  use testdrive\n  implicit none\n  private\n\
             public :: collect\ncontains\n\
             subroutine test_private(error)\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_private\n\
             subroutine collect(testsuite)\n  type(unittest_type), allocatable, intent(out) :: testsuite(:)\nend subroutine collect\n\
             end module m\n",
        );
        assert_eq!(anchor_units(&parsed).len(), 1);
    }

    #[test]
    fn hostile_constructs_abstain_whole_file_with_typed_refusals() {
        let nested =
            output("module outer\ncontains\nmodule inner\nend module inner\nend module outer\n");
        assert!(anchor_units(&nested).is_empty());
        assert_eq!(unknown_kinds(&nested), vec!["unadmitted_fortran_construct"]);
        assert!(!nested.report.diagnostics.is_empty());

        let submodule = output("submodule (parent) child\nend submodule child\n");
        assert_eq!(
            unknown_kinds(&submodule),
            vec!["unadmitted_fortran_construct"]
        );

        let include = output(
            "module m\n  use testdrive\ncontains\nsubroutine test_i(error)\n  include 'helpers.f90'\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_i\nend module m\n",
        );
        assert_eq!(unknown_kinds(&include), vec!["include_statement"]);

        let fixed = output("      SUBROUTINE TEST_F(A)\n     1REAL A\n      END\n");
        assert_eq!(unknown_kinds(&fixed), vec!["fixed_form_signature"]);

        let pre = output("#define N 3\nmodule m\nend module m\n");
        assert_eq!(unknown_kinds(&pre), vec!["preprocessor_directive"]);
    }

    #[test]
    fn fixture_corpus_matches_the_admitted_and_refused_sets() {
        let suite = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_exact_tests/test_drive_suite.f90"
        ));
        assert_eq!(anchor_units(&suite).len(), 3);
        standing_kinds_only(&suite, 3);

        let top = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_exact_tests/top_level_tests.f90"
        ));
        assert_eq!(anchor_units(&top).len(), 3);

        let lookalikes = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_lookalikes/lookalikes.f90"
        ));
        assert!(anchor_units(&lookalikes).is_empty());
        assert_eq!(unknown_kinds(&lookalikes), vec!["testdrive_use_not_proven"]);

        let nested_kinds = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_lookalikes/internal_and_typebound.f90"
        ));
        assert!(anchor_units(&nested_kinds).is_empty());
        assert!(unknown_kinds(&nested_kinds).is_empty());

        let missing = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_missing_use/missing_use.f90"
        ));
        assert!(anchor_units(&missing).is_empty());
        assert_eq!(unknown_kinds(&missing), vec!["testdrive_use_not_proven"]);

        let low = fixture(&format!(
            "{FIXTURE_ROOT}/testdrive_low_support/low_support.f90"
        ));
        assert_eq!(anchor_units(&low).len(), 2);

        for (name, kind) in [
            ("fixed_form_content.f90", "fixed_form_signature"),
            ("preprocessed_content.f90", "preprocessor_directive"),
            ("include_statement.f90", "include_statement"),
        ] {
            let degraded = fixture(&format!("{FIXTURE_ROOT}/testdrive_parse_degraded/{name}"));
            assert!(anchor_units(&degraded).is_empty(), "{name}");
            assert_eq!(unknown_kinds(&degraded), vec![kind], "{name}");
            assert!(!degraded.report.diagnostics.is_empty(), "{name}");
        }
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(CANONICAL_SUITE);
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn registry_declares_each_kind_once_and_only_identity_blocks() {
        let mut kinds = FORTRAN_OBLIGATION_REGISTRY
            .iter()
            .map(|entry| entry.kind)
            .collect::<Vec<_>>();
        kinds.sort_unstable();
        let count = kinds.len();
        kinds.dedup();
        assert_eq!(kinds.len(), count, "every registry kind is unique");
        for emitted in [
            "testdrive_use_not_proven",
            "testdrive_module_binding",
            "test_registration_unproven",
            "preprocessor_directive",
            "include_statement",
            "fixed_form_signature",
            "unadmitted_fortran_construct",
            "parser_depth_limit",
            "source_byte_limit",
            "parser_resource_limit",
        ] {
            assert!(registry_entry(emitted).is_ok(), "{emitted} is registered");
        }
        for entry in FORTRAN_OBLIGATION_REGISTRY {
            assert_eq!(
                entry.blocks_family_claim,
                entry.kind == "testdrive_use_not_proven",
                "only the identity obligation may block"
            );
            assert!(!entry.standing || !entry.blocks_family_claim);
        }
    }

    #[test]
    fn unit_limit_is_bounded_and_plus_one_is_a_typed_unknown() {
        let mut text = String::new();
        for index in 0..=MAX_UNITS {
            text.push_str(&format!(
                "subroutine test_{index}(error)\nuse testdrive\ntype(error_type), allocatable, intent(out) :: error\nend subroutine\n"
            ));
        }
        let parsed = output(&text);
        assert_eq!(anchor_units(&parsed).len(), MAX_UNITS);
        assert!(unknown_kinds(&parsed).contains(&"parser_resource_limit"));
    }

    #[test]
    fn parsing_is_deterministic() {
        assert_eq!(output(CANONICAL_SUITE), output(CANONICAL_SUITE));
    }

    #[test]
    fn no_repository_identifier_reaches_a_fact_or_unit_id() {
        let parsed = output(&CANONICAL_SUITE.replace("test_sizes", "test_zsecretname"));
        let rendered = format!(
            "{:?}\n{:?}",
            parsed.report.semantic_facts, parsed.report.units
        );
        assert!(!rendered.to_ascii_lowercase().contains("zsecretname"));
        assert!(!rendered.to_ascii_lowercase().contains("test_suite"));
    }

    #[test]
    fn rejects_fixed_form_paths_config_files_and_other_languages_at_boundary() {
        let cases = [
            ("src/legacy.f", Language::Fortran),
            ("fpm.toml", Language::FortranConfig),
            ("src/main.py", Language::Fortran),
            ("test/main.F90", Language::Fortran),
        ];
        for (path, language) in cases {
            let mut source = document(path, "module m\nend module m\n");
            source.language = language;
            assert_eq!(
                FortranTestDriveParser
                    .parse_with_context_output(source, &ParserProjectContext::default()),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
    }

    // -- ADR-0058 differential: retained legacy parser vs Tree-sitter reader ----

    fn legacy_output(path: &str, text: &str) -> SourceParseOutput {
        parse_output_with(document(path, text), |text, tokens| {
            Ok(legacy_parse_tokens(text, tokens))
        })
        .expect("legacy parse")
    }

    fn digest(parsed: &SourceParseOutput) -> String {
        let anchors = anchor_units(parsed)
            .iter()
            .map(|unit| (unit.range.start_byte, unit.range.end_byte))
            .collect::<Vec<_>>();
        format!(
            "anchors={anchors:?} unknowns={:?} diagnostics={}",
            unknown_kinds(parsed),
            parsed.report.diagnostics.len()
        )
    }

    #[test]
    fn differential_every_fixture_source_matches_the_legacy_parser() {
        let mut compared = 0;
        let mut stack = vec![std::path::PathBuf::from(FIXTURE_ROOT)];
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(&directory).expect("fixture directory") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.to_str().is_some_and(is_free_form_fortran_path) {
                    let text = std::fs::read_to_string(&path).expect("fixture text");
                    let path = path.to_str().expect("utf8 path");
                    let legacy = legacy_output(path, &text);
                    let current = parse_output(document(path, &text)).expect("parse");
                    assert!(
                        legacy == current,
                        "{path}: legacy {} != tree-sitter {}",
                        digest(&legacy),
                        digest(&current)
                    );
                    compared += 1;
                }
            }
        }
        assert_eq!(compared, 9, "every committed Fortran fixture source");
    }

    const TEST_SUB: &str = "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n";

    fn in_module(use_line: &str, body: &str) -> String {
        format!("module m\n  {use_line}\n  implicit none\ncontains\n{body}end module m\n")
    }

    fn top_level(use_line: &str, declarations: &str, body: &str) -> String {
        format!(
            "subroutine test_x(error)\n  {use_line}\n{declarations}{body}end subroutine test_x\n"
        )
    }

    /// Inputs both parsers must read identically: every anchor variation, every
    /// refusal class, and near-misses. Nothing here is a syntax error inside a
    /// region the legacy parser skipped, which is the declared divergence.
    fn branch_corpus() -> Vec<(String, String)> {
        let mut corpus: Vec<(String, String)> = Vec::new();
        let mut add = |name: &str, text: String| corpus.push((name.to_string(), text));
        let decl = "  type(error_type), allocatable, intent(out) :: error\n";

        // Admitted shapes and `use` proofs.
        add("canonical", CANONICAL_SUITE.to_string());
        add("top_level_tests", TOP_LEVEL_TESTS.to_string());
        add("empty", String::new());
        add("blank_and_comments", "\n! only a comment\n\n".to_string());
        for (name, use_line) in [
            ("use_plain", "use testdrive"),
            ("use_colon", "use :: testdrive"),
            ("use_only_bare", "use testdrive, only : error_type"),
            (
                "use_only_many",
                "use testdrive, only : check, error_type, new_unittest",
            ),
            ("use_only_without_error_type", "use testdrive, only : check"),
            ("use_only_empty", "use testdrive, only :"),
            (
                "use_only_rename_target",
                "use testdrive, only : e => error_type",
            ),
            (
                "use_only_rename_and_bare",
                "use testdrive, only : error_type, e => error_type",
            ),
            (
                "use_only_rename_other",
                "use testdrive, only : error_type, c => check",
            ),
            (
                "use_only_operator",
                "use testdrive, only : operator(+), error_type",
            ),
            ("use_rename_no_only", "use testdrive, e => error_type"),
            ("use_intrinsic", "use, intrinsic :: testdrive"),
            ("use_non_intrinsic", "use, non_intrinsic :: testdrive"),
            ("use_other_module", "use other_mod"),
            ("use_other_module_only", "use other_mod, only : error_type"),
            ("use_uppercase", "USE TESTDRIVE, ONLY : ERROR_TYPE"),
            ("use_two_statements", "use other_mod\n  use testdrive"),
        ] {
            add(&format!("module_{name}"), in_module(use_line, TEST_SUB));
            add(&format!("own_{name}"), top_level(use_line, decl, ""));
        }
        add(
            "own_use_inside_module_subroutine",
            in_module(
                "implicit none",
                "  subroutine test_x(error)\n    use testdrive\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n",
            ),
        );
        add(
            "module_use_skipped_when_own_use_present",
            in_module("use other_mod", "  subroutine test_x(error)\n    use testdrive, only : error_type\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );

        // Prefixes, casing, spacing, and name shapes.
        for prefix in [
            "pure ",
            "elemental ",
            "recursive ",
            "impure ",
            "pure elemental ",
            "impure elemental recursive ",
        ] {
            add(
                &format!("prefix_{}", prefix.trim().replace(' ', "_")),
                in_module(
                    "use testdrive",
                    &format!(
                        "  {prefix}subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"
                    ),
                ),
            );
        }
        add(
            "uppercase_everything",
            "MODULE M\n  USE TESTDRIVE\nCONTAINS\n  SUBROUTINE TEST_UP(ERROR)\n    TYPE(ERROR_TYPE), ALLOCATABLE, INTENT(OUT) :: ERROR\n  END SUBROUTINE TEST_UP\nEND MODULE M\n".to_string(),
        );
        add(
            "mixed_case_name",
            in_module("use testdrive", "  subroutine Test_Mixed(Error)\n    type(Error_Type), allocatable, intent(OUT) :: error\n  end subroutine\n"),
        );
        add(
            "end_without_qualifier",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end\n"),
        );
        add(
            "end_with_trailing_comment",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x ! done\n"),
        );
        add(
            "end_followed_by_semicolon_statement",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x; \n"),
        );
        add(
            "spaces_in_intent",
            in_module("use testdrive", "  subroutine test_x(error)\n    type( error_type ) , allocatable , intent( out ) :: error\n  end subroutine test_x\n"),
        );
        add(
            "attribute_order_swapped",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), intent(out), allocatable :: error\n  end subroutine test_x\n"),
        );
        add(
            "no_allocatable",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "extra_dummies_declared_after",
            in_module("use testdrive", "  subroutine test_x(error, first, last)\n    type(error_type), allocatable, intent(out) :: error\n    integer, intent(in) :: first, last\n  end subroutine test_x\n"),
        );
        add(
            "first_dummy_declared_last",
            in_module("use testdrive", "  subroutine test_x(error, first)\n    integer, intent(in) :: first\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "dummy_declared_in_entity_list",
            in_module("use testdrive", "  subroutine test_x(error, other)\n    type(error_type), allocatable, intent(out) :: error, other\n  end subroutine test_x\n"),
        );
        add(
            "sized_error_entity",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error(:)\n  end subroutine test_x\n"),
        );
        add(
            "old_style_declaration",
            in_module(
                "use testdrive",
                "  subroutine test_x(error)\n    type(error_type) error\n  end subroutine test_x\n",
            ),
        );
        add(
            "only_public_statement_names_error",
            in_module("use testdrive", "  subroutine test_x(error)\n    public :: error\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "implicit_none_and_other_decls_first",
            in_module("use testdrive", "  subroutine test_x(error)\n    implicit none\n    integer :: i, j(3)\n    real, parameter :: x = 1.0\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );

        // Near-misses that must never anchor.
        for (name, body) in [
            ("class_dummy", "  subroutine test_x(error)\n    class(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
            ("intent_inout", "  subroutine test_x(error)\n    type(error_type), intent(inout) :: error\n  end subroutine test_x\n"),
            ("intent_in", "  subroutine test_x(error)\n    type(error_type), intent(in) :: error\n  end subroutine test_x\n"),
            ("no_intent", "  subroutine test_x(error)\n    type(error_type), allocatable :: error\n  end subroutine test_x\n"),
            ("extra_target_attribute", "  subroutine test_x(error)\n    type(error_type), intent(out), target :: error\n  end subroutine test_x\n"),
            ("extra_optional_attribute", "  subroutine test_x(error)\n    type(error_type), intent(out), optional :: error\n  end subroutine test_x\n"),
            ("pointer_attribute", "  subroutine test_x(error)\n    type(error_type), intent(out), pointer :: error\n  end subroutine test_x\n"),
            ("other_type", "  subroutine test_x(error)\n    type(other_type), intent(out) :: error\n  end subroutine test_x\n"),
            ("integer_dummy", "  subroutine test_x(error)\n    integer, intent(out) :: error\n  end subroutine test_x\n"),
            ("declared_twice", "  subroutine test_x(error)\n    type(error_type), intent(out) :: error\n    integer :: error\n  end subroutine test_x\n"),
            ("not_declared", "  subroutine test_x(error)\n    integer :: i\n  end subroutine test_x\n"),
            ("no_dummies", "  subroutine test_x()\n  end subroutine test_x\n"),
            ("no_parens", "  subroutine test_x\n  end subroutine test_x\n"),
            ("not_test_prefix", "  subroutine check_x(error)\n    type(error_type), intent(out) :: error\n  end subroutine check_x\n"),
            ("bare_test_name", "  subroutine test(error)\n    type(error_type), intent(out) :: error\n  end subroutine test\n"),
            ("test_prefix_uppercase_only", "  subroutine TEST_ONLY(error)\n    type(error_type), intent(out) :: error\n  end subroutine\n"),
            ("function_named_test", "  function test_f(error)\n    type(error_type), intent(out) :: error\n  end function test_f\n"),
            ("function_with_result", "  function test_f(error) result(r)\n    type(error_type), intent(out) :: error\n    integer :: r\n  end function test_f\n"),
            ("pure_function", "  pure function g(x) result(r)\n    integer, intent(in) :: x\n    integer :: r\n  end function g\n"),
            ("bind_c_suffix", "  subroutine test_x(error) bind(c)\n    type(error_type), intent(out) :: error\n  end subroutine test_x\n"),
        ] {
            add(&format!("near_miss_{name}"), in_module("use testdrive", body));
        }

        // Scoping units that never anchor.
        add(
            "internal_procedure_in_subroutine",
            in_module("use testdrive", "  subroutine outer(x)\n    integer :: x\n  contains\n    subroutine test_inner(error)\n      type(error_type), intent(out) :: error\n    end subroutine test_inner\n  end subroutine outer\n"),
        );
        add(
            "internal_procedure_after_anchor",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    call helper()\n  contains\n    subroutine helper()\n    end subroutine helper\n  end subroutine test_x\n"),
        );
        add(
            "program_internal_procedures",
            "program p\n  use testdrive\n  implicit none\n  call s()\ncontains\n  subroutine test_p(error)\n    type(error_type), intent(out) :: error\n  end subroutine test_p\n  function f() result(r)\n    integer :: r\n  end function f\nend program p\n".to_string(),
        );
        add(
            "program_then_anchor",
            format!("program p\n  call s()\nend program p\n\nsubroutine test_y(error)\n  use testdrive\n{decl}end subroutine test_y\n"),
        );
        add(
            "interface_body_in_module",
            "module m\n  use testdrive\n  interface\n    subroutine test_i(error)\n      type(error_type), intent(out) :: error\n    end subroutine test_i\n  end interface\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string(),
        );
        add(
            "abstract_interface_in_module",
            "module m\n  use testdrive\n  abstract interface\n    subroutine cb(error)\n      type(error_type), intent(out) :: error\n    end subroutine cb\n  end interface\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string(),
        );
        add(
            "interface_in_subroutine_spec",
            "subroutine test_x(error)\n  use testdrive\n  interface\n    subroutine cb(i)\n      integer :: i\n    end subroutine cb\n  end interface\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_x\n".to_string(),
        );
        add(
            "derived_type_with_type_bound_procedures",
            "module m\n  use testdrive\n  type :: t\n    integer :: x\n  contains\n    procedure :: p\n  end type t\n  type u\n    real :: y\n  end type\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string(),
        );
        add(
            "type_definition_inside_subroutine_spec",
            "subroutine test_x(error)\n  use testdrive\n  type :: local\n    integer :: i\n  end type local\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_x\n".to_string(),
        );
        add(
            "module_without_contains",
            "module m\n  use testdrive\n  integer :: x\nend module m\n".to_string(),
        );
        add(
            "module_with_contains_only",
            "module m\ncontains\nend module m\n".to_string(),
        );
        add(
            "two_modules_one_with_use",
            "module a\n  use testdrive\ncontains\n  subroutine test_a(error)\n    type(error_type), intent(out) :: error\n  end subroutine test_a\nend module a\nmodule b\ncontains\n  subroutine test_b(error)\n    type(error_type), intent(out) :: error\n  end subroutine test_b\nend module b\n".to_string(),
        );
        add(
            "anchor_and_missing_use_together",
            "module m\ncontains\n  subroutine test_x(error)\n    type(error_type), intent(out) :: error\n  end subroutine test_x\nend module m\nsubroutine test_y(error)\n  use testdrive\n  type(error_type), intent(out) :: error\nend subroutine test_y\n".to_string(),
        );
        add(
            "module_with_function_and_subroutines",
            in_module("use testdrive", "  function f(x) result(y)\n    integer :: x, y\n    y = x\n  end function f\n  subroutine helper()\n  end subroutine helper\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "mismatched_module_name_after_qualifier_is_accepted",
            "module m\n  use testdrive\ncontains\n  subroutine test_x(error)\n    type(error_type), intent(out) :: error\n  end subroutine other_name\nend module different\n".to_string(),
        );

        // Bodies: constructs, labels, continuation, literals.
        let body = |statements: &str| -> String {
            in_module("use testdrive", &format!("  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    integer :: i\n{statements}  end subroutine test_x\n"))
        };
        for (name, statements) in [
            ("call_statements", "    call check(error, 1 + 2 == 3)\n    if (allocated(error)) return\n"),
            ("block_if_else", "    if (i > 0) then\n      call a()\n    else\n      call b()\n    end if\n"),
            ("one_line_if", "    if (i > 0) call a()\n"),
            ("do_loop", "    do i = 1, 3\n      cycle\n    end do\n"),
            ("do_while", "    do while (i < 3)\n      i = i + 1\n    end do\n"),
            ("select_case", "    select case (i)\n    case (1)\n      call a()\n    case default\n      call b()\n    end select\n"),
            ("where_construct", "    where (a > 0)\n      b = 1\n    elsewhere\n      b = 2\n    end where\n"),
            ("where_statement", "    where (a > 0) b = 1\n"),
            ("forall_construct", "    forall (i = 1:3)\n      a(i) = i\n    end forall\n"),
            ("forall_statement", "    forall (i = 1:3) a(i) = i\n"),
            ("associate_construct", "    associate (z => i)\n      call a(z)\n    end associate\n"),
            ("block_construct", "    block\n      integer :: j\n      j = 1\n    end block\n"),
            ("nested_constructs", "    do i = 1, 3\n      if (i > 1) then\n        select case (i)\n        case (2)\n          call a()\n        end select\n      end if\n    end do\n"),
            ("statement_label_continue", "    goto 10\n10  continue\n"),
            ("semicolons", "    i = 1; i = 2; call a()\n"),
            ("string_with_bang_and_quotes", "    s = 'it''s ! not a comment'\n    t = \"say \"\"hi\"\"\"\n"),
            ("continuation_between_tokens", "    i = 1 + &\n      2\n"),
            ("continuation_with_comment", "    i = 1 + & ! comment\n      & 2\n"),
            ("array_constructor_continuation", "    a = [ &\n      1, &\n      2 &\n    ]\n"),
            ("string_continuation", "    s = 'abc&\n         &def'\n"),
            ("inline_comment", "    i = 1 ! trailing\n"),
            ("print_statement", "    print *, 'hello', i\n"),
            ("write_statement", "    write (*, '(a)') 'x'\n"),
            ("format_label", "    write (*, 100) i\n100 format (i5)\n"),
            ("stop_statement", "    stop 1\n"),
            ("allocate_statements", "    allocate (a(10))\n    deallocate (a)\n"),
            ("function_call_expression", "    i = f(1, g(2, 3))\n"),
            ("derived_type_component", "    x%y = 1\n"),
            ("array_section", "    b(1:3) = a(2:4)\n"),
            ("logical_operators", "    if (a .and. .not. b .or. c) i = 1\n"),
            ("exponent_numbers", "    x = 1.0e-3 + 2.5d0\n"),
            ("character_concat", "    s = 'a' // 'b'\n"),
        ] {
            add(&format!("body_{name}"), body(statements));
        }
        // Executable statements before a use proof, and a declaration after.
        add(
            "continuation_split_subroutine_name",
            in_module("use testdrive", "  subroutine test_va&\n    &lid(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_va&\n    &lid\n"),
        );
        add(
            "continuation_split_use_item",
            in_module("use testdrive, only : &\n    error_&\n    &type", TEST_SUB),
        );
        add(
            "continuation_split_attribute_list",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), &\n      allocatable, &\n      intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "continuation_split_keyword",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(o&\n    &ut) :: error\n  end subroutine test_x\n"),
        );
        add(
            "semicolon_separated_declarations",
            in_module("use testdrive", "  subroutine test_x(error)\n    integer :: i; type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n"),
        );
        add(
            "crlf_line_endings",
            "module m\r\n  use testdrive\r\ncontains\r\n  subroutine test_x(error)\r\n    type(error_type), allocatable, intent(out) :: error\r\n  end subroutine test_x\r\nend module m\r\n".to_string(),
        );
        add(
            "tabs_for_indent",
            "module m\n\tuse testdrive\ncontains\n\tsubroutine test_x(error)\n\t\ttype(error_type), allocatable, intent(out) :: error\n\tend subroutine test_x\nend module m\n".to_string(),
        );
        add(
            "no_trailing_newline",
            "module m\n  use testdrive\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m".to_string(),
        );
        add(
            "unterminated_string_degrades_only",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    s = 'unterminated\n    i = 1\n  end subroutine test_x\n"),
        );
        add(
            "unterminated_string_in_comment_free_statement",
            in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    print *, \"open\n  end subroutine test_x\n"),
        );

        // Refusal classes.
        add(
            "refuse_preprocessor_top",
            "#ifdef X\nmodule m\nend module m\n#endif\n".to_string(),
        );
        add(
            "refuse_preprocessor_inside",
            "module m\n  #include \"c.f90\"\nend module m\n".to_string(),
        );
        add(
            "refuse_preprocessor_indented",
            "module m\n    #define N 3\nend module m\n".to_string(),
        );
        add(
            "refuse_fixed_form_column_six",
            "      subroutine test_f(a)\n     1real a\n      end\n".to_string(),
        );
        add(
            "refuse_fixed_form_star_comment",
            "* comment\nsubroutine s()\nend subroutine s\n".to_string(),
        );
        add(
            "refuse_fixed_form_after_free_form",
            "module m\ncontains\nend module m\n      x\n     +y\n".to_string(),
        );
        add(
            "refuse_lone_ampersand",
            "module m\n  x = 1 & 2\nend module m\n".to_string(),
        );
        add(
            "refuse_include_in_module_spec",
            "module m\n  use testdrive\n  include 'x.f90'\ncontains\nend module m\n".to_string(),
        );
        add(
            "refuse_include_in_subroutine_spec",
            top_level(
                "use testdrive",
                "  include 'h.f90'\n  type(error_type), allocatable, intent(out) :: error\n",
                "",
            ),
        );
        add(
            "refuse_include_in_subroutine_body",
            top_level("use testdrive", decl, "  call a()\n  include 'h.f90'\n"),
        );
        add(
            "refuse_include_inside_if_body",
            top_level(
                "use testdrive",
                decl,
                "  if (a) then\n    include 'h.f90'\n  end if\n",
            ),
        );
        add(
            "refuse_include_in_program_body",
            "program p\n  include 'h.f90'\nend program p\n".to_string(),
        );
        add(
            "refuse_include_at_top_level",
            "include 'h.f90'\nmodule m\nend module m\n".to_string(),
        );
        add(
            "refuse_include_between_module_subprograms",
            "module m\ncontains\n  include 'h.f90'\nend module m\n".to_string(),
        );
        add(
            "refuse_include_in_program_internal_procedures",
            "program p\ncontains\n  include 'h.f90'\nend program p\n".to_string(),
        );
        add(
            "refuse_include_in_interface",
            "module m\n  interface\n    include 'h.f90'\n  end interface\nend module m\n"
                .to_string(),
        );
        add(
            "refuse_include_in_derived_type",
            "module m\n  type t\n    include 'h.f90'\n  end type t\nend module m\n".to_string(),
        );
        add(
            "refuse_include_after_continue_statement",
            top_level("use testdrive", decl, "  i = 1; include 'h.f90'\n"),
        );
        add(
            "near_miss_include_as_variable",
            top_level("use testdrive", decl, "  include = 3\n"),
        );
        add(
            "near_miss_include_not_followed_by_string",
            top_level("use testdrive", decl, "  include(1) = 3\n"),
        );
        add(
            "refuse_submodule",
            "submodule (parent) child\nend submodule child\n".to_string(),
        );
        add(
            "refuse_block_data",
            "block data\nend block data\n".to_string(),
        );
        add(
            "refuse_top_level_use",
            "use testdrive\nsubroutine test_x(error)\nend subroutine\n".to_string(),
        );
        add("refuse_top_level_statement", "x = 1\n".to_string());
        add("refuse_top_level_number", "10 continue\n".to_string());
        add(
            "refuse_nested_module",
            "module outer\ncontains\nmodule inner\nend module inner\nend module outer\n"
                .to_string(),
        );
        add(
            "refuse_module_procedure_top",
            "module procedure p\nend procedure\n".to_string(),
        );
        add(
            "refuse_module_subroutine_prefix",
            "module subroutine s()\nend subroutine s\n".to_string(),
        );
        add(
            "refuse_module_subroutine_prefix_in_module",
            "module m\ncontains\n  module subroutine s()\n  end subroutine s\nend module m\n"
                .to_string(),
        );
        add(
            "refuse_typed_function_in_module",
            "module m\ncontains\n  integer function f(x)\n    integer :: x\n    f = x\n  end function f\nend module m\n".to_string(),
        );
        add(
            "refuse_typed_function_top",
            "real function f(x)\n  real :: x\n  f = x\nend function f\n".to_string(),
        );
        add(
            "refuse_prefix_then_typed_function",
            "module m\ncontains\n  pure real function f(x)\n    real, intent(in) :: x\n    f = x\n  end function f\nend module m\n".to_string(),
        );
        add(
            "refuse_prefix_without_procedure",
            "pure\nsubroutine s()\nend subroutine s\n".to_string(),
        );
        add(
            "refuse_statement_after_module_contains",
            "module m\ncontains\n  use testdrive\nend module m\n".to_string(),
        );
        add(
            "refuse_type_after_module_contains",
            "module m\ncontains\n  type t\n  end type t\nend module m\n".to_string(),
        );
        add(
            "refuse_statement_after_program_contains",
            "program p\ncontains\n  integer :: i\nend program p\n".to_string(),
        );
        add(
            "refuse_label_in_module_spec",
            "module m\n10 continue\ncontains\nend module m\n".to_string(),
        );
        add(
            "refuse_label_in_program_spec",
            "program p\n10 continue\nend program p\n".to_string(),
        );
        add(
            "near_miss_label_after_program_body_started",
            "program p\n  if (a) then\n  end if\n10 continue\nend program p\n".to_string(),
        );
        add(
            "refuse_labeled_do_in_subroutine",
            top_level("use testdrive", decl, "  do 10 i = 1, 3\n10  continue\n"),
        );
        add(
            "refuse_labeled_do_after_declarations_in_program",
            "program p\n  integer :: i\n  do 20 i = 1, 3\n20 continue\nend program p\n".to_string(),
        );
        add(
            "near_miss_do_with_number_in_name",
            top_level("use testdrive", decl, "  do i = 1, 10\n  end do\n"),
        );
        add(
            "refuse_end_subroutine_closing_function",
            "module m\ncontains\n  subroutine s()\n  end function s\nend module m\n".to_string(),
        );
        add(
            "refuse_end_function_closing_subroutine_top",
            "subroutine s()\nend function s\n".to_string(),
        );
        add(
            "refuse_end_with_wrong_name",
            "subroutine s()\nend wrong\n".to_string(),
        );
        add(
            "refuse_end_module_with_wrong_name_only",
            "module m\nend wrong\n".to_string(),
        );
        add(
            "refuse_end_program_with_name_only",
            "program p\nend p\n".to_string(),
        );
        add(
            "refuse_end_interface_with_wrong_word",
            "module m\n  interface\n  end wrong\nend module m\n".to_string(),
        );
        add(
            "refuse_end_type_with_wrong_word",
            "module m\n  type t\n  end wrong\nend module m\n".to_string(),
        );
        add(
            "accept_end_interface_with_generic_name",
            "module m\n  interface g\n    module procedure a\n  end interface g\nend module m\n"
                .to_string(),
        );
        add(
            "accept_end_type_with_own_name",
            "module m\n  type t\n    integer :: x\n  end type t\nend module m\n".to_string(),
        );
        add(
            "refuse_missing_end_module",
            "module m\ncontains\n  subroutine s()\n  end subroutine s\n".to_string(),
        );
        add(
            "refuse_missing_end_subroutine",
            "subroutine s()\n  x = 1\n".to_string(),
        );
        add(
            "refuse_missing_end_program",
            "program p\n  x = 1\n".to_string(),
        );
        add(
            "refuse_unclosed_if_in_body",
            "subroutine s()\n  if (a) then\nend subroutine s\n".to_string(),
        );
        add(
            "refuse_missing_end_interface",
            "module m\n  interface\n    subroutine s()\n    end subroutine s\nend module m\n"
                .to_string(),
        );
        add(
            "refuse_stray_end_after_unit",
            "subroutine s()\nend subroutine s\nend subroutine s\n".to_string(),
        );
        add(
            "refuse_end_inside_module_spec_wrong_kind",
            "module m\n  x = 1\nend subroutine\n".to_string(),
        );
        add(
            "refuse_text_before_module",
            "garbage\nmodule m\nend module m\n".to_string(),
        );
        add(
            "refuse_text_after_module",
            "module m\nend module m\ngarbage here\n".to_string(),
        );
        add("refuse_text_only", "just some words\n".to_string());

        // Further constructs read identically (second differential round).
        add("extra_comment_after_contains", "module m\n  use testdrive\ncontains ! c\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_two_contains_like_text", "module m\n  use testdrive\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n  subroutine test_y(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_y\nend module m\n".to_string());
        add("extra_program_with_spec", "program p\n  use testdrive\n  implicit none\n  integer :: i\n  type(error_type) :: e\n  i = 1\n  print *, i\nend program p\n".to_string());
        add(
            "extra_program_end_with_name",
            "program p\n  stop\nend program p\n".to_string(),
        );
        add(
            "extra_program_no_name_end",
            "program p\n  stop\nend\n".to_string(),
        );
        add("extra_interface_operator", "module m\n  use testdrive\n  interface operator(+)\n    module procedure add\n  end interface\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_generic_interface_functions", "module m\n  use testdrive\n  interface g\n    module procedure a, b\n  end interface g\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_private_public_stmts", "module m\n  use testdrive\n  private\n  public :: test_x\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_parameter_decl_in_module", "module m\n  use testdrive\n  integer, parameter :: n = 3\n  real :: x(n) = 0.0\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_data_and_common", "module m\n  use testdrive\n  integer :: a\n  common /blk/ a\n  data a /1/\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\nend module m\n".to_string());
        add("extra_select_type_in_body", "subroutine s(x)\n  class(*) :: x\n  select type (x)\n  type is (integer)\n    call a()\n  class default\n    call b()\n  end select\nend subroutine s\n".to_string());
        add("extra_function_with_internal", "module m\ncontains\n  function f(x) result(y)\n    integer :: x, y\n    y = g(x)\n  contains\n    function g(z)\n      integer :: z, g\n      g = z\n    end function g\n  end function f\nend module m\n".to_string());
        add("extra_subroutine_with_semicolon_end", "subroutine test_x(error)\n  use testdrive\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_x ; \n".to_string());
        add("extra_two_statements_after_end_line", "subroutine test_x(error)\n  use testdrive\n  type(error_type), allocatable, intent(out) :: error\nend subroutine test_x; subroutine test_y(error); end subroutine\n".to_string());
        add(
            "extra_interface_with_semicolons",
            "module m\n  interface; subroutine a(); end subroutine; end interface\nend module m\n"
                .to_string(),
        );
        add("extra_where_elsewhere_nested", "subroutine s()\n  where (a > 0)\n    where (b > 0)\n      c = 1\n    end where\n  end where\nend subroutine s\n".to_string());
        add("extra_forall_block", "subroutine s()\n  forall (i = 1:3, j = 1:3)\n    a(i, j) = 0\n  end forall\nend subroutine s\n".to_string());
        add("extra_omp_comment_directive", "subroutine s()\n  !$omp parallel do\n  do i = 1, 3\n  end do\n  !$omp end parallel do\nend subroutine s\n".to_string());
        add(
            "extra_lone_quote",
            "subroutine s()\n  x = '\n  y = 1\nend subroutine s\n".to_string(),
        );
        add("extra_lone_quote_in_module_test", "module m\n  use testdrive\ncontains\n  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    x = '\n  end subroutine test_x\nend module m\n".to_string());
        add(
            "extra_dbl_colon_in_array_constructor",
            "subroutine s()\n  a = [integer :: 1, 2]\nend subroutine s\n".to_string(),
        );
        add("extra_type_guard_class_is", "subroutine s(x)\n  class(*) :: x\n  select type (x)\n  class is (integer)\n  end select\nend subroutine s\n".to_string());
        add("extra_block_with_declarations", "subroutine s()\n  block\n    integer :: j\n    do j = 1, 2\n    end do\n  end block\nend subroutine s\n".to_string());
        add(
            "extra_pointer_assignment",
            "subroutine s()\n  p => t\n  q(1:2) => t\nend subroutine s\n".to_string(),
        );
        add(
            "extra_do_concurrent",
            "subroutine s()\n  do concurrent (i = 1:3)\n    a(i) = 0\n  end do\nend subroutine s\n"
                .to_string(),
        );
        add(
            "extra_coarray",
            "subroutine s()\n  integer :: a[*]\n  sync all\nend subroutine s\n".to_string(),
        );
        add(
            "extra_submodule_procedure",
            "submodule (m) sm\ncontains\n  module procedure p\n  end procedure\nend submodule sm\n"
                .to_string(),
        );
        add(
            "extra_entry_statement",
            "subroutine s()\n  entry t()\nend subroutine s\n".to_string(),
        );
        add(
            "extra_implicit_typing_decl",
            "subroutine s(a)\n  implicit real (a-h, o-z)\n  dimension a(3)\nend subroutine s\n"
                .to_string(),
        );
        add(
            "extra_save_equivalence",
            "subroutine s()\n  real :: a, b\n  equivalence (a, b)\n  save a\nend subroutine s\n"
                .to_string(),
        );
        add(
            "extra_namelist_stmt",
            "subroutine s()\n  namelist /nl/ a, b\nend subroutine s\n".to_string(),
        );
        add("extra_interface_with_import", "module m\n  interface\n    subroutine a(x)\n      import :: t\n      type(t) :: x\n    end subroutine a\n  end interface\nend module m\n".to_string());
        add(
            "extra_procedure_pointer_decl",
            "module m\n  procedure(a), pointer :: p\nend module m\n".to_string(),
        );
        add("extra_function_prefix_result_bind", "module m\ncontains\n  function f() result(r) bind(c)\n    integer :: r\n  end function f\nend module m\n".to_string());
        add(
            "extra_recursive_subroutine_top",
            "recursive subroutine s()\nend subroutine s\n".to_string(),
        );
        add("extra_elemental_function_module", "module m\ncontains\n  elemental function f(x)\n    real :: f, x\n  end function f\nend module m\n".to_string());
        add(
            "extra_class_default_in_subroutine_spec",
            "subroutine s(x)\n  class(t), intent(in) :: x\nend subroutine s\n".to_string(),
        );
        add("extra_long_name_with_continuation_everywhere", "module m\n  use testdrive\ncontains\n  subroutine test_&\n    &long_&\n    &name(error)\n    type(error_type), allocatable, &\n      intent(out) :: error\n  end &\n    subroutine test_long_name\nend module m\n".to_string());
        add(
            "extra_end_do_label_loop",
            "subroutine s()\n  do i = 1, 3\n  end do\nend subroutine s\n".to_string(),
        );
        add("extra_if_then_on_continuation", "subroutine s()\n  if (a .and. &\n      b) then\n    c = 1\n  end if\nend subroutine s\n".to_string());
        add("extra_select_case_nested_in_do", "subroutine s()\n  do i = 1, 3\n    select case (i)\n    case (1)\n      cycle\n    end select\n  end do\nend subroutine s\n".to_string());

        // Depth limit: 256 nested constructs read, the 257th refuses.
        for (count, name) in [
            (255usize, "depth_255"),
            (256, "depth_256"),
            (257, "depth_257"),
            (300, "depth_300"),
        ] {
            let opens = "if (a) then\n".repeat(count);
            let closes = "end if\n".repeat(count);
            add(
                &format!("module_subroutine_{name}"),
                in_module("use testdrive", &format!("  subroutine test_x(error)\n    type(error_type), intent(out) :: error\n{opens}{closes}  end subroutine test_x\n")),
            );
            add(
                &format!("program_{name}"),
                format!("program p\n{opens}{closes}end program p\n"),
            );
            let dos = "do i = 1, 2\n".repeat(count);
            let end_dos = "end do\n".repeat(count);
            add(
                &format!("top_level_do_{name}"),
                format!("subroutine s()\n{dos}{end_dos}end subroutine s\n"),
            );
        }
        corpus
    }

    #[test]
    fn differential_branch_corpus_matches_the_legacy_parser() {
        let corpus = branch_corpus();
        eprintln!("branch corpus sources: {}", corpus.len());
        assert!(corpus.len() > 150, "branch corpus size {}", corpus.len());
        let mut failures = Vec::new();
        for (name, text) in &corpus {
            let legacy = legacy_output("test/main.f90", text);
            let current = output(text);
            if legacy != current {
                failures.push(format!(
                    "{name}: legacy {} != tree-sitter {}",
                    digest(&legacy),
                    digest(&current)
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// Reads every `.f90` file below `REPOGRAMMAR_FORTRAN_DIFFERENTIAL_CORPUS`
    /// with both parsers, catching legacy panics, and prints one line per
    /// differing file. Differences are reported, never failed on.
    #[test]
    #[ignore = "reads REPOGRAMMAR_FORTRAN_DIFFERENTIAL_CORPUS, an external read-only directory"]
    fn differential_external_corpus_report() {
        let Ok(root) = std::env::var("REPOGRAMMAR_FORTRAN_DIFFERENTIAL_CORPUS") else {
            eprintln!("REPOGRAMMAR_FORTRAN_DIFFERENTIAL_CORPUS is not set");
            return;
        };
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.to_str().is_some_and(is_free_form_fortran_path) {
                    files.push(path);
                }
            }
        }
        files.sort();
        let (mut compared, mut differing, mut legacy_panics) = (0usize, 0usize, 0usize);
        let (mut both_refuse, mut both_read) = (0usize, 0usize);
        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let name = path.to_string_lossy().into_owned();
            let legacy = std::panic::catch_unwind(|| legacy_output("test/main.f90", &text));
            let current = output(&text);
            compared += 1;
            match legacy {
                Err(_) => {
                    legacy_panics += 1;
                    eprintln!("DIFF legacy-panic {name} new: {}", digest(&current));
                }
                Ok(legacy) if legacy != current => {
                    differing += 1;
                    eprintln!(
                        "DIFF {name}\n  legacy: {}\n  new:    {}",
                        digest(&legacy),
                        digest(&current)
                    );
                }
                Ok(_) => {
                    if unknown_kinds(&current).iter().any(|kind| {
                        !matches!(
                            *kind,
                            "testdrive_module_binding" | "test_registration_unproven"
                        )
                    }) {
                        both_refuse += 1;
                    } else {
                        both_read += 1;
                    }
                }
            }
        }
        eprintln!(
            "SUMMARY compared={compared} differing={differing} legacy_panics={legacy_panics} identical_refused={both_refuse} identical_read={both_read}"
        );
    }

    /// Inputs where the Tree-sitter reader and the legacy parser legitimately
    /// differ: `(name, text, anchors, refusal, class)`. `refusal` is the typed
    /// unknown the new reader emits, if any.
    fn divergence_corpus() -> Vec<(
        &'static str,
        String,
        usize,
        Option<&'static str>,
        &'static str,
    )> {
        let sub = |statements: &str| {
            in_module(
                "use testdrive",
                &format!("  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n    integer :: i\n{statements}  end subroutine test_x\n{TEST_SUB}"),
            )
        };
        let unadmitted = Some("unadmitted_fortran_construct");
        vec![
            // Legacy misparses the grammar reads correctly (unknown_removed).
            (
                "else_if_chain",
                sub("    if (i > 0) then\n      i = 1\n    else if (i < 0) then\n      i = 2\n    else\n      i = 3\n    end if\n"),
                2,
                None,
                "legacy counted `else if (...) then` as a block opener",
            ),
            (
                "named_do",
                sub("    outer: do i = 1, 3\n      exit outer\n    end do outer\n"),
                2,
                None,
                "legacy did not see a named construct's opener",
            ),
            (
                "named_if",
                sub("    chk: if (i > 0) then\n      i = 1\n    end if chk\n"),
                2,
                None,
                "legacy did not see a named construct's opener",
            ),
            (
                "variable_named_end",
                sub("    end = 3\n"),
                2,
                None,
                "legacy closed the unit at an assignment to a variable named end",
            ),
            (
                "derived_type_with_attributes",
                "module m\n  use testdrive\n  type, private, bind(c) :: a\n    integer :: i\n  end type a\ncontains\n".to_string() + TEST_SUB + "end module m\n",
                1,
                None,
                "legacy read `type, attr :: name` as an ordinary statement",
            ),
            (
                "typed_function_in_interface",
                "module m\n  use testdrive\n  interface\n    type(c_ptr) function acc_malloc(bytes) bind(C)\n      integer :: bytes\n    end function\n  end interface\ncontains\n".to_string() + TEST_SUB + "end module m\n",
                1,
                None,
                "legacy did not see a typed function statement as a procedure opener",
            ),
            (
                "enum_block",
                "module m\n  use testdrive\n  enum, bind(c)\n    enumerator :: a = 1\n  end enum\ncontains\n".to_string() + TEST_SUB + "end module m\n",
                1,
                None,
                "legacy did not see an enum block's opener",
            ),
            // Syntax the grammar rejects where the legacy parser skipped the
            // statement unread (unknown_added).
            (
                "syntax_error_in_skipped_body",
                sub("    x = = 1\n"),
                0,
                unadmitted,
                "syntax_error_in_skipped_region",
            ),
            (
                "end_followed_by_the_unit_name_only",
                in_module("use testdrive", "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end test_x\n"),
                0,
                unadmitted,
                "syntax_error_in_skipped_region",
            ),
            (
                "alternate_return_dummy",
                in_module("use testdrive", "  subroutine test_x(error, *)\n    type(error_type), intent(out) :: error\n  end subroutine test_x\n"),
                0,
                unadmitted,
                "grammar gap: alternate return",
            ),
            (
                "non_ascii_identifier",
                "module m\n  integer :: caf\u{e9}\nend module m\n".to_string(),
                0,
                unadmitted,
                "syntax_error_in_skipped_region",
            ),
        ]
    }

    #[test]
    fn tree_sitter_reader_pins_the_inputs_the_legacy_parser_misread_or_skipped() {
        for (name, text, anchors, refusal, _) in divergence_corpus() {
            let current = output(&text);
            assert_eq!(anchor_units(&current).len(), anchors, "{name}");
            let kinds = unknown_kinds(&current);
            match refusal {
                Some(kind) => assert_eq!(kinds, vec![kind], "{name}"),
                None => assert!(
                    kinds.iter().all(|kind| matches!(
                        *kind,
                        "testdrive_module_binding" | "test_registration_unproven"
                    )),
                    "{name}: {kinds:?}"
                ),
            }
        }
    }

    #[test]
    fn differential_declared_divergences_really_differ_from_the_legacy_parser() {
        for (name, text, _, _, class) in divergence_corpus() {
            assert_ne!(
                legacy_output("test/main.f90", &text),
                output(&text),
                "{name} ({class})"
            );
        }
    }
}
