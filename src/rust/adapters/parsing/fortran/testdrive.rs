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
use super::free_form::{self, ident_is, lower, LexAbstention, Token, TokenKind};
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
        None => parse_tokens(document.text, &tokens),
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

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
}

fn parse_tokens(text: &str, tokens: &[Token]) -> FileParse {
    let mut parser = Parser {
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

impl<'a> Parser<'a> {
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
}
