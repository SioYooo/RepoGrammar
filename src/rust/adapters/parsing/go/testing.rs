//! Bounded Go testing frontend for the ADR-0050 admitted shapes.
//!
//! Only `*_test.go` bytes reach this parser, because the filename is part of
//! the anchor's meaning: `go test` compiles only those files as tests, so a
//! function with a test's exact signature in an ordinary file is not a test.
//! Every other `.go` byte stays inventory and is never decoded.
//!
//! Nothing here starts a process, spawns a descendant, consults a
//! `GOPACKAGESDRIVER`, touches cgo, resolves a module graph, or evaluates a
//! build constraint. ADR-0021's containment obligation is bound to routes that
//! execute the Go toolchain and is unreached rather than waived: nothing runs.
//!
//! Declarations come from the pinned Tree-sitter Go grammar (ADR-0058), with
//! the ADR-0050 declared subset applied over the tree: the subset's lexer stays
//! the lexical gate, and every ADR-0050 refusal is read off the tree and the
//! token stream. An anchor therefore answers "is this a package-level
//! declaration with this shape", never "does this text appear at brace depth
//! zero", which is the question the `go.testing.test_function` family claim
//! rests on and what discharges ADR-0021's no-text-matching requirement.
//!
//! The frontend has two outcomes and no third. Either the whole file parses
//! against the declared subset and its admitted declarations are reported, or
//! the file is refused and contributes no anchor at all. A tree that needed
//! error recovery (`ERROR` or `MISSING` nodes) refuses the file, because a
//! recovered anchor cannot be told apart from a real one. Input is untrusted, so the input size, the
//! nesting depth, and the emitted unit and fact counts are all bounded.
//!
//! Every typed `UNKNOWN` this frontend can emit is declared exactly once in
//! [`GO_OBLIGATION_REGISTRY`], the lane's ADR-0020 gate 4 source-semantic
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
use tree_sitter::{Node, Parser};

pub const GO_ANCHOR_ENGINE: &str = "repogrammar-go-testing-parser";
pub const GO_ANCHOR_METHOD: &str = "bounded_go_test_declaration_v2";

/// Fixed support target for the one admitted exact anchor. The token denotes
/// the top-level Go test-function contract, not a symbol named
/// `testing.Test` (no such standard-library API exists).
pub const GO_TESTING_TEST_TARGET: &str = "go.testing.test_function";

const TESTING_IMPORT_PATH: &str = "testing";
const CGO_IMPORT_PATH: &str = "C";
const MAX_FUNCTION_UNITS: usize = 4_096;
const MAX_FACTS: usize = 16_384;
/// Bound on bracket nesting anywhere in the parse. Input is untrusted, and a
/// file of nothing but open braces must abstain rather than exhaust the stack.
const MAX_PARSE_DEPTH: usize = 1_024;

/// True for the only filenames this frontend may read.
pub fn is_go_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with("_test.go") && name.len() > "_test.go".len()
}

#[derive(Debug, Default)]
pub struct GoTestingParser;

impl SourceParser for GoTestingParser {
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

/// One source-semantic obligation the admitted Go family claim rests on.
///
/// This is the lane's ADR-0020 gate 4 registry. Every typed `UNKNOWN` the
/// testing frontend emits is declared here exactly once, with the claim it
/// scopes, its reason code, whether an unmet obligation blocks the family
/// claim, and the provider-fallback policy that names what could discharge it.
/// `application/family.rs` stays the authoritative claim-impact classifier;
/// this record must agree with it, and the registry tests below pin the
/// recorded split to the rules that classifier implements so the two cannot
/// drift silently.
pub(crate) struct GoObligation {
    /// Stable affected-claim token (`affected_claim=` assumption).
    pub(crate) affected_claim: &'static str,
    /// Stable kind token (`go_unknown_kind=` assumption).
    pub(crate) kind: &'static str,
    /// Stable protocol reason code (the fact target).
    pub(crate) reason: UnknownReasonCode,
    /// A standing obligation rides on every admitted anchor because the
    /// bounded parse can never discharge it; a triggered one fires only on
    /// the construct that leaves it unmet.
    pub(crate) standing: bool,
    /// Recorded claim impact of an unmet obligation. Must agree with
    /// `go_unknown_reason_blocks_family_membership` and
    /// `go_unknown_is_non_blocking_family_subclaim` in
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
/// `go.testing.test_function` family claim.
///
/// Obligations split three ways. The two identity obligations block the family
/// claim when unmet: a dot or blank `testing` import binds no qualified name,
/// so no test signature in that file resolves, exactly as ADR-0021 D3 and
/// ADR-0041 D3 state. The build-constraint obligation is a recorded non-
/// blocking subclaim: the file's declarations are what they are whether or not
/// the target platform compiles the file, and `application/family.rs` already
/// classifies it that way. Everything else is either a bounded-input refusal
/// or a residual the bounded parse can never discharge -- test execution
/// semantics and the cross-file package/type graph -- so those ride along as
/// non-blocking unknowns rather than being guessed away.
///
/// Fallback tokens: `none_source_decidable` means the file's own source
/// decides the fact permanently and no provider can change it.
/// `build_environment_provider_not_integrated` means GOOS/GOARCH/toolchain and
/// build-tag selection could settle the claim but the ADR-0021 worker is not
/// integrated. `source_inside_declared_subset_only` means the parse can only
/// read source the ADR-0050 subset admits and never recovers past a refusal.
/// `source_only_shape_widening_needs_adr` means the admitted shape is a
/// decision and widening it needs a superseding ADR, not a provider.
/// `bounded_input_refusal` means the untrusted-input bounds or the malformed
/// input refused the file and no provider is involved.
/// `runtime_observation_not_integrated` means a bounded runtime observation
/// would settle the claim but none is authorized or integrated.
/// `go_semantic_worker_not_integrated` means the ADR-0021 pinned
/// standard-library worker is the mechanism that could discharge the
/// obligation, and it remains future provider work.
pub(crate) const GO_OBLIGATION_REGISTRY: &[GoObligation] = &[
    // Fallback: decidable from the file itself and permanent -- a dot import
    // puts T in file scope with no qualifier, so no test signature resolves.
    // No provider can change what the file binds.
    GoObligation {
        affected_claim: "go_test_declaration",
        kind: "dot_testing_import",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: false,
        blocks_family_claim: true,
        note: "a dot testing import binds no qualified name, so no test signature in this file resolves",
        fallback: "none_source_decidable",
    },
    // Fallback: decidable from the file itself and permanent -- a blank import
    // binds no name at all.
    GoObligation {
        affected_claim: "go_test_declaration",
        kind: "blank_testing_import",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: false,
        blocks_family_claim: true,
        note: "a blank testing import binds no name at all, so no test signature in this file resolves",
        fallback: "none_source_decidable",
    },
    // Fallback: build constraints are evaluated by the toolchain against a
    // selected GOOS/GOARCH/toolchain/tag environment. The ADR-0021 worker
    // route could carry that environment; none is integrated.
    GoObligation {
        affected_claim: "go_build_constraint",
        kind: "build_constraint_not_evaluated",
        reason: UnknownReasonCode::BuildVariantAmbiguity,
        standing: false,
        blocks_family_claim: false,
        note: "Go build constraints are not evaluated, so this file may be excluded on the target platform",
        fallback: "build_environment_provider_not_integrated",
    },
    // Fallback: recoverable only by source inside the declared ADR-0050
    // subset. The parse never recovers past the boundary.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "unadmitted_construct",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Go source used a construct outside the declared subset, so the file was refused and read no further",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: generic signatures are valid Go that this frontend
    // deliberately does not parse. Admitting them is a superseding decision,
    // not a provider question.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "generic_signature",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a function declares type parameters, which the declared subset refuses, so the file was not parsed",
        fallback: "source_only_shape_widening_needs_adr",
    },
    // Fallback: a bracket in a method receiver could be an array type or an
    // instantiated generic type, and telling them apart needs type
    // information. The ADR-0021 worker could discharge it; none is integrated.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "undecidable_receiver_type",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a method receiver type contains a bracket, which could be an array or an instantiated generic, so the file was not parsed",
        fallback: "go_semantic_worker_not_integrated",
    },
    // Fallback: cgo semantics need the ADR-0021 worker with its separate cgo
    // gates. Refusing the file is a subset decision the worker does not remove.
    GoObligation {
        affected_claim: "go_cgo_boundary",
        kind: "cgo_import",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "the file imports C, so cgo processing applies and the file was refused rather than read",
        fallback: "go_semantic_worker_not_integrated",
    },
    // Fallback: malformed source. The token stream after an open literal or
    // comment was read as literal text, so every later boundary is unproven.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "unterminated_literal",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a Go string, rune, or comment is left open, so the token stream after it is unproven",
        fallback: "bounded_input_refusal",
    },
    // Fallback: malformed source. An escape the Go scanner rejects means the
    // file does not compile, so it was refused.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "invalid_escape",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a Go string or rune uses an escape the language does not define, so the file was refused",
        fallback: "bounded_input_refusal",
    },
    // Fallback: malformed source. Brackets or braces that never close or do
    // not match leave every later boundary unproven.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "unbalanced_braces",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Go brackets or braces do not balance, so the declaration extents in this file are unproven",
        fallback: "bounded_input_refusal",
    },
    // Fallback: malformed source. Two imports binding the same local name do
    // not compile, so the file was refused.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "duplicate_import_binding",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "two imports bind the same local name, which Go rejects, so the file was refused",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted nesting. No provider is
    // involved.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "parser_depth_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Go source nested past the bounded parse depth, so the file was refused",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted size. No provider is
    // involved.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "source_byte_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Go source exceeded the bounded input-byte limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: bounded-input refusal on untrusted unit or fact counts. No
    // provider is involved.
    GoObligation {
        affected_claim: "go_test_parse",
        kind: "parser_resource_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "Go parser exceeded the bounded declaration or fact limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: whether a declared test runs -- and passes, skips, or runs in
    // parallel -- is runtime state. A bounded runtime observation would settle
    // it; none is authorized or integrated.
    GoObligation {
        affected_claim: "go_test_execution",
        kind: "go_test_execution",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: true,
        blocks_family_claim: false,
        note: "test outcomes, skips, parallelism, and t.Run subtest identity are runtime behavior the declaration does not determine",
        fallback: "runtime_observation_not_integrated",
    },
    // Fallback: the file's own package identity, the test binary's package
    // graph, and cross-file type identity need the ADR-0021 pinned
    // standard-library worker. No worker slot exists or is registered.
    GoObligation {
        affected_claim: "go_package_identity",
        kind: "go_package_identity",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: true,
        blocks_family_claim: false,
        note: "no module, package graph, or cross-file identity is resolved, so the declaring package's composition stays unproven",
        fallback: "go_semantic_worker_not_integrated",
    },
];

