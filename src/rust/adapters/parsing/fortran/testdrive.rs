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
        None => parse_tokens(document.text, &tokens)?,
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

    const TEST_SUB: &str = "  subroutine test_x(error)\n    type(error_type), allocatable, intent(out) :: error\n  end subroutine test_x\n";

    fn in_module(use_line: &str, body: &str) -> String {
        format!("module m\n  {use_line}\n  implicit none\ncontains\n{body}end module m\n")
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
}