fn registry_entry(kind: &str) -> Result<&'static GoObligation, ParseError> {
    GO_OBLIGATION_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind)
        .ok_or_else(|| ParseError::Internal(format!("unregistered Go obligation kind {kind}")))
}

fn standing_obligations() -> impl Iterator<Item = &'static GoObligation> {
    GO_OBLIGATION_REGISTRY.iter().filter(|entry| entry.standing)
}

fn tree_sitter_go_file(text: &str) -> Result<Result<ParsedGoFile, Refusal>, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load Go grammar: {error}")))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter Go parse failed".to_string()))?;
    Ok(parse_go_file(text, tree.root_node()))
}

pub(crate) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Go || !is_go_test_path(document.path) {
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
            "unit:{}#go_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Go,
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

    let file = match tree_sitter_go_file(document.text)? {
        Ok(file) => file,
        Err(refusal) => return refused(units, facts, module, document.path, refusal),
    };

    if file.has_header_build_directive {
        // Recorded, not evaluated: the declarations are what they are whether
        // or not the target platform compiles the file.
        facts.push(obligation_fact(
            &module,
            registry_entry("build_constraint_not_evaluated")?,
            full_range.clone(),
        )?);
    }

    match &file.testing_binding {
        TestingBinding::Dot | TestingBinding::Blank => {
            // Neither form yields a resolvable parameter spelling, so the
            // file's test declarations are unproven rather than absent.
            let kind = if matches!(file.testing_binding, TestingBinding::Dot) {
                "dot_testing_import"
            } else {
                "blank_testing_import"
            };
            facts.push(obligation_fact(
                &module,
                registry_entry(kind)?,
                full_range.clone(),
            )?);
        }
        TestingBinding::Absent | TestingBinding::Qualified(_) => {}
    }

    let mut limit_hit = false;
    for (ordinal, declaration) in file.functions.iter().enumerate() {
        if ordinal >= MAX_FUNCTION_UNITS || facts.len() >= MAX_FACTS.saturating_sub(3) {
            limit_hit = true;
            break;
        }
        let family = admitted_anchor(declaration, &file.testing_binding);
        let kind = if family.is_some() {
            CodeUnitKind::GoTestFunction
        } else {
            CodeUnitKind::GoFunction
        };
        let range =
            SourceRange::new(declaration.start, declaration.end).map_err(ParseError::Internal)?;
        // The trailing component is the declaration's ordinal, not its name.
        // A unit id reaches MCP output, and ADR-0020 gate 7 excludes
        // high-cardinality repository identifiers from those surfaces; the
        // path and byte range already address the declaration uniquely. This
        // matches every other bounded lane: R and SQL carry a bounded ordinal,
        // and Ada, MATLAB, Delphi, PHP, Swift, Ruby, and Fortran carry
        // nothing. The name spelling was inherited from the ADR-0041 scanner,
        // which no ADR requires.
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}:{}",
                document.path,
                kind.as_str(),
                declaration.start,
                declaration.end,
                ordinal
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Go,
            kind,
            range: range.clone(),
            provenance: provenance.clone(),
        };
        if let Some(family) = family {
            facts.push(anchor_fact(&unit, family)?);
            // The standing obligations ride on every admitted anchor: the
            // bounded parse proves the declaration shape and nothing beyond
            // it, so execution semantics and cross-file identity stay typed
            // `UNKNOWN` on the anchor itself rather than being guessed away.
            // They are recorded residuals and must never block the family
            // claim.
            for entry in standing_obligations() {
                debug_assert!(
                    !entry.blocks_family_claim,
                    "a standing Go obligation is a recorded residual and must not block"
                );
                facts.push(obligation_fact(&unit, entry, range.clone())?);
            }
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

    // A file that parsed is not degraded: the parser consumed the whole file
    // or it would have refused it.
    finish(units, facts, Vec::new())
}

/// How the file binds the `testing` package, which decides the anchor
/// spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestingBinding {
    /// Not imported at all: no test function can be declared here.
    Absent,
    /// Imported under this local name, so the anchor type is `*<name>.T`.
    Qualified(String),
    /// `import . "testing"` puts `T` in file scope with no qualifier.
    Dot,
    /// `import _ "testing"` binds no name at all.
    Blank,
}

/// How a refusal reaches the operator.
///
/// Every field is fixed vocabulary. The offending token, identifier, literal,
/// and line never appear, because a refusal reaches `index --json`, `unknowns`,
/// and the MCP readiness payloads.
struct RefusalRecord {
    kind: &'static str,
    degraded: Option<&'static str>,
}

fn refusal_record(refusal: Refusal) -> RefusalRecord {
    match refusal {
        Refusal::UnterminatedLiteral => RefusalRecord {
            kind: "unterminated_literal",
            degraded: Some(
                "a Go string, rune, or comment is left open, so every declaration after it was read as literal text",
            ),
        },
        Refusal::InvalidEscape => RefusalRecord {
            kind: "invalid_escape",
            degraded: Some(
                "a Go string or rune uses an escape the language does not define, so the file was refused",
            ),
        },
        Refusal::UnbalancedBraces => RefusalRecord {
            kind: "unbalanced_braces",
            degraded: Some(
                "Go brackets or braces do not balance, so the declaration extents in this file are unproven",
            ),
        },
        Refusal::UnadmittedConstruct => RefusalRecord {
            kind: "unadmitted_construct",
            degraded: None,
        },
        Refusal::GenericSignature => RefusalRecord {
            kind: "generic_signature",
            degraded: None,
        },
        Refusal::UndecidableReceiverType => RefusalRecord {
            kind: "undecidable_receiver_type",
            degraded: None,
        },
        Refusal::CgoImport => RefusalRecord {
            kind: "cgo_import",
            degraded: None,
        },
        Refusal::DuplicateImportBinding => RefusalRecord {
            kind: "duplicate_import_binding",
            degraded: Some(
                "two imports bind the same local name, which Go rejects, so the file was refused",
            ),
        },
        Refusal::DepthLimit => RefusalRecord {
            kind: "parser_depth_limit",
            degraded: None,
        },
    }
}

fn refused(
    units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
    module: CodeUnit,
    path: &str,
    refusal: Refusal,
) -> Result<SourceParseOutput, ParseError> {
    let record = refusal_record(refusal);
    let entry = registry_entry(record.kind)?;
    facts.push(obligation_fact(&module, entry, module.range.clone())?);
    let diagnostics = record
        .degraded
        .map(|message| vec![degraded(path, message)])
        .unwrap_or_default();
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
// Refusals
// ---------------------------------------------------------------------------

/// Why the file was refused.
///
/// Each class is decidable from source alone. None is ever recovered from: the
/// parser reports the boundary and stops, so no declaration after it is guessed
/// at, and the file contributes no anchor at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// A string, rune, or comment is left open.
    UnterminatedLiteral,
    /// An escape the Go scanner rejects.
    InvalidEscape,
    /// A bracketed region never closes, or a closer does not match its opener.
    UnbalancedBraces,
    /// A byte or construct outside the declared subset of ADR-0050: a stray
    /// semicolon inside a signature, a top-level token that begins no
    /// declaration, a raw-string import path, or a missing package clause.
    UnadmittedConstruct,
    /// A function declares type parameters.
    GenericSignature,
    /// A method receiver contains a bracket: array or instantiated generic,
    /// undecidable without type information.
    UndecidableReceiverType,
    /// The file imports C, so cgo processing applies.
    CgoImport,
    /// Two imports bind the same local name.
    DuplicateImportBinding,
    /// Nesting past [`MAX_PARSE_DEPTH`].
    DepthLimit,
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// A token of the declared Go subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Semi,
    Comma,
    Dot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Star,
    /// `++` and `--`, which end a line for semicolon insertion.
    IncDec,
    /// Every other Go operator or delimiter.
    Operator,
    Identifier,
    Keyword,
    IntLiteral,
    FloatLiteral,
    ImagLiteral,
    RuneLiteral,
    StringLiteral,
    RawStringLiteral,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The keywords whose presence at end of line triggers semicolon insertion.
fn keyword_inserts_semicolon(word: &str) -> bool {
    matches!(word, "break" | "continue" | "fallthrough" | "return")
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "break"
            | "case"
            | "chan"
            | "const"
            | "continue"
            | "default"
            | "defer"
            | "else"
            | "fallthrough"
            | "for"
            | "func"
            | "go"
            | "goto"
            | "if"
            | "import"
            | "interface"
            | "map"
            | "package"
            | "range"
            | "return"
            | "select"
            | "struct"
            | "switch"
            | "type"
            | "var"
    )
}

/// True when a newline after a token of this kind inserts a semicolon, per the
/// Go specification's rule.
fn inserts_semicolon(kind: TokenKind, text: &str) -> bool {
    match kind {
        TokenKind::Identifier
        | TokenKind::IntLiteral
        | TokenKind::FloatLiteral
        | TokenKind::ImagLiteral
        | TokenKind::RuneLiteral
        | TokenKind::StringLiteral
        | TokenKind::RawStringLiteral
        | TokenKind::RParen
        | TokenKind::RBracket
        | TokenKind::RBrace
        | TokenKind::IncDec => true,
        TokenKind::Keyword => keyword_inserts_semicolon(text),
        _ => false,
    }
}

/// Longest-match operator table. Every Go operator and delimiter is admitted
/// so that skipped bodies tokenize exactly; `#`, `$`, and every other byte
/// begin no token of the subset and refuse the file.
fn operator(bytes: &[u8], index: usize) -> Option<(TokenKind, usize)> {
    let rest = &bytes[index..];
    let table: &[(&[u8], TokenKind)] = &[
        (b"<<=", TokenKind::Operator),
        (b">>=", TokenKind::Operator),
        (b"&^=", TokenKind::Operator),
        (b"<<", TokenKind::Operator),
        (b">>", TokenKind::Operator),
        (b"&^", TokenKind::Operator),
        (b"...", TokenKind::Operator),
        (b"<-", TokenKind::Operator),
        (b":=", TokenKind::Operator),
        (b"&&", TokenKind::Operator),
        (b"||", TokenKind::Operator),
        (b"==", TokenKind::Operator),
        (b"!=", TokenKind::Operator),
        (b"<=", TokenKind::Operator),
        (b">=", TokenKind::Operator),
        (b"++", TokenKind::IncDec),
        (b"--", TokenKind::IncDec),
        (b"+=", TokenKind::Operator),
        (b"-=", TokenKind::Operator),
        (b"*=", TokenKind::Operator),
        (b"/=", TokenKind::Operator),
        (b"%=", TokenKind::Operator),
        (b"&=", TokenKind::Operator),
        (b"|=", TokenKind::Operator),
        (b"^=", TokenKind::Operator),
        (b"=", TokenKind::Operator),
        (b"+", TokenKind::Operator),
        (b"-", TokenKind::Operator),
        (b"*", TokenKind::Star),
        (b"/", TokenKind::Operator),
        (b"%", TokenKind::Operator),
        (b"&", TokenKind::Operator),
        (b"|", TokenKind::Operator),
        (b"^", TokenKind::Operator),
        (b"<", TokenKind::Operator),
        (b">", TokenKind::Operator),
        (b"!", TokenKind::Operator),
        (b"~", TokenKind::Operator),
        (b"(", TokenKind::LParen),
        (b")", TokenKind::RParen),
        (b"[", TokenKind::LBracket),
        (b"]", TokenKind::RBracket),
        (b"{", TokenKind::LBrace),
        (b"}", TokenKind::RBrace),
        (b",", TokenKind::Comma),
        (b";", TokenKind::Semi),
        (b".", TokenKind::Dot),
        (b":", TokenKind::Operator),
    ];
    for (spelling, kind) in table {
        if rest.starts_with(spelling) {
            return Some((*kind, spelling.len()));
        }
    }
    None
}

/// End of an interpreted string starting at `start`, with every escape
/// validated against the Go scanner's rule, or a refusal.
fn interpreted_string_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let Some(width) = escape_width(bytes, index) else {
                    return Err(Refusal::InvalidEscape);
                };
                index += width;
            }
            b'"' => return Ok(index + 1),
            b'\n' => return Err(Refusal::UnterminatedLiteral),
            _ => index += 1,
        }
    }
    Err(Refusal::UnterminatedLiteral)
}

/// Width of the escape starting at `index` (the backslash included), or `None`
/// when the Go scanner rejects it.
fn escape_width(bytes: &[u8], index: usize) -> Option<usize> {
    let after = bytes.get(index + 1).copied()?;
    match after {
        b'a' | b'b' | b'f' | b'n' | b'r' | b't' | b'v' | b'\\' | b'\'' | b'"' => Some(2),
        b'x' => {
            hex_at(bytes, index + 2, 2)?;
            Some(4)
        }
        b'u' => {
            hex_at(bytes, index + 2, 4)?;
            Some(6)
        }
        b'U' => {
            hex_at(bytes, index + 2, 8)?;
            Some(10)
        }
        b'0'..=b'7' => {
            // An octal byte value is exactly three octal digits.
            if bytes
                .get(index + 2..index + 4)
                .is_some_and(|pair| pair.iter().all(|byte| (b'0'..=b'7').contains(byte)))
            {
                Some(4)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn hex_at(bytes: &[u8], start: usize, count: usize) -> Option<()> {
    (0..count)
        .all(|offset| {
            bytes
                .get(start + offset)
                .is_some_and(|byte| byte.is_ascii_hexdigit())
        })
        .then_some(())
}

/// End of a raw string starting at `start`, or a refusal when it never closes.
fn raw_string_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'`' {
            return Ok(index + 1);
        }
        index += 1;
    }
    Err(Refusal::UnterminatedLiteral)
}

/// End of a rune literal starting at `start`, or a refusal. Escapes are
/// validated; an unescaped newline or end of file leaves the rune open.
fn rune_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let Some(width) = escape_width(bytes, index) else {
                    return Err(Refusal::InvalidEscape);
                };
                index += width;
            }
            b'\'' => return Ok(index + 1),
            b'\n' => return Err(Refusal::UnterminatedLiteral),
            _ => index += 1,
        }
    }
    Err(Refusal::UnterminatedLiteral)
}

/// A numeral's kind and validated end, per the Go spec's literal grammar.
fn numeral_end(bytes: &[u8], start: usize) -> Result<(usize, TokenKind), Refusal> {
    let (end, kind) = numeral_span(bytes, start)?;
    // A numeral may not run straight into an identifier or another dot.
    if bytes
        .get(end)
        .is_some_and(|byte| is_ident_continue(*byte) || !byte.is_ascii() || *byte == b'.')
    {
        return Err(Refusal::UnadmittedConstruct);
    }
    Ok((end, kind))
}

fn numeral_span(bytes: &[u8], start: usize) -> Result<(usize, TokenKind), Refusal> {
    if bytes[start] == b'0' && matches!(bytes.get(start + 1), Some(b'x') | Some(b'X')) {
        let mut index = start + 2;
        let int_run = index;
        index = digit_run_end(bytes, index, hex_digit)?;
        let int_digits = index > int_run;
        let mut is_float = false;
        let mut frac_digits = 0;
        if bytes.get(index) == Some(&b'.') {
            is_float = true;
            let frac_run = index + 1;
            index = digit_run_end(bytes, index + 1, hex_digit)?;
            frac_digits = index - frac_run;
        }
        if !int_digits && frac_digits == 0 {
            // `0x` and `0x.` are not numerals.
            return Err(Refusal::UnadmittedConstruct);
        }
        if matches!(bytes.get(index), Some(b'p') | Some(b'P')) {
            is_float = true;
            index += 1;
            if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
                index += 1;
            }
            let exponent = index;
            index = digit_run_end(bytes, index, |byte| byte.is_ascii_digit())?;
            if index == exponent {
                return Err(Refusal::UnadmittedConstruct);
            }
        } else if is_float {
            // A hex fraction requires a binary exponent.
            return Err(Refusal::UnadmittedConstruct);
        }
        Ok(imaginary_suffix(bytes, index, is_float))
    } else if bytes[start] == b'0' && matches!(bytes.get(start + 1), Some(b'b') | Some(b'B')) {
        let mut index = start + 2;
        let digits = index;
        index = digit_run_end(bytes, index, |byte| byte == b'0' || byte == b'1')?;
        if index == digits {
            return Err(Refusal::UnadmittedConstruct);
        }
        Ok(imaginary_suffix(bytes, index, false))
    } else if bytes[start] == b'0' && matches!(bytes.get(start + 1), Some(b'o') | Some(b'O')) {
        let mut index = start + 2;
        let digits = index;
        index = digit_run_end(bytes, index, |byte| (b'0'..=b'7').contains(&byte))?;
        if index == digits {
            return Err(Refusal::UnadmittedConstruct);
        }
        Ok(imaginary_suffix(bytes, index, false))
    } else {
        let mut index = start;
        let int_digits = index;
        index = digit_run_end(bytes, index, |byte| byte.is_ascii_digit())?;
        let has_int = index > int_digits;
        let mut is_float = false;
        if bytes.get(index) == Some(&b'.') {
            is_float = true;
            index += 1;
            index = digit_run_end(bytes, index, |byte| byte.is_ascii_digit())?;
        }
        if !has_int && !is_float {
            return Err(Refusal::UnadmittedConstruct);
        }
        if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
            is_float = true;
            index += 1;
            if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
                index += 1;
            }
            let exponent = index;
            index = digit_run_end(bytes, index, |byte| byte.is_ascii_digit())?;
            if index == exponent {
                return Err(Refusal::UnadmittedConstruct);
            }
        }
        if bytes[start] == b'0' && !is_float && index - start > 1 {
            // Legacy octal: every digit after the leading zero is octal.
            for byte in &bytes[start + 1..index] {
                if !(b'0'..=b'7').contains(byte) {
                    return Err(Refusal::UnadmittedConstruct);
                }
            }
        }
        Ok(imaginary_suffix(bytes, index, is_float))
    }
}

fn imaginary_suffix(bytes: &[u8], index: usize, is_float: bool) -> (usize, TokenKind) {
    if bytes.get(index) == Some(&b'i') {
        (index + 1, TokenKind::ImagLiteral)
    } else if is_float {
        (index, TokenKind::FloatLiteral)
    } else {
        (index, TokenKind::IntLiteral)
    }
}

fn hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// End of a digit-and-underscore run, refusing underscores that are not
/// between two digits.
fn digit_run_end(bytes: &[u8], start: usize, digit: fn(u8) -> bool) -> Result<usize, Refusal> {
    let mut index = start;
    let mut last_was_digit = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if digit(byte) {
            last_was_digit = true;
            index += 1;
        } else if byte == b'_' {
            if !last_was_digit {
                return Err(Refusal::UnadmittedConstruct);
            }
            last_was_digit = false;
            index += 1;
        } else {
            break;
        }
    }
    if !last_was_digit && index > start {
        return Err(Refusal::UnadmittedConstruct);
    }
    Ok(index)
}

/// True when a header comment carries a `//go:build` or legacy `// +build`
/// constraint directive.
fn comment_is_build_directive(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix("//go:build") {
        return rest.is_empty() || rest.starts_with([' ', '\t']);
    }
    text.starts_with("// +build")
}

/// Tokenize the whole file.
///
/// Newlines are kept only where Go inserts semicolons after them; every other
/// newline is whitespace. Comments are never tokens, but a line comment does
/// not shield the newline that follows it: Go inserts a semicolon after the
/// last token before the comment, and so does this lexer.
fn lex(text: &str) -> Result<(Vec<Token>, bool), Refusal> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut last: Option<Token> = None;
    let mut emitted_any = false;
    let mut has_header_build_directive = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        let kind = match byte {
            b' ' | b'\t' | b'\r' => {
                index += 1;
                continue;
            }
            b'\n' => {
                index += 1;
                if last.is_some_and(|token| {
                    inserts_semicolon(token.kind, &text[token.start..token.end])
                }) {
                    tokens.push(Token {
                        kind: TokenKind::Semi,
                        start,
                        end: index,
                    });
                    // A semicolon never triggers another insertion, so two
                    // blank lines produce one separator.
                    last = None;
                }
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                let mut end = index;
                while end < bytes.len() && bytes[end] != b'\n' {
                    end += 1;
                }
                if !emitted_any && comment_is_build_directive(&text[index..end]) {
                    has_header_build_directive = true;
                }
                index = end;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let mut end = index + 2;
                loop {
                    match bytes.get(end) {
                        None => return Err(Refusal::UnterminatedLiteral),
                        Some(b'*') if bytes.get(end + 1) == Some(&b'/') => {
                            end += 2;
                            break;
                        }
                        Some(_) => end += 1,
                    }
                }
                index = end;
                continue;
            }
            b'"' => {
                let end = interpreted_string_end(bytes, index)?;
                index = end;
                TokenKind::StringLiteral
            }
            b'`' => {
                let end = raw_string_end(bytes, index)?;
                index = end;
                TokenKind::RawStringLiteral
            }
            b'\'' => {
                let end = rune_end(bytes, index)?;
                index = end;
                TokenKind::RuneLiteral
            }
            b'.' if bytes
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_digit()) =>
            {
                let (end, kind) = numeral_end(bytes, index)?;
                index = end;
                kind
            }
            byte if is_ident_start(byte) => {
                index += 1;
                while index < bytes.len() && is_ident_continue(bytes[index]) {
                    index += 1;
                }
                let word = &text[start..index];
                if is_keyword(word) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Identifier
                }
            }
            byte if byte.is_ascii_digit() => {
                let (end, kind) = numeral_end(bytes, index)?;
                index = end;
                kind
            }
            _ => {
                let Some((kind, width)) = operator(bytes, index) else {
                    // Every remaining byte -- `?`, `$`, `#`, `@`, a stray
                    // symbol, or any non-ASCII byte in code position -- begins
                    // no token of the declared subset. Identifiers are ASCII
                    // in this subset, so non-ASCII identifier bytes refuse the
                    // file rather than being classified by a locale.
                    return Err(Refusal::UnadmittedConstruct);
                };
                index += width;
                kind
            }
        };
        let token = Token {
            kind,
            start,
            end: index,
        };
        last = Some(token);
        emitted_any = true;
        tokens.push(token);
    }
    Ok((tokens, has_header_build_directive))
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// One package-level function or method declaration.
struct FunctionDecl {
    name: String,
    is_method: bool,
    has_results: bool,
    /// `(qualifier, type name)` when the parameter list is exactly one
    /// optionally named `*qualifier.Type` parameter, as Go AST field-name
    /// semantics read it.
    pointer_parameter: Option<(String, String)>,
    start: usize,
    end: usize,
}

struct ParsedGoFile {
    testing_binding: TestingBinding,
    functions: Vec<FunctionDecl>,
    has_header_build_directive: bool,
}

/// The alias shape of one import specification.
enum ImportAlias<'a> {
    None,
    Named(&'a str),
    Dot,
}

/// The ADR-0050 declared-subset policy, read off a Tree-sitter Go tree
/// (ADR-0058).
///
/// The lexer still runs first and stays authoritative for the lexical subset
/// and the header build directive, so every lexical refusal is unchanged. The
/// grammar supplies the declaration structure, and the ADR-0050 parse refusals
/// are read off the tree and the token stream. As in a left-to-right parse, the
/// earliest refusal in source order wins; at one position a token-stream
/// refusal precedes a tree refusal. Any `ERROR` or `MISSING` node refuses the
/// file, so a recovered tree never yields an anchor. Error recovery decides
/// where such a node sits, so it refuses as `UnadmittedConstruct` only when no
/// token or structural refusal applies.
fn parse_go_file(text: &str, root: Node<'_>) -> Result<ParsedGoFile, Refusal> {
    let (tokens, has_header_build_directive) = lex(text)?;
    let mut refusals = token_refusals(&tokens);
    let mut testing_binding = TestingBinding::Absent;
    // Local names decidable from source: explicit aliases, and the local name
    // of an exact `testing` import. Default names of other paths come from the
    // imported package's clause, which no source-local read can see.
    let mut bound_names: Vec<String> = Vec::new();
    let mut functions = Vec::new();
    let mut seen_package = false;
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        let declaration_refusals = refusals.len();
        match node.kind() {
            "comment" => continue,
            "package_clause" if !seen_package => seen_package = true,
            _ if !seen_package => refusals.push((node.start_byte(), Refusal::UnadmittedConstruct)),
            "import_declaration" => import_declaration(
                text,
                node,
                &mut testing_binding,
                &mut bound_names,
                &mut refusals,
            ),
            "function_declaration" | "method_declaration" => {
                if let Some(function) = function_declaration(text, node, &tokens, &mut refusals) {
                    functions.push(function);
                }
            }
            "type_declaration" | "var_declaration" | "const_declaration" => continue,
            _ => refusals.push((node.start_byte(), Refusal::UnadmittedConstruct)),
        }
        if refusals.len() == declaration_refusals {
            end_of_declaration(&tokens, node.end_byte(), &mut refusals);
        }
    }
    if !seen_package {
        refusals.push((0, Refusal::UnadmittedConstruct));
    }
    // Stable: at one position the earlier-recorded token refusal wins.
    refusals.sort_by_key(|(position, _)| *position);
    if let Some((_, refusal)) = refusals.first() {
        return Err(*refusal);
    }
    if root.has_error() {
        return Err(Refusal::UnadmittedConstruct);
    }
    Ok(ParsedGoFile {
        testing_binding,
        functions,
        has_header_build_directive,
    })
}

/// Bracket balance and nesting depth over the whole token stream. An unclosed
/// opener refuses at its own position, because a left-to-right parse would
/// swallow everything after it into one unproven extent.
fn token_refusals(tokens: &[Token]) -> Vec<(usize, Refusal)> {
    let mut open: Vec<Token> = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                open.push(*token);
                if open.len() > MAX_PARSE_DEPTH {
                    return vec![(token.start, Refusal::DepthLimit)];
                }
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                let expected = match open.pop().map(|opener| opener.kind) {
                    Some(TokenKind::LParen) => TokenKind::RParen,
                    Some(TokenKind::LBracket) => TokenKind::RBracket,
                    Some(_) => TokenKind::RBrace,
                    None => return vec![(token.start, Refusal::UnbalancedBraces)],
                };
                if token.kind != expected {
                    return vec![(token.start, Refusal::UnbalancedBraces)];
                }
            }
            _ => {}
        }
    }
    open.first()
        .map(|opener| vec![(opener.start, Refusal::UnbalancedBraces)])
        .unwrap_or_default()
}

/// After a package clause, import, or function declaration, nothing but a
/// separator or end of file may follow on the same line.
fn end_of_declaration(tokens: &[Token], end: usize, refusals: &mut Vec<(usize, Refusal)>) {
    if let Some(next) = tokens.iter().find(|token| token.start >= end) {
        if next.kind != TokenKind::Semi {
            refusals.push((next.start, Refusal::UnadmittedConstruct));
        }
    }
}

fn import_declaration(
    text: &str,
    declaration: Node<'_>,
    testing_binding: &mut TestingBinding,
    bound_names: &mut Vec<String>,
    refusals: &mut Vec<(usize, Refusal)>,
) {
    let mut specs = Vec::new();
    let mut cursor = declaration.walk();
    for child in declaration.named_children(&mut cursor) {
        match child.kind() {
            "import_spec" => specs.push(child),
            "import_spec_list" => {
                let mut inner = child.walk();
                specs.extend(
                    child
                        .named_children(&mut inner)
                        .filter(|spec| spec.kind() == "import_spec"),
                );
            }
            _ => {}
        }
    }
    for spec in specs {
        if let Err(refusal) = import_spec(text, spec, testing_binding, bound_names) {
            refusals.push((spec.start_byte(), refusal));
            return;
        }
    }
}

fn import_spec(
    text: &str,
    spec: Node<'_>,
    testing_binding: &mut TestingBinding,
    bound_names: &mut Vec<String>,
) -> Result<(), Refusal> {
    let alias = match spec.child_by_field_name("name") {
        Some(name) if name.kind() == "dot" => ImportAlias::Dot,
        Some(name) => ImportAlias::Named(&text[name.byte_range()]),
        None => ImportAlias::None,
    };
    let path = spec
        .child_by_field_name("path")
        .filter(|path| path.kind() == "interpreted_string_literal")
        .ok_or(Refusal::UnadmittedConstruct)?;
    let path = unquote_interpreted(
        text,
        Token {
            kind: TokenKind::StringLiteral,
            start: path.start_byte(),
            end: path.end_byte(),
        },
    )?;
    bind_import(&path, alias, testing_binding, bound_names)
}

fn bind_import(
    path: &str,
    alias: ImportAlias<'_>,
    testing_binding: &mut TestingBinding,
    bound_names: &mut Vec<String>,
) -> Result<(), Refusal> {
    if path == CGO_IMPORT_PATH {
        return Err(Refusal::CgoImport);
    }
    if path == TESTING_IMPORT_PATH {
        if !matches!(*testing_binding, TestingBinding::Absent) {
            return Err(Refusal::DuplicateImportBinding);
        }
        *testing_binding = match alias {
            ImportAlias::Named("_") => TestingBinding::Blank,
            ImportAlias::Named(name) => TestingBinding::Qualified(name.to_string()),
            ImportAlias::Dot => TestingBinding::Dot,
            ImportAlias::None => TestingBinding::Qualified(TESTING_IMPORT_PATH.to_string()),
        };
        if let TestingBinding::Qualified(name) = &*testing_binding {
            if bound_names.iter().any(|bound| bound == name) {
                return Err(Refusal::DuplicateImportBinding);
            }
            bound_names.push(name.clone());
        }
    } else if let ImportAlias::Named(name) = alias {
        if name != "_" {
            if bound_names.iter().any(|bound| bound == name) {
                return Err(Refusal::DuplicateImportBinding);
            }
            bound_names.push(name.to_string());
        }
    }
    Ok(())
}

/// One package-level declaration, refused exactly where ADR-0050's subset
/// ends: a bracket in the receiver, a type-parameter list, a separator inside
/// the signature, an unparenthesized result outside the admitted type tokens,
/// or a missing body.
fn function_declaration(
    text: &str,
    node: Node<'_>,
    tokens: &[Token],
    refusals: &mut Vec<(usize, Refusal)>,
) -> Option<FunctionDecl> {
    let refuse = |refusals: &mut Vec<(usize, Refusal)>, position: usize, refusal: Refusal| {
        refusals.push((position, refusal));
        None
    };
    let in_range = |start: usize, end: usize| {
        tokens
            .iter()
            .filter(move |token| token.start >= start && token.end <= end)
    };
    let receiver = node.child_by_field_name("receiver");
    if let Some(receiver) = receiver {
        if let Some(bracket) = in_range(receiver.start_byte(), receiver.end_byte())
            .find(|token| token.kind == TokenKind::LBracket)
        {
            return refuse(refusals, bracket.start, Refusal::UndecidableReceiverType);
        }
    }
    let Some(name) = node.child_by_field_name("name") else {
        return refuse(refusals, node.start_byte(), Refusal::UnadmittedConstruct);
    };
    if let Some(parameters) = node.child_by_field_name("type_parameters") {
        return refuse(refusals, parameters.start_byte(), Refusal::GenericSignature);
    }
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return refuse(refusals, name.end_byte(), Refusal::UnadmittedConstruct);
    };
    let Some(body) = node.child_by_field_name("body") else {
        // A `{` that error recovery left outside the declaration is still the
        // body a left-to-right parse would consume; the token stream decides
        // how it fails.
        let next = tokens.iter().find(|token| token.start >= node.end_byte());
        if next.is_some_and(|token| token.kind == TokenKind::LBrace) {
            return None;
        }
        return refuse(refusals, node.end_byte(), Refusal::UnadmittedConstruct);
    };
    // Go inserts a semicolon only after a line-ending token; inside the
    // signature that is always a syntax error, and so is a body on the next
    // line.
    if let Some(separator) =
        in_range(node.start_byte(), body.start_byte()).find(|token| token.kind == TokenKind::Semi)
    {
        return refuse(refusals, separator.start, Refusal::UnadmittedConstruct);
    }
    let result = node.child_by_field_name("result");
    if let Some(result) = result.filter(|result| result.kind() != "parameter_list") {
        if let Some(token) = in_range(result.start_byte(), result.end_byte()).find(|token| {
            !matches!(
                token.kind,
                TokenKind::Identifier
                    | TokenKind::Keyword
                    | TokenKind::Dot
                    | TokenKind::Star
                    | TokenKind::LParen
                    | TokenKind::RParen
                    | TokenKind::LBracket
                    | TokenKind::RBracket
                    | TokenKind::LBrace
                    | TokenKind::RBrace
            )
        }) {
            return refuse(refusals, token.start, Refusal::UnadmittedConstruct);
        }
    }
    Some(FunctionDecl {
        name: text[name.byte_range()].to_string(),
        is_method: receiver.is_some(),
        has_results: result.is_some(),
        pointer_parameter: pointer_parameter(text, parameters),
        start: node.start_byte(),
        end: body.end_byte(),
    })
}

fn named_non_comment_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect()
}

/// Exactly one parameter field, with at most one name and the type
/// `*qualifier.Type`.
fn pointer_parameter(text: &str, parameters: Node<'_>) -> Option<(String, String)> {
    let [parameter] = named_non_comment_children(parameters)[..] else {
        return None;
    };
    if parameter.kind() != "parameter_declaration" {
        return None;
    }
    let mut cursor = parameter.walk();
    if parameter
        .children_by_field_name("name", &mut cursor)
        .count()
        > 1
    {
        return None;
    }
    let pointer = parameter
        .child_by_field_name("type")
        .filter(|kind| kind.kind() == "pointer_type")?;
    let [qualified] = named_non_comment_children(pointer)[..] else {
        return None;
    };
    if qualified.kind() != "qualified_type" {
        return None;
    }
    let package = qualified.child_by_field_name("package")?;
    let name = qualified.child_by_field_name("name")?;
    Some((
        text[package.byte_range()].to_string(),
        text[name.byte_range()].to_string(),
    ))
}

/// Decode an interpreted string literal the way Go's scanner does.
///
/// Escape syntax was already validated by the lexer; this only assembles the
/// value, because the `testing` and `C` comparisons are over the decoded
/// path, not the source bytes.
fn unquote_interpreted(text: &str, token: Token) -> Result<String, Refusal> {
    let bytes = text.as_bytes();
    let mut value = String::new();
    let mut index = token.start + 1;
    while index < token.end - 1 {
        if bytes[index] != b'\\' {
            let end = index
                + text[index..token.end - 1]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
            value.push_str(text.get(index..end).unwrap_or_default());
            index = end;
            continue;
        }
        let after = bytes[index + 1];
        match after {
            b'a' => {
                value.push('\u{7}');
                index += 2;
            }
            b'b' => {
                value.push('\u{8}');
                index += 2;
            }
            b'f' => {
                value.push('\u{c}');
                index += 2;
            }
            b'n' => {
                value.push('\n');
                index += 2;
            }
            b'r' => {
                value.push('\r');
                index += 2;
            }
            b't' => {
                value.push('\t');
                index += 2;
            }
            b'v' => {
                value.push('\u{b}');
                index += 2;
            }
            b'\\' | b'\'' | b'"' => {
                value.push(after as char);
                index += 2;
            }
            b'x' => {
                let hex = text.get(index + 2..index + 4).unwrap_or_default();
                let byte = u8::from_str_radix(hex, 16).map_err(|_| Refusal::InvalidEscape)?;
                value.push(byte as char);
                index += 4;
            }
            b'u' => {
                let hex = text.get(index + 2..index + 6).unwrap_or_default();
                let code = u32::from_str_radix(hex, 16).map_err(|_| Refusal::InvalidEscape)?;
                value.push(char::from_u32(code).ok_or(Refusal::InvalidEscape)?);
                index += 6;
            }
            b'U' => {
                let hex = text.get(index + 2..index + 10).unwrap_or_default();
                let code = u32::from_str_radix(hex, 16).map_err(|_| Refusal::InvalidEscape)?;
                value.push(char::from_u32(code).ok_or(Refusal::InvalidEscape)?);
                index += 10;
            }
            b'0'..=b'7' => {
                let octal = text.get(index + 1..index + 4).unwrap_or_default();
                let byte = u8::from_str_radix(octal, 8).map_err(|_| Refusal::InvalidEscape)?;
                value.push(byte as char);
                index += 4;
            }
            _ => return Err(Refusal::InvalidEscape),
        }
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// What the parse proves
// ---------------------------------------------------------------------------

/// The admitted variation of the `go.testing.test_function` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestFamily {
    Test,
    Benchmark,
    Fuzz,
}

impl TestFamily {
    fn letter(self) -> char {
        match self {
            Self::Test => 'T',
            Self::Benchmark => 'B',
            Self::Fuzz => 'F',
        }
    }

    fn anchor_kind(self) -> &'static str {
        match self {
            Self::Test => "go_test_function",
            Self::Benchmark => "go_benchmark_function",
            Self::Fuzz => "go_fuzz_function",
        }
    }
}

/// The `go test` name rule, mirrored exactly.
///
/// `cmd/go` accepts a function when its name is the bare prefix (`Test`,
/// `Benchmark`, `Fuzz`) or the prefix followed by a suffix whose first
/// character is not a lowercase letter. This replaces ADR-0021 D3's narrower
/// exported-uppercase narrowing, which existed to keep a text scanner from
/// guessing positives; a parse-proven declaration does not need that extra
/// caution, and mirroring the toolchain's own predicate is the honest version
/// of the claim "a function `go test` would recognize". `TestMain` is the
/// special entrypoint form rather than a test and stays a named non-claim.
fn toolchain_family_for_name(name: &str) -> Option<TestFamily> {
    if name == "TestMain" {
        return None;
    }
    for (prefix, family) in [
        ("Test", TestFamily::Test),
        ("Benchmark", TestFamily::Benchmark),
        ("Fuzz", TestFamily::Fuzz),
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let admitted_suffix = match rest.as_bytes().first() {
                None => true,
                Some(&byte) => !byte.is_ascii_lowercase(),
            };
            if admitted_suffix {
                return Some(family);
            }
        }
    }
    None
}

/// The ADR-0050 anchor, asked of the parse.
///
/// A plain top-level declaration, no results, whose single parameter is
/// optionally named and whose type is exactly a pointer to the family's type
/// letter in the file's own spelling of the `testing` package. Every part is
/// read off tokens a parse produced; nothing is matched against text.
fn admitted_anchor(declaration: &FunctionDecl, binding: &TestingBinding) -> Option<TestFamily> {
    if declaration.is_method || declaration.has_results {
        return None;
    }
    let family = toolchain_family_for_name(&declaration.name)?;
    let TestingBinding::Qualified(local) = binding else {
        return None;
    };
    let (qualifier, type_name) = declaration.pointer_parameter.as_ref()?;
    (qualifier == local && type_name.len() == 1 && type_name.as_bytes()[0] == family.letter() as u8)
        .then_some(family)
}

fn anchor_fact(unit: &CodeUnit, family: TestFamily) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(GO_TESTING_TEST_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded Go testing test-function declaration anchor",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            format!("go_anchor_kind={}", family.anchor_kind()),
        ],
    })
}

fn obligation_fact(
    unit: &CodeUnit,
    entry: &GoObligation,
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
            format!("go_unknown_kind={}", entry.kind),
            // The provider-fallback policy rides on the fact itself so the
            // recorded registry policy is machine-visible per obligation, the
            // same way provider unknowns carry their recovery guidance.
            format!("provider_fallback={}", entry.fallback),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: GO_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: GO_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};
    use std::fs;
    use std::path::Path;

    fn output_at(path: &str, text: &str) -> SourceParseOutput {
        GoTestingParser
            .parse_with_context_output(
                SourceDocument {
                    path,
                    language: Language::Go,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse Go test source")
    }

    fn output(text: &str) -> SourceParseOutput {
        output_at("pkg/catalog_test.go", text)
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(GO_TESTING_TEST_TARGET)
            })
            .count()
    }

    fn anchor_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("go_anchor_kind=").map(String::from))
            .collect()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| {
                assumption
                    .strip_prefix("go_unknown_kind=")
                    .map(String::from)
            })
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

    fn is_degraded(parsed: &SourceParseOutput) -> bool {
        parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error)
    }

    fn test_units(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::GoTestFunction)
            .count()
    }

    #[test]
    fn admitted_test_declarations_anchor_in_both_parameter_spellings() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n\n\
             func TestFilters(*testing.T) {}\n\n\
             func Test_underscore(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 3);
        assert_eq!(test_units(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn benchmarks_fuzz_and_the_bare_test_name_are_the_admitted_variation() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func BenchmarkSorts(b *testing.B) { for i := 0; i < b.N; i++ {} }\n\n\
             func FuzzParses(f *testing.F) { f.Fuzz(func(t *testing.T, raw string) {}) }\n\n\
             func Test(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 3);
        assert_eq!(
            anchor_kinds(&parsed),
            vec![
                "go_benchmark_function".to_string(),
                "go_fuzz_function".to_string(),
                "go_test_function".to_string(),
            ]
        );
    }

    #[test]
    fn an_import_alias_moves_the_anchor_spelling() {
        let aliased = output(
            "package catalog\n\nimport tt \"testing\"\n\n\
             func TestLoads(t *tt.T) {}\n",
        );
        assert_eq!(anchors(&aliased), 1);

        // In that same file the unaliased spelling names nothing.
        let stale = output(
            "package catalog\n\nimport tt \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&stale), 0);
    }

    #[test]
    fn a_grouped_import_block_resolves_aliases_like_the_single_form() {
        let parsed = output(
            "package catalog\n\nimport (\n\t\"strings\"\n\ttt \"testing\"\n)\n\n\
             func TestLoads(t *tt.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn dot_and_blank_testing_imports_abstain_instead_of_guessing() {
        for (source, kind) in [
            (
                "package catalog\n\nimport . \"testing\"\n\nfunc TestLoads(t *T) {}\n",
                "dot_testing_import",
            ),
            (
                "package catalog\n\nimport _ \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
                "blank_testing_import",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(anchors(&parsed), 0, "{kind} must not anchor");
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{kind}: {:?}",
                unknown_kinds(&parsed)
            );
            assert!(
                affected_claims(&parsed).contains(&"go_test_declaration".to_string()),
                "{kind}"
            );
        }
    }

    #[test]
    fn declaration_text_in_strings_and_comments_never_anchors() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             // func TestCommented(t *testing.T) {}\n\
             /* func TestBlockCommented(t *testing.T) {} */\n\
             var golden = `func TestRaw(t *testing.T) {}`\n\
             var quoted = \"func TestQuoted(t *testing.T) {}\"\n\
             var escaped = \"func TestEscaped(t \\\"still a string\\\" ) {}\"\n\
             var r = '('\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_file_without_the_testing_import_anchors_nothing() {
        let parsed = output("package catalog\n\nfunc TestLoads(t *testing.T) {}\n");
        assert_eq!(anchors(&parsed), 0);
        // Absent is not unproven: no identity unknown fires.
        assert!(
            unknown_kinds(&parsed).is_empty(),
            "{:?}",
            unknown_kinds(&parsed)
        );
    }

    #[test]
    fn a_local_testing_lookalike_is_not_the_standard_library() {
        // The import path is compared after decoding, so only the exact
        // standard-library path binds.
        let parsed = output(
            "package catalog\n\nimport testing \"example.test/fake\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 0);
    }

    #[test]
    fn an_escaped_import_path_that_decodes_to_testing_binds_testing() {
        let parsed = output(
            "package catalog\n\nimport \"te\\u0073ting\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn near_miss_signatures_are_functions_not_tests() {
        for source in [
            // lowercase after Test
            "func Testify(t *testing.T) {}",
            "func Testx(t *testing.T) {}",
            // two parameters
            "func TestPair(t *testing.T, extra int) {}",
            // a result
            "func TestResult(t *testing.T) error { return nil }",
            // the wrong type letter for the family
            "func BenchmarkWithT(t *testing.T) {}",
            "func TestWithB(b *testing.B) {}",
            "func FuzzWithT(t *testing.T) {}",
            // value rather than pointer
            "func TestValue(t testing.T) {}",
            // wrong package qualifier
            "func TestWrong(t *foo.T) {}",
            // variadic
            "func TestVariadic(t ...*testing.T) {}",
            // method, not a package-level function
            "type S struct{}\nfunc (s S) TestMethod(t *testing.T) {}",
            "type S struct{}\nfunc (s *S) TestPointer(t *testing.T) {}",
            // the package entry point is not a test
            "func TestMain(m *testing.M) {}",
            // unnamed receiver method with the exact parameter shape
            "type R struct{}\nfunc (R) TestShape(t *testing.T) {}",
        ] {
            let parsed = output(&format!(
                "package catalog\n\nimport (\n\t\"testing\"\n\tfoo \"example.test/foo\"\n)\n\n{source}\n"
            ));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
            assert!(
                unknown_kinds(&parsed).is_empty(),
                "an unadmitted shape is not an unknown: {source}: {:?}",
                unknown_kinds(&parsed)
            );
        }
    }

    #[test]
    fn nested_declarations_in_bodies_are_not_package_level() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestOuter(t *testing.T) {\n\
             \tinner := func(t *testing.T) {}\n\
             \t_ = inner\n\
             \tgo func() { }()\n\
             \tif t == nil {\n\
             \t\tdefer func() { recover() }()\n\
             \t}\n\
             }\n",
        );
        assert_eq!(anchors(&parsed), 1, "only the package-level declaration");
        assert_eq!(test_units(&parsed), 1);
    }

    #[test]
    fn build_directives_are_recorded_not_evaluated() {
        let parsed = output(
            "//go:build linux\n\n// +build amd64\n\npackage catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1, "the file still anchors");
        assert!(unknown_kinds(&parsed).contains(&"build_constraint_not_evaluated".to_string()));
        assert!(affected_claims(&parsed).contains(&"go_build_constraint".to_string()));
    }

    #[test]
    fn a_build_directive_after_the_package_clause_is_prose() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {\n\t//go:build inert inside a body\n}\n",
        );
        assert_eq!(anchors(&parsed), 1);
        assert!(!unknown_kinds(&parsed).contains(&"build_constraint_not_evaluated".to_string()));
    }

    #[test]
    fn only_test_filenames_are_admitted() {
        let parser = GoTestingParser;
        for path in ["pkg/catalog.go", "pkg/_test.go", "pkg/testing.go"] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::Go,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text: "package catalog\n",
                }),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
        assert!(is_go_test_path("pkg/catalog_test.go"));
        assert!(!is_go_test_path("pkg/_test.go"));
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed =
            output("package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n");
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn ir_contains_edges_link_declarations_to_their_module() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n\nfunc helper() {}\n",
        );
        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert_eq!(parsed.report.ir_edges.len(), 2);
    }

    // --- the declared subset, and refusals outside it ---

    #[test]
    fn hostile_constructs_refuse_the_whole_file() {
        for (source, kind) in [
            (
                "package catalog\n\nimport \"testing\"\n\nfunc TestGeneric[T any](t *testing.T) {}\n",
                "generic_signature",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\n\
                 type Buf struct{}\n\nfunc (b Buf[int]) TestLoads(t *testing.T) {}\n",
                "undecidable_receiver_type",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n\
                 func(t *testing.T) {}\n",
                "unadmitted_construct",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nfunc \u{e9}clair(x int) {}\n",
                "unadmitted_construct",
            ),
            (
                "package catalog\n\nimport `testing`\n\nfunc TestLoads(t *testing.T) {}\n",
                "unadmitted_construct",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(anchors(&parsed), 0, "{kind} must not anchor");
            assert!(unknown_kinds(&parsed).contains(&kind.to_string()), "{kind}");
        }
    }

    #[test]
    fn a_generic_type_declaration_is_skipped_and_does_not_refuse_the_file() {
        // D4: generic type declarations carry no anchor and are consumed, not
        // parsed; only signatures refuse. A test declared beside one anchors.
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             type Pair[K comparable, V any] struct{ First K; Second V }\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1);
        // No refusal fires; the only unknowns are the standing residuals
        // that ride on every admitted anchor.
        assert_eq!(
            unknown_kinds(&parsed),
            vec![
                "go_test_execution".to_string(),
                "go_package_identity".to_string(),
            ]
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_cgo_import_refuses_the_whole_file() {
        let parsed = output(
            "package catalog\n\n// #include <stdlib.h>\nimport \"C\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"cgo_import".to_string()));
        assert!(affected_claims(&parsed).contains(&"go_cgo_boundary".to_string()));
        assert!(!is_degraded(&parsed));
    }

    #[test]
    fn malformed_files_are_degraded_and_never_recover() {
        for (source, kind) in [
            (
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {\n\tnote := \"never closed\n}\n",
                "unterminated_literal",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nvar open = `never closed\nfunc TestLoads(t *testing.T) {}\n",
                "unterminated_literal",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\n/* never closed\nfunc TestLoads(t *testing.T) {}\n",
                "unterminated_literal",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nvar bad = \"trailing \\q escape\"\nfunc TestLoads(t *testing.T) {}\n",
                "invalid_escape",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {\n\tif true {\n}\n",
                "unbalanced_braces",
            ),
            (
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T)] {}\n",
                "unbalanced_braces",
            ),
            (
                "package catalog\n\nimport (\n\t\"testing\"\n\tfoo \"example.test/a\"\n\tfoo \"example.test/b\"\n)\nfunc TestLoads(t *testing.T) {}\n",
                "duplicate_import_binding",
            ),
            (
                "package catalog\n\nimport \"testing\"\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
                "duplicate_import_binding",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(anchors(&parsed), 0, "{kind} must not anchor");
            assert!(unknown_kinds(&parsed).contains(&kind.to_string()), "{kind}");
            assert!(is_degraded(&parsed), "{kind} must report a degraded parse");
            // A refusal yields the module unit only: no declaration unit may
            // survive an unproven token stream.
            assert!(
                parsed
                    .report
                    .units
                    .iter()
                    .all(|unit| unit.kind == CodeUnitKind::Module),
                "{kind}"
            );
        }
    }

    #[test]
    fn go_line_discipline_is_reproduced() {
        // A trailing comma continues the parameter list across lines.
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(\n\tt *testing.T,\n) {}\n",
        );
        assert_eq!(anchors(&parsed), 1);
        // A newline after a complete token inserts a semicolon inside the
        // parens, which Go rejects, and so does this parser.
        let broken = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(\n\tt *testing.T\n) {}\n",
        );
        assert_eq!(anchors(&broken), 0);
        assert!(unknown_kinds(&broken).contains(&"unadmitted_construct".to_string()));
        // A comment does not shield the newline: the semicolon lands after
        // the token before it.
        let commented = output(
            "package catalog // the package\n\nimport \"testing\" // the import\n\n\
             func TestLoads(t *testing.T) { // open\n} // close\n",
        );
        assert_eq!(anchors(&commented), 1);
        assert!(commented.report.diagnostics.is_empty());
    }

    #[test]
    fn body_less_declarations_are_outside_the_subset() {
        let parsed =
            output("package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T)\n");
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_construct".to_string()));
    }

    #[test]
    fn a_well_formed_multiline_test_file_parses_whole() {
        let parsed = output(
            "package catalog\n\nimport (\n\t\"strings\"\n\t\"testing\"\n)\n\n\
             type row struct {\n\tname string\n\twant int\n}\n\n\
             var rows = []row{\n\t{\"a\", 1},\n\t{\"b\", 2},\n}\n\n\
             func TestSums(t *testing.T) {\n\
             \tfor _, r := range rows {\n\
             \t\tif got := len(r.name); got != r.want {\n\
             \t\t\tt.Errorf(\"len(%q) = %d; want %d\", r.name, got, r.want)\n\
             \t\t}\n\
             \t}\n\
             }\n\n\
             func TestJoins(t *testing.T) {\n\
             \tconst golden = `raw \" table `\n\
             \tif strings.Join([]string{\"a\"}, \",\") == \"\" {\n\
             \t\tt.Fatal(\"unreachable\")\n\
             \t}\n\
             \t_ = golden\n\
             }\n\n\
             func TestWeirdNumerals(t *testing.T) {\n\
             \t_ = 0x1p-2\n\
             \t_ = 0b1010 + 0o777 + 0755 + 1_000_000 + 0xff11 + 1.5e-3 + .5 + 3i + '\\u00e9'\n\
             }\n",
        );
        assert_eq!(anchors(&parsed), 3);
        assert!(
            parsed.report.diagnostics.is_empty(),
            "{:?}",
            parsed.report.diagnostics
        );
    }

    #[test]
    fn nesting_beyond_the_bounded_depth_refuses_instead_of_recursing() {
        let deep = [
            "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {",
            &"{".repeat(MAX_PARSE_DEPTH + 1),
            &"}".repeat(MAX_PARSE_DEPTH + 1),
            "}\n",
        ]
        .concat();
        let parsed = output(&deep);
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"parser_depth_limit".to_string()));
    }

    #[test]
    fn byte_and_unit_bounds_are_exact_then_fail_at_plus_one() {
        // "package catalog // " is 19 bytes and the newline is 1, so a
        // filler of `limit - 20` lands exactly on the ceiling.
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let exact = format!("package catalog // {}\n", "x".repeat(limit - 20));
        assert!(exact.len() <= limit);
        let parsed = output(&exact);
        assert_eq!(anchors(&parsed), 0);
        assert!(!unknown_kinds(&parsed).contains(&"source_byte_limit".to_string()));
        let over = format!("package catalog // {}\n", "x".repeat(limit - 19));
        assert!(over.len() > limit);
        let parsed = output(&over);
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"source_byte_limit".to_string()));

        let exact_units = "func TestFills(t *testing.T) {}\n".repeat(MAX_FUNCTION_UNITS);
        let parsed = output(&format!(
            "package catalog\n\nimport \"testing\"\n\n{exact_units}"
        ));
        assert_eq!(anchors(&parsed), MAX_FUNCTION_UNITS);
        assert!(!unknown_kinds(&parsed).contains(&"parser_resource_limit".to_string()));
        let over_units = "func TestFills(t *testing.T) {}\n".repeat(MAX_FUNCTION_UNITS + 1);
        let parsed = output(&format!(
            "package catalog\n\nimport \"testing\"\n\n{over_units}"
        ));
        assert_eq!(anchors(&parsed), MAX_FUNCTION_UNITS);
        assert!(unknown_kinds(&parsed).contains(&"parser_resource_limit".to_string()));
    }

    // --- ADR-0020 gate 4: the source-semantic obligation registry ---

    #[test]
    fn standing_obligations_ride_on_every_admitted_anchor_and_only_there() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n\
             func BenchmarkSorts(b *testing.B) {}\n",
        );
        assert_eq!(anchors(&parsed), 2);
        for entry in GO_OBLIGATION_REGISTRY.iter().filter(|entry| entry.standing) {
            assert_eq!(
                unknown_kind_count(&parsed, entry.kind),
                2,
                "standing obligation {} must ride on every anchor",
                entry.kind
            );
        }
        let anchor_units = parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::GoTestFunction)
            .map(|unit| unit.id.as_str().to_string())
            .collect::<Vec<_>>();
        let standing_kinds = GO_OBLIGATION_REGISTRY
            .iter()
            .filter(|entry| entry.standing)
            .map(|entry| format!("go_unknown_kind={}", entry.kind))
            .collect::<Vec<_>>();
        for fact in &parsed.report.semantic_facts {
            if fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| standing_kinds.contains(assumption))
            {
                assert!(
                    anchor_units.contains(&fact.subject),
                    "standing unknown must be anchored to a test unit"
                );
            }
        }
        // With no anchors there is nothing for a standing obligation to ride
        // on, and no other unknown fires either.
        let empty = output("package catalog\n\nimport \"testing\"\n\nfunc helper() {}\n");
        assert!(
            unknown_kinds(&empty).is_empty(),
            "{:?}",
            unknown_kinds(&empty)
        );
    }

    #[test]
    fn every_registry_entry_fires_on_its_trigger_and_every_emission_is_registered() {
        let declared_triggers: &[(&str, &str)] = &[
            (
                "dot_testing_import",
                "package catalog\n\nimport . \"testing\"\n\nfunc TestLoads(t *T) {}\n",
            ),
            (
                "blank_testing_import",
                "package catalog\n\nimport _ \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
            ),
            (
                "build_constraint_not_evaluated",
                "//go:build linux\n\npackage catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
            ),
            (
                "unadmitted_construct",
                "package catalog\n\nfunc TestLoads(t *testing.T) {}\n$ = 1\n",
            ),
            (
                "generic_signature",
                "package catalog\n\nimport \"testing\"\n\nfunc TestGeneric[T any](t *testing.T) {}\n",
            ),
            (
                "undecidable_receiver_type",
                "package catalog\n\nimport \"testing\"\n\ntype S struct{}\n\nfunc (s S[0]) TestLoads(t *testing.T) {}\n",
            ),
            ("cgo_import", "package catalog\n\nimport \"C\"\n"),
            (
                "unterminated_literal",
                "package catalog\n\nimport \"testing\"\n\nvar s = \"open\nfunc TestLoads(t *testing.T) {}\n",
            ),
            (
                "invalid_escape",
                "package catalog\n\nimport \"testing\"\n\nvar s = \"\\q\"\nfunc TestLoads(t *testing.T) {}\n",
            ),
            (
                "unbalanced_braces",
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {\n",
            ),
            (
                "duplicate_import_binding",
                "package catalog\n\nimport \"testing\"\nimport \"testing\"\n",
            ),
            (
                "go_test_execution",
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
            ),
            (
                "go_package_identity",
                "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
            ),
        ];
        for (kind, source) in declared_triggers {
            let parsed = output(source);
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{kind} must fire on its trigger: {:?}",
                unknown_kinds(&parsed)
            );
        }
        // The byte, unit, and depth bounds get their own triggers because
        // their inputs are too large to inline above.
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let over_bytes = format!("package catalog // {}\n", "x".repeat(limit - 17));
        assert!(unknown_kinds(&output(&over_bytes)).contains(&"source_byte_limit".to_string()));
        let over_units = "func TestFills(t *testing.T) {}\n".repeat(MAX_FUNCTION_UNITS + 1);
        assert!(unknown_kinds(&output(&format!(
            "package catalog\n\nimport \"testing\"\n\n{over_units}"
        )))
        .contains(&"parser_resource_limit".to_string()));
        let deep = [
            "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {",
            &"{".repeat(MAX_PARSE_DEPTH + 1),
            &"}".repeat(MAX_PARSE_DEPTH + 1),
            "}\n",
        ]
        .concat();
        assert!(unknown_kinds(&output(&deep)).contains(&"parser_depth_limit".to_string()));

        // And the reverse direction: nothing the frontend emits is outside
        // the registry, and the registry's (claim, kind) pairs are unique.
        let mut pairs = GO_OBLIGATION_REGISTRY
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
                    GO_OBLIGATION_REGISTRY
                        .iter()
                        .any(|entry| entry.kind == emitted),
                    "emitted kind {emitted} from {kind} trigger is not registered"
                );
            }
        }
    }

    #[test]
    fn obligation_output_is_deterministic_and_counts_are_stable() {
        let source = "package catalog\n\nimport \"testing\"\n\n\
                      func TestLoads(t *testing.T) {}\n\
                      func BenchmarkSorts(b *testing.B) {}\n\
                      func Testify(t *testing.T) {}\n";
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
        // and nothing else fires.
        assert_eq!(anchors(&first), 2);
        assert_eq!(unknown_kind_count(&first, "go_test_execution"), 2);
        assert_eq!(unknown_kind_count(&first, "go_package_identity"), 2);
        assert_eq!(
            unknown_kinds(&first).len(),
            4,
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
        for entry in GO_OBLIGATION_REGISTRY {
            for value in [
                format!("affected_claim={}", entry.affected_claim),
                format!("go_unknown_kind={}", entry.kind),
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
        // an unresolved testing import blocks the `go_test_declaration`
        // claim, and a build constraint rides along as a non-blocking
        // subclaim. The rules live verbatim in
        // `go_unknown_reason_blocks_family_membership` and
        // `go_unknown_is_non_blocking_family_subclaim`.
        for entry in GO_OBLIGATION_REGISTRY {
            if entry.blocks_family_claim {
                assert_eq!(
                    entry.affected_claim, "go_test_declaration",
                    "only the declaration identity claim may block the Go family"
                );
                assert_eq!(
                    entry.reason,
                    UnknownReasonCode::UnresolvedImport,
                    "{} blocks with a reason the classifier does not block on",
                    entry.kind
                );
                assert!(!entry.standing);
            } else if entry.reason == UnknownReasonCode::BuildVariantAmbiguity {
                // The classifier's one recorded Go subclaim.
                assert_eq!(entry.affected_claim, "go_build_constraint");
                assert_eq!(entry.kind, "build_constraint_not_evaluated");
            }
        }
        // The two rules the classifier implements are each represented.
        assert!(GO_OBLIGATION_REGISTRY
            .iter()
            .any(|entry| entry.blocks_family_claim));
        assert!(GO_OBLIGATION_REGISTRY.iter().any(|entry| {
            !entry.blocks_family_claim && entry.reason == UnknownReasonCode::BuildVariantAmbiguity
        }));
    }

    #[test]
    fn no_refusal_surface_carries_source_text() {
        // A parser naturally wants to name the token it choked on. None of
        // these surfaces may, because they reach `index --json`, `unknowns`,
        // and the MCP readiness payloads.
        let secrets = ["Secret_Identifier", "s3cr3t-literal", "\u{e9}clair", "$"];
        for source in [
            "package catalog\n\nimport \"testing\"\n\nvar Secret_Identifier = \"s3cr3t-literal\nfunc TestLoads(t *testing.T) {}\n",
            "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {\n\tSecret_Identifier := $\n",
            "package catalog\n\nimport \"testing\"\n\nfunc Test\u{e9}clair(x int) {}\n",
        ] {
            let parsed = output(source);
            let mut surfaces = parsed
                .report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>();
            for fact in &parsed.report.semantic_facts {
                surfaces.push(fact.evidence.note.clone());
                surfaces.extend(fact.assumptions.iter().cloned());
            }
            for surface in surfaces {
                for secret in secrets {
                    assert!(
                        !surface.contains(secret),
                        "refusal surface leaked source text: {surface}"
                    );
                }
            }
        }
    }

    // --- the committed release fixtures, read from disk ---

    fn fixture_text(relative: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("go")
            .join("release")
            .join("v0_2")
            .join(relative);
        fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read fixture {}: {error}", path.display()))
    }

    #[test]
    fn the_exact_tests_fixture_anchors_three_test_functions() {
        let parsed = output_at(
            "testing_exact_tests/catalog_test.go",
            &fixture_text("testing_exact_tests/catalog_test.go"),
        );
        assert_eq!(anchors(&parsed), 3);
        assert_eq!(test_units(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
        // TestMain, the helper, and the alias import are not test anchors.
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::GoFunction)
                .count(),
            2
        );
    }

    #[test]
    fn the_benchmarks_and_fuzz_fixture_anchors_the_family_variation() {
        let parsed = output_at(
            "testing_benchmarks_fuzz/benchmark_fuzz_test.go",
            &fixture_text("testing_benchmarks_fuzz/benchmark_fuzz_test.go"),
        );
        assert_eq!(anchors(&parsed), 4);
        let mut kinds = anchor_kinds(&parsed);
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            vec![
                "go_benchmark_function".to_string(),
                "go_benchmark_function".to_string(),
                "go_fuzz_function".to_string(),
                "go_test_function".to_string(),
            ]
        );
    }

    #[test]
    fn the_table_driven_fixture_anchors_though_bodies_are_skipped() {
        let parsed = output_at(
            "testing_table_driven/table_driven_test.go",
            &fixture_text("testing_table_driven/table_driven_test.go"),
        );
        assert_eq!(anchors(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn the_lookalike_fixture_anchors_nothing() {
        let parsed = output_at(
            "testing_lookalikes/lookalike_test.go",
            &fixture_text("testing_lookalikes/lookalike_test.go"),
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(parsed.report.diagnostics.is_empty());
        // No testing import is bound, so no identity unknown fires either:
        // the shapes are simply not tests.
        assert!(
            unknown_kinds(&parsed).is_empty(),
            "{:?}",
            unknown_kinds(&parsed)
        );
    }

    #[test]
    fn the_low_support_fixture_anchors_two_only() {
        let parsed = output_at(
            "testing_low_support/solo_test.go",
            &fixture_text("testing_low_support/solo_test.go"),
        );
        assert_eq!(anchors(&parsed), 2);
    }

    #[test]
    fn the_parse_degraded_fixtures_abstain_whole_file() {
        for (relative, kind, degraded) in [
            ("generic_signature_test.go", "generic_signature", false),
            ("cgo_import_test.go", "cgo_import", false),
            ("unterminated_string_test.go", "unterminated_literal", true),
            ("unbalanced_braces_test.go", "unbalanced_braces", true),
        ] {
            let parsed = output_at(
                &format!("testing_parse_degraded/{relative}"),
                &fixture_text(&format!("testing_parse_degraded/{relative}")),
            );
            assert_eq!(anchors(&parsed), 0, "{relative} must not anchor");
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{relative}"
            );
            assert_eq!(is_degraded(&parsed), degraded, "{relative}");
            assert!(
                parsed
                    .report
                    .units
                    .iter()
                    .all(|unit| unit.kind == CodeUnitKind::Module),
                "{relative} must keep no declaration unit"
            );
        }
    }

    #[test]
    fn a_syntax_error_inside_a_body_refuses_the_whole_file() {
        // ADR-0058: an `ERROR` node anywhere refuses the file, including in a
        // body the declared subset skips; such source does not compile.
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {\n\tx := := 1\n}\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert_eq!(
            unknown_kinds(&parsed),
            vec!["unadmitted_construct".to_string()]
        );
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn function_types_and_literals_in_declarations_do_not_refuse_the_file() {
        // A `func` keyword inside a type, var, or const declaration is a type
        // or literal, not a declaration; the grammar reads it as Go does.
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             type readerFunc func([]byte) (int, error)\n\n\
             var globalSkip = func(t testing.TB) {}\n\n\
             var helper = func(a int) int {\n\treturn a\n}\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
        assert!(!unknown_kinds(&parsed).contains(&"unadmitted_construct".to_string()));
    }
}
