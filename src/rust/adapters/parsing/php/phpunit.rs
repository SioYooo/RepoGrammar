//! Bounded PHP PHPUnit frontend for the ADR-0047 admitted anchor.
//!
//! Nothing here invokes PHP, Composer, PHPUnit, Artisan, a vendor binary, an
//! autoloader, a plugin, or any repository script. ADR-0024 D3's execution
//! prohibitions all name running PHP or Composer and are unreached: nothing
//! runs.
//!
//! Declarations come from the pinned Tree-sitter PHP grammar (ADR-0058), with
//! the declared PHP subset of ADR-0047 D4 applied over the tree and the
//! subset's own lexer kept as the lexical gate; it is not a text scan. A tree
//! that needed error recovery refuses the file. It reads only files whose
//! bytes begin with the `<?php` prologue; every other `.php` file is HTML
//! output with no PHP code region and never crosses the source-store
//! boundary. The anchor is a question about the parsed declaration structure:
//! a named, non-abstract class whose in-file `extends` chain terminates at a
//! `TestCase`-suffixed name, and a directly declared `public`, non-static,
//! non-abstract, zero-parameter method named `test*` or marked `@test` or
//! `#[Test]`.
//!
//! Asking the question structurally is the point. A byte scan cannot tell a
//! `test*` method in a `TestCase` class from the same spelling in a helper
//! class, a private helper, a static utility, or a string literal, because all
//! of those are byte sequences; the parse can, because they are different
//! nodes. ADR-0024 D6's forbidden rung — regex or text-only matching for the
//! claim — is discharged by this parser over a declared subset, per the
//! ADR-0042 through ADR-0046 precedent.
//!
//! Outside the declared subset the frontend abstains for the whole file: it
//! never guesses, never recovers, and never resynchronizes, so no anchor after
//! a refusal can be invented. Input is untrusted, so input bytes, unit counts,
//! and nesting are bounded.
//!
//! Every typed `UNKNOWN` this frontend can emit is declared exactly once in
//! [`PHP_OBLIGATION_REGISTRY`], the lane's ADR-0020 gate 4 source-semantic
//! obligation registry: the claim each unknown scopes, whether an unmet
//! obligation blocks the family claim, and the provider-fallback policy that
//! names what could discharge it. Nothing outside the bounded parse may turn a
//! registry entry into certainty.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use super::lexer;
use super::syntax::{self, Abstention, DeclaredClass, DeclaredMethod};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

pub const PHP_ANCHOR_ENGINE: &str = "repogrammar-php-phpunit-parser";
pub const PHP_ANCHOR_METHOD: &str = "bounded_php_phpunit_v1";

/// Fixed support target for the one admitted exact anchor.
pub const PHP_TEST_METHOD_TARGET: &str = "phpunit.TestMethod";

/// Context target for the enclosing class; it carries no family role.
pub const PHP_TEST_CLASS_TARGET: &str = "phpunit.TestCase";

const MAX_UNITS: usize = 4_096;

/// One source-semantic obligation the admitted PHPUnit family claim rests on.
///
/// This is the lane's ADR-0020 gate 4 registry. Every typed `UNKNOWN` the
/// frontend emits is declared here exactly once, with the claim it scopes, its
/// reason code, whether an unmet obligation blocks the family claim, and the
/// provider-fallback policy that names what could discharge it.
pub(crate) struct PhpObligation {
    /// Stable affected-claim token (`affected_claim=` assumption).
    pub(crate) affected_claim: &'static str,
    /// Stable kind token (`php_unknown_kind=` assumption).
    pub(crate) kind: &'static str,
    /// Stable protocol reason code (the fact target).
    pub(crate) reason: UnknownReasonCode,
    /// A standing obligation rides on every admitted anchor because the
    /// bounded parse can never discharge it; a triggered one fires only on
    /// the construct that leaves it unmet.
    pub(crate) standing: bool,
    /// Recorded claim impact of an unmet obligation.
    pub(crate) blocks_family_claim: bool,
    /// Source-free, fixed human-facing evidence note.
    pub(crate) note: &'static str,
    /// Provider-fallback policy as a stable low-cardinality token, mirrored on
    /// every emitted fact as the `provider_fallback=` assumption.
    pub(crate) fallback: &'static str,
}

/// The complete ADR-0020 gate 4 source-semantic obligation registry for the
/// `php.phpunit.test_method` family claim.
///
/// No entry blocks the family claim at the repository level: whole-file
/// abstentions drop only their own file's anchors (understatement), and the
/// shape and ancestry obligations exclude the affected declaration rather
/// than unproving the anchors beside it. Two residuals ride on every admitted
/// anchor because the bounded parse can never discharge them: PHPUnit's
/// runtime selection and execution of a declared test, and the fact that a
/// `TestCase`-suffixed base name is naming-shape evidence rather than proof
/// of PHPUnit lineage.
///
/// Fallback tokens: `source_inside_declared_subset_only` means the parse can
/// only read source the ADR-0047 D4 subset admits and never recovers past a
/// refusal. `bounded_attribute_same_line_only` means the PHP 7.4/8.x
/// attribute divergence is discharged only by the same-line closure rule.
/// `bounded_input_refusal` means the untrusted-input bounds refused the file
/// and no provider is involved. `cross_file_source_unread` means the
/// discharging evidence lives in another file this frontend never reads.
/// `source_only_shape_widening_needs_adr` means the admitted shape is a
/// decision and widening it needs a superseding ADR rather than a provider.
/// `no_php_provider_irreducible` means the obligation needs a PHP semantic
/// provider or evaluator, no PHP provider slot exists or is registered, and
/// ADR-0024 forbids executing PHP — so the residual is irreducible under
/// current constraints.
pub(crate) const PHP_OBLIGATION_REGISTRY: &[PhpObligation] = &[
    // Fallback: recoverable only by source inside the declared subset. The
    // parse never recovers past the boundary.
    PhpObligation {
        affected_claim: "php_test_parse",
        kind: "unadmitted_php_construct",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "PHP source left the declared parsed subset, so no later declaration in this file was read",
        fallback: "source_inside_declared_subset_only",
    },
    // Fallback: the one PHP 7.4/8.x divergence inside the subset surface. A
    // same-line-closed attribute keeps every later boundary identical between
    // the readings; a spanning one does not, so the file abstains.
    PhpObligation {
        affected_claim: "php_dialect_invariance",
        kind: "attribute_spanning_lines",
        reason: UnknownReasonCode::ConflictingFacts,
        standing: false,
        blocks_family_claim: false,
        note: "a PHP attribute runs past its opening line, where the PHP 7.4 and 8.x readings disagree about token boundaries",
        fallback: "bounded_attribute_same_line_only",
    },
    PhpObligation {
        affected_claim: "php_test_parse",
        kind: "unterminated_literal",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a PHP string, comment, or heredoc is left open, so every later token boundary is unreliable",
        fallback: "source_inside_declared_subset_only",
    },
    PhpObligation {
        affected_claim: "php_test_parse",
        kind: "php_source_byte_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "PHP source exceeded the bounded input-byte limit",
        fallback: "bounded_input_refusal",
    },
    PhpObligation {
        affected_claim: "php_test_parse",
        kind: "parser_depth_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "PHP nesting exceeded the bounded parse depth, so no later declaration in this file was read",
        fallback: "bounded_input_refusal",
    },
    PhpObligation {
        affected_claim: "php_test_parse",
        kind: "parser_unit_limit",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "PHP parser exceeded the bounded unit limit",
        fallback: "bounded_input_refusal",
    },
    // Fallback: the parent class lives in a file this frontend never reads;
    // admitting cross-file ancestry is a superseding decision.
    PhpObligation {
        affected_claim: "php_phpunit_testcase_ancestry",
        kind: "test_markers_without_resolved_testcase_base",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: false,
        blocks_family_claim: false,
        note: "test-markered methods sit in a class whose in-file extends chain does not end at a TestCase-suffixed name, so the framework identity is unproven",
        fallback: "cross_file_source_unread",
    },
    // Fallback: the admitted shape is a decision; widening it (parameters with
    // provider metadata, implicit visibility) needs a superseding ADR.
    PhpObligation {
        affected_claim: "php_phpunit_test_method_shape",
        kind: "test_method_shape_not_admitted",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a test-markered method is not the admitted public, non-static, non-abstract, zero-parameter shape, so it was not anchored",
        fallback: "source_only_shape_widening_needs_adr",
    },
    PhpObligation {
        affected_claim: "php_phpunit_data_provider_obligation",
        kind: "data_provider_attached",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: false,
        blocks_family_claim: false,
        note: "a test-markered method carries a data-provider attachment, so its invocation shape is framework-selected and it was not anchored",
        fallback: "source_only_shape_widening_needs_adr",
    },
    // Fallback: trait-supplied tests are invisible here, so a trait can only
    // understate support; the trait's own source is never read.
    PhpObligation {
        affected_claim: "php_trait_test_origin",
        kind: "trait_used_in_test_class",
        reason: UnknownReasonCode::InsufficientSupport,
        standing: false,
        blocks_family_claim: false,
        note: "a resolved test class imports a trait, so trait-supplied test methods are not extracted",
        fallback: "cross_file_source_unread",
    },
    // Fallback: the anchor proves a source-visible declaration, not that
    // PHPUnit selects, runs, or passes it. Only an executable-suite model
    // could discharge it and none is authorized.
    PhpObligation {
        affected_claim: "php_phpunit_runtime_selection",
        kind: "test_execution_unproven",
        reason: UnknownReasonCode::FrameworkMagic,
        standing: true,
        blocks_family_claim: false,
        note: "the anchor proves the declaration, not that PHPUnit selects, executes, or passes the test",
        fallback: "no_php_provider_irreducible",
    },
    // Fallback: the TestCase-suffixed base name is naming-shape evidence, not
    // proof of PHPUnit lineage; a binding check needs a PHP provider and none
    // exists or is registered.
    PhpObligation {
        affected_claim: "php_phpunit_base_name_binding",
        kind: "testcase_suffix_is_shape_evidence",
        reason: UnknownReasonCode::UnresolvedImport,
        standing: true,
        blocks_family_claim: false,
        note: "the TestCase-suffixed base name is naming-shape evidence from the parsed header, not proof that the base binds to PHPUnit",
        fallback: "no_php_provider_irreducible",
    },
];

fn registry_entry(kind: &str) -> Result<&'static PhpObligation, ParseError> {
    PHP_OBLIGATION_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind)
        .ok_or_else(|| ParseError::Internal(format!("unregistered PHP obligation kind {kind}")))
}

fn standing_obligations() -> impl Iterator<Item = &'static PhpObligation> {
    PHP_OBLIGATION_REGISTRY
        .iter()
        .filter(|entry| entry.standing)
}

/// True for the only suffix this frontend may read. Discovery classifies the
/// exact case-sensitive `.php` suffix as Source; every other candidate stays
/// inventory and never reaches this parser.
pub fn is_phpunit_source_path(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path).ends_with(".php")
}

#[derive(Debug, Default)]
pub struct PhpPHPUnitParser;

impl SourceParser for PhpPHPUnitParser {
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
    parse_output_with(document, syntax::parse)
}

/// The syntax layer a parse runs: the ADR-0058 Tree-sitter reader, or in
/// tests the legacy parser it replaced, so a differential can run both.
type SyntaxReader =
    for<'a> fn(&'a str, &[lexer::Token]) -> Result<syntax::FileParse<'a>, ParseError>;

fn parse_output_with(
    document: SourceDocument<'_>,
    read_syntax: SyntaxReader,
) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Php || !is_phpunit_source_path(document.path) {
        return Err(ParseError::UnsupportedLanguage);
    }
    // A `.php` file without the open tag holds no PHP code region at all: it
    // is inline HTML output, stays inventory, and is never read. This is the
    // same admission shape the R lane gives the testthat runner path.
    let Some(code_start) = lexer::prologue_code_start(document.text) else {
        return Err(ParseError::UnsupportedLanguage);
    };
    let full_range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let module = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#php_file:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Php,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };
    let mut units = vec![module.clone()];
    let mut facts = Vec::new();

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(obligation_fact(
            &module,
            registry_entry("php_source_byte_limit")?,
            full_range,
        )?);
        return finish(units, facts, Vec::new());
    }

    let mut lexed = match lexer::lex(&document.text[code_start..]) {
        Ok(tokens) => tokens,
        Err(refusal) => {
            return abstain(module, units, Abstention::Lex(refusal), document.path);
        }
    };
    // The lexer runs on the bytes after the prologue; every token moves back
    // onto absolute offsets so ranges, line bounds, and provenance agree.
    for token in &mut lexed {
        token.start += code_start;
        token.end += code_start;
    }
    let parsed = read_syntax(document.text, &lexed)?;
    if let Some(abstention) = parsed.abstention {
        return abstain(module, units, abstention, document.path);
    }

    // Whole-file abstention semantics: the file above either parsed
    // completely or returned early, so every declaration below is proven.
    let class_index: BTreeMap<&str, usize> = parsed
        .classes
        .iter()
        .enumerate()
        .map(|(index, class)| (class.name, index))
        .collect();
    let mut unresolved_markers = false;
    let mut shape_not_admitted = false;
    let mut provider_attached = false;
    let mut limit_hit = false;

    for (index, class) in parsed.classes.iter().enumerate() {
        if class.is_abstract {
            // An abstract base is never instantiated; its markered members
            // understate silently rather than anchoring.
            continue;
        }
        if !resolves_to_testcase(&class_index, &parsed.classes, index) {
            unresolved_markers |= class.methods.iter().any(method_has_marker);
            continue;
        }
        if units.len() < MAX_UNITS {
            units.push(class_unit(&provenance, document.path, class)?);
            facts.push(anchor_fact(
                units.last().expect("class unit just pushed"),
                PHP_TEST_CLASS_TARGET,
                "php_anchor_kind=phpunit_test_class",
                "bounded PHP PHPUnit TestCase-derived class anchor",
            )?);
        } else {
            limit_hit = true;
        }
        for method in &class.methods {
            if !method_has_marker(method) {
                continue;
            }
            if method_has_provider_attachment(method) {
                provider_attached = true;
                continue;
            }
            if !method_has_admitted_shape(method) {
                shape_not_admitted = true;
                continue;
            }
            if units.len() >= MAX_UNITS {
                limit_hit = true;
                break;
            }
            let unit = method_unit(&provenance, document.path, method)?;
            facts.push(anchor_fact(
                &unit,
                PHP_TEST_METHOD_TARGET,
                "php_anchor_kind=phpunit_test_method",
                "bounded PHP PHPUnit test method anchor",
            )?);
            // The standing obligations ride on every admitted anchor: the
            // bounded parse proves the declaration shape and nothing beyond
            // it, so PHPUnit's runtime selection and the base-name binding
            // stay typed UNKNOWN on the anchor itself.
            for entry in standing_obligations() {
                debug_assert!(
                    !entry.blocks_family_claim,
                    "a standing PHP obligation is a recorded residual and must not block"
                );
                facts.push(obligation_fact(&unit, entry, unit.range.clone())?);
            }
            units.push(unit);
        }
    }

    if parsed.trait_used_in_class
        && parsed.classes.iter().enumerate().any(|(index, class)| {
            !class.is_abstract && resolves_to_testcase(&class_index, &parsed.classes, index)
        })
    {
        facts.push(obligation_fact(
            &module,
            registry_entry("trait_used_in_test_class")?,
            module.range.clone(),
        )?);
    }
    for (kind, claim) in [
        (
            "test_markers_without_resolved_testcase_base",
            unresolved_markers,
        ),
        ("test_method_shape_not_admitted", shape_not_admitted),
        ("data_provider_attached", provider_attached),
        ("parser_unit_limit", limit_hit),
    ] {
        if claim {
            facts.push(obligation_fact(
                &module,
                registry_entry(kind)?,
                module.range.clone(),
            )?);
        }
    }

    finish(units, facts, Vec::new())
}

/// The ADR-0047 whole-file abstention: one typed `UNKNOWN`, one degraded
/// diagnostic, and no anchor at all. The diagnostic message is a fixed
/// string and the fact vocabulary is registered, so the refusal stays
/// source-free on every surface.
fn abstain(
    module: CodeUnit,
    units: Vec<CodeUnit>,
    abstention: Abstention,
    path: &str,
) -> Result<SourceParseOutput, ParseError> {
    let fact = obligation_fact(
        &module,
        registry_entry(abstention.unknown_kind())?,
        module.range.clone(),
    )?;
    let diagnostics = vec![ParseDiagnostic {
        path: path.to_string(),
        range: None,
        severity: ParseDiagnosticSeverity::Error,
        message: abstention.diagnostic().to_string(),
    }];
    finish(units, vec![fact], diagnostics)
}

/// Follow the written `extends` chain through classes declared in this file
/// and ask whether it terminates at a `TestCase`-suffixed name.
///
/// The suffix is the source-visible identity evidence the anchor claims —
/// exactly the `TestCase` spelling PHPUnit convention uses, whether written
/// directly, imported, or fully qualified, because all of those spellings
/// share the final segment. It is naming-shape evidence, not proof of
/// lineage: the standing `php_phpunit_base_name_binding` obligation says so
/// on every anchor.
fn resolves_to_testcase(
    class_index: &BTreeMap<&str, usize>,
    classes: &[DeclaredClass<'_>],
    start: usize,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut current = start;
    loop {
        if !visited.insert(current) {
            // A declaration cycle proves no terminus.
            return false;
        }
        let Some(parent) = classes[current].extends_last_segment else {
            return false;
        };
        if parent.ends_with("TestCase") {
            return true;
        }
        match class_index.get(parent) {
            Some(&next) => current = next,
            None => return false,
        }
    }
}

fn method_has_marker(method: &DeclaredMethod<'_>) -> bool {
    method.name.starts_with("test")
        || method
            .nearest_doc
            .is_some_and(|doc| doc_has_token(doc, "@test"))
        || method
            .attributes
            .iter()
            .any(|(name, arguments)| *name == "Test" && !*arguments)
}

fn method_has_provider_attachment(method: &DeclaredMethod<'_>) -> bool {
    method
        .nearest_doc
        .is_some_and(|doc| doc_has_token(doc, "@dataProvider"))
        || method
            .attributes
            .iter()
            .any(|(name, _)| *name == "DataProvider" || name.starts_with("TestWith"))
}

fn method_has_admitted_shape(method: &DeclaredMethod<'_>) -> bool {
    method.explicit_public && !method.is_static && !method.is_abstract && method.zero_parameters
}

/// A bounded annotation-token search: `token` must appear at word boundaries
/// so `@test` matches `@test` and `@test anything` but not `@tests` or
/// `my@tests`.
fn doc_has_token(doc: &str, token: &str) -> bool {
    let bytes = doc.as_bytes();
    let mut from = 0usize;
    while let Some(found) = doc[from..].find(token) {
        let at = from + found;
        let before_ok = at == 0 || !is_identifier_byte(bytes[at - 1]);
        let after = at + token.len();
        let after_ok = after >= bytes.len() || !is_identifier_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        from = at + 1;
    }
    false
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn class_unit(
    provenance: &Provenance,
    path: &str,
    class: &DeclaredClass<'_>,
) -> Result<CodeUnit, ParseError> {
    let (start, end) = class.line;
    Ok(CodeUnit {
        id: CodeUnitId::new(format!("unit:{path}#php_test_class:{start}-{end}"))
            .map_err(ParseError::Internal)?,
        language: Language::Php,
        kind: CodeUnitKind::PhpTestClass,
        range: SourceRange::new(start, end).map_err(ParseError::Internal)?,
        provenance: provenance.clone(),
    })
}

fn method_unit(
    provenance: &Provenance,
    path: &str,
    method: &DeclaredMethod<'_>,
) -> Result<CodeUnit, ParseError> {
    let (start, end) = method.line;
    Ok(CodeUnit {
        id: CodeUnitId::new(format!("unit:{path}#php_test_method:{start}-{end}"))
            .map_err(ParseError::Internal)?,
        language: Language::Php,
        kind: CodeUnitKind::PhpTestMethod,
        range: SourceRange::new(start, end).map_err(ParseError::Internal)?,
        provenance: provenance.clone(),
    })
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

fn obligation_fact(
    unit: &CodeUnit,
    entry: &PhpObligation,
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
            format!("php_unknown_kind={}", entry.kind),
            format!("provider_fallback={}", entry.fallback),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: PHP_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: PHP_ANCHOR_METHOD.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};
    use crate::ports::framework_roles::FrameworkRoleDetector;
    use std::path::Path;

    const HASH: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::Php,
            content_hash: ContentHash::new(HASH).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn output(path: &str, text: &str) -> SourceParseOutput {
        PhpPHPUnitParser
            .parse_with_context_output(document(path, text), &ParserProjectContext::default())
            .expect("parse PHP source")
    }

    fn test_methods(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(PHP_TEST_METHOD_TARGET)
            })
            .count()
    }

    fn test_classes(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(PHP_TEST_CLASS_TARGET)
            })
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
                    .strip_prefix("php_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    fn fixture_text(relative: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read fixture {relative}: {error}"))
    }

    const PREFIX_CLASS: &str = "<?php\n\nnamespace Acme\\Catalog\\Tests;\n\nuse PHPUnit\\Framework\\TestCase;\n\nfinal class CatalogTest extends TestCase\n{\n    public function testLoadsTheCatalog(): void\n    {\n        self::assertTrue(true);\n    }\n\n    public function testFiltersByCategory(): void\n    {\n        self::assertTrue(true);\n    }\n\n    public function testSortsEntriesByName(): void\n    {\n        self::assertTrue(true);\n    }\n}\n";

    #[test]
    fn the_path_and_prologue_gates_admit_only_php_source() {
        for (path, text) in [
            ("main.php", "inventory only"),
            ("main.php", "<?= html ?>"),
            ("main.php", " <?php\nclass A extends TestCase {}\n"),
            ("main.php", "<?phpfoo\nclass A extends TestCase {}\n"),
            ("main.PHP", "<?php\nclass A extends TestCase {}\n"),
            ("view.phtml", "<?php\nclass A extends TestCase {}\n"),
        ] {
            assert_eq!(
                PhpPHPUnitParser.parse(document(path, text)),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
    }

    #[test]
    fn admitted_test_methods_anchor_across_all_three_markers() {
        let parsed = output("tests/CatalogTest.php", PREFIX_CLASS);
        assert_eq!(test_methods(&parsed), 3);
        assert_eq!(test_classes(&parsed), 1);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::PhpTestMethod)
                .count(),
            3
        );
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::PhpTestClass)
                .count(),
            1
        );
        assert!(parsed.report.diagnostics.is_empty());

        // The @test doc-comment marker.
        let doc = output(
            "tests/OrderAnnotationTest.php",
            "<?php\n\nuse PHPUnit\\Framework\\TestCase;\n\nclass OrderAnnotationTest extends TestCase\n{\n    /**\n     * @test\n     */\n    public function appliesVolumeDiscount(): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&doc), 1);

        // The #[Test] attribute marker.
        let attribute = output(
            "tests/InvoiceAttributeTest.php",
            "<?php\n\nuse PHPUnit\\Framework\\TestCase;\n\n#[CoversClass(Invoice::class)]\nfinal class InvoiceAttributeTest extends TestCase\n{\n    #[Test]\n    public function totalsNetAmount(): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&attribute), 1);
        assert_eq!(test_classes(&attribute), 1);
    }

    #[test]
    fn an_in_file_ancestry_chain_anchors_through_its_terminus() {
        let parsed = output(
            "tests/ChainTest.php",
            "<?php\n\nclass ProjectTestCase extends TestCase\n{\n}\n\nfinal class ChainTest extends ProjectTestCase\n{\n    public function testRunsThroughTheChain(): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_classes(&parsed), 2);
        assert_eq!(test_methods(&parsed), 1);
    }

    #[test]
    fn a_fully_qualified_and_aliased_testcase_base_still_anchors() {
        for source in [
            "<?php\nclass A extends \\PHPUnit\\Framework\\TestCase\n{\n    public function testFq(): void\n    {\n    }\n}\n",
            "<?php\nuse PHPUnit\\Framework\\TestCase as BaseTestCase;\nclass B extends BaseTestCase\n{\n    public function testAliased(): void\n    {\n    }\n}\n",
        ] {
            let parsed = output("tests/FqTest.php", source);
            assert_eq!(test_methods(&parsed), 1, "must anchor: {source}");
        }
    }

    #[test]
    fn lookalikes_never_anchor() {
        for source in [
            // Not a TestCase-derived class.
            "<?php\nclass SortHelper\n{\n    public function testCompareEntries(): void\n    {\n    }\n}\n",
            // Base name does not end in TestCase and is not in file.
            "<?php\nclass PaymentTest extends AbstractService\n{\n    public function testCharges(): void\n    {\n    }\n}\n",
            // Private and protected test-prefixed helpers.
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass VisibilityTest extends TestCase\n{\n    private function testHidden(): void\n    {\n    }\n\n    protected function testAlsoHidden(): void\n    {\n    }\n}\n",
            // Static utility spelling.
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass StaticTest extends TestCase\n{\n    public static function testStaticShape(): void\n    {\n    }\n}\n",
            // No explicit visibility keyword.
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass LegacyTest extends TestCase\n{\n    function testNoVisibility(): void\n    {\n    }\n}\n",
            // Abstract base class members.
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nabstract class BasePaymentTest extends TestCase\n{\n    abstract public function testSignatureOnly(): void;\n\n    public function testInheritedShape(): void\n    {\n    }\n}\n",
            // Free function at top level.
            "<?php\nfunction testHelperFunction(): void\n{\n}\n",
            // Uppercase prefix is not the admitted spelling.
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass CaseTest extends TestCase\n{\n    public function TestUppercasePrefix(): void\n    {\n    }\n}\n",
            // An extends cycle proves no terminus.
            "<?php\nclass CycleOne extends CycleTwo\n{\n    public function testCycle(): void\n    {\n    }\n}\n\nclass CycleTwo extends CycleOne\n{\n}\n",
        ] {
            let parsed = output("tests/LookalikeTest.php", source);
            assert_eq!(test_methods(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn shape_and_ancestry_failures_record_their_own_unknowns() {
        // Parameters without provider metadata.
        let parsed = output(
            "tests/ParamsTest.php",
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass ParamsTest extends TestCase\n{\n    public function testWithParameters(int $amount): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"test_method_shape_not_admitted".to_string()));

        // Docblock data provider.
        let provider = output(
            "tests/ProviderTest.php",
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass ProviderTest extends TestCase\n{\n    /**\n     * @dataProvider amounts\n     */\n    public function testWithProvider(int $amount): void\n    {\n    }\n\n    public static function amounts(): array\n    {\n        return [[1]];\n    }\n}\n",
        );
        assert_eq!(test_methods(&provider), 0);
        assert!(unknown_kinds(&provider).contains(&"data_provider_attached".to_string()));

        // Attribute data provider.
        let attribute = output(
            "tests/TestWithTest.php",
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nuse PHPUnit\\Framework\\Attributes\\TestWith;\nclass TestWithTest extends TestCase\n{\n    #[TestWith([1])]\n    public function testWithAttributeData(int $amount): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&attribute), 0);
        assert!(unknown_kinds(&attribute).contains(&"data_provider_attached".to_string()));

        // Markers in a class whose base chain does not resolve in file.
        let unbound = output(
            "tests/UnboundTest.php",
            "<?php\nclass SortHelper\n{\n    public function testCompareEntries(): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&unbound), 0);
        assert!(unknown_kinds(&unbound)
            .contains(&"test_markers_without_resolved_testcase_base".to_string()));
    }

    #[test]
    fn a_trait_import_records_its_origin_unknown_but_direct_methods_still_anchor() {
        let parsed = output(
            "tests/TraitTest.php",
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass TraitTest extends TestCase\n{\n    use SortsEntries;\n\n    public function testDirectDeclaration(): void\n    {\n    }\n}\n",
        );
        assert_eq!(test_methods(&parsed), 1);
        assert!(unknown_kinds(&parsed).contains(&"trait_used_in_test_class".to_string()));
    }

    #[test]
    fn refusals_are_whole_file_typed_and_never_recover() {
        // A would-be anchor before each refusal must be dropped with it.
        let anchor_prefix = "<?php\nuse PHPUnit\\Framework\\TestCase;\nfinal class BeforeTest extends TestCase\n{\n    public function testBefore(): void\n    {\n    }\n}\n";
        let cases: &[(&str, &str)] = &[
            (
                "unadmitted_php_construct",
                "<?php\nuse PHPUnit\\Framework\\{TestCase};\nfinal class A extends TestCase {}\n",
            ),
            (
                "unadmitted_php_construct",
                "<?php\nenum Suit {\n    case Hearts;\n}\n",
            ),
            (
                "unadmitted_php_construct",
                "<?php\n?>\n<p>inline html</p>\n",
            ),
            (
                "unadmitted_php_construct",
                "<?php\n#[test]\nclass A extends TestCase {}\n",
            ),
            (
                "unterminated_literal",
                "<?php\n$x = 'never closed;\nclass A extends TestCase {}\n",
            ),
            ("unterminated_literal", "<?php\n$x = <<<EOT\nnever closed\n"),
            (
                "attribute_spanning_lines",
                "<?php\n#[TestWith([\n    1,\n])]\nfunction f() {}\n",
            ),
        ];
        for (kind, source) in cases {
            let with_anchor = format!("{anchor_prefix}\n{source}");
            let parsed = output("tests/DegradedTest.php", &with_anchor);
            assert_eq!(test_methods(&parsed), 0, "must abstain: {source}");
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{kind}: {:?}",
                unknown_kinds(&parsed)
            );
            assert_eq!(
                parsed.report.diagnostics[0].severity,
                ParseDiagnosticSeverity::Error,
                "{kind}"
            );
            assert_eq!(parsed.report.units.len(), 1, "{kind}");
        }

        // The dialect refusal carries the invariance claim.
        let dialect = output(
            "tests/DialectTest.php",
            &format!("{anchor_prefix}\n#[TestWith([\n    1,\n])]\nfunction f() {{}}\n"),
        );
        assert!(dialect
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .any(|assumption| assumption == "affected_claim=php_dialect_invariance"));
    }

    #[test]
    fn a_syntax_error_in_a_skipped_region_refuses_the_whole_file() {
        // ADR-0058: an `ERROR` or `MISSING` node anywhere refuses the file as
        // the generic unadmitted class, including in a body, parameter list,
        // property default, or constant value that the declared subset only
        // admits for its extent. Such source is not PHP, and a recovered
        // anchor cannot be told apart from a real one.
        for (region, member) in [
            ("body", "public function testB(): void { $x = := 1; }"),
            ("parameters", "public function testB(int $x = ): void {}"),
            ("property default", "public $x = := 1;"),
            ("constant value", "const X = := 1;"),
        ] {
            let source = format!(
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nfinal class ATest extends TestCase\n{{\n    public function testA(): void {{}}\n    {member}\n}}\n"
            );
            let parsed = output("tests/ATest.php", &source);
            assert_eq!(test_methods(&parsed), 0, "{region}");
            assert_eq!(
                unknown_kinds(&parsed),
                vec!["unadmitted_php_construct".to_string()],
                "{region}"
            );
            assert_eq!(parsed.report.units.len(), 1, "{region}");
        }
    }

    #[test]
    fn anchors_inside_strings_comments_and_heredocs_never_form() {
        let parsed = output(
            "tests/LiteralsTest.php",
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass LiteralsTest extends TestCase\n{\n    public function testHoldsLiterals(): void\n    {\n        $a = 'public function testFake(): void {}';\n        $b = \"class FakeTest extends TestCase\";\n        $c = <<<EOT\nclass HeredocTest extends TestCase\n{\n    public function testHeredoc(): void\n    EOT;\n        // public function testCommented(): void {}\n        /* final class BlockTest extends TestCase {} */\n        self::assertTrue(true);\n    }\n}\n",
        );
        assert_eq!(test_methods(&parsed), 1);
        assert_eq!(test_classes(&parsed), 1);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output("tests/CatalogTest.php", PREFIX_CLASS);
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn standing_obligations_ride_on_every_anchor_and_only_there() {
        let parsed = output("tests/CatalogTest.php", PREFIX_CLASS);
        assert_eq!(test_methods(&parsed), 3);
        for entry in PHP_OBLIGATION_REGISTRY
            .iter()
            .filter(|entry| entry.standing)
        {
            let count = unknown_kinds(&parsed)
                .iter()
                .filter(|emitted| *emitted == entry.kind)
                .count();
            assert_eq!(
                count, 3,
                "standing obligation {} must ride on every anchor",
                entry.kind
            );
        }
        let method_units = parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::PhpTestMethod)
            .map(|unit| unit.id.as_str().to_string())
            .collect::<Vec<_>>();
        for fact in &parsed.report.semantic_facts {
            if fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption.starts_with("php_unknown_kind="))
            {
                assert!(
                    method_units.contains(&fact.subject),
                    "an unknown must be scoped to a method or module unit"
                );
            }
        }
        // With no anchors there is nothing for a standing obligation to ride
        // on, and no other unknown fires.
        let empty = output("tests/PlainTest.php", "<?php\nclass Plain {}\n");
        assert!(
            unknown_kinds(&empty).is_empty(),
            "{:?}",
            unknown_kinds(&empty)
        );
    }

    #[test]
    fn every_registry_entry_fires_on_its_trigger_and_every_emission_is_registered() {
        let depth_source = format!(
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass DeepTest extends TestCase\n{{\n    public function testDeep(): void\n    {{\n        {}\n    }}\n}}\n",
            "{".repeat(512)
        );
        let triggers: &[(&str, String)] = &[
            (
                "unadmitted_php_construct",
                "<?php\nif (true) {\n    class A extends TestCase {}\n}\n".to_string(),
            ),
            (
                "attribute_spanning_lines",
                "<?php\n#[TestWith([\n    1,\n])]\nfunction f() {}\n".to_string(),
            ),
            (
                "unterminated_literal",
                "<?php\n$x = 'open;\n".to_string(),
            ),
            (
                "parser_depth_limit",
                depth_source,
            ),
            (
                "test_markers_without_resolved_testcase_base",
                "<?php\nclass SortHelper\n{\n    public function testCompare(): void\n    {\n    }\n}\n".to_string(),
            ),
            (
                "test_method_shape_not_admitted",
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass P extends TestCase\n{\n    public function testP(int $a): void\n    {\n    }\n}\n".to_string(),
            ),
            (
                "data_provider_attached",
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass D extends TestCase\n{\n    /** @dataProvider rows */\n    public function testD(): void\n    {\n    }\n}\n".to_string(),
            ),
            (
                "trait_used_in_test_class",
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass T extends TestCase\n{\n    use Helper;\n    public function testT(): void\n    {\n    }\n}\n".to_string(),
            ),
            (
                "test_execution_unproven",
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass S extends TestCase\n{\n    public function testS(): void\n    {\n    }\n}\n".to_string(),
            ),
            (
                "testcase_suffix_is_shape_evidence",
                "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass S extends TestCase\n{\n    public function testS(): void\n    {\n    }\n}\n".to_string(),
            ),
        ];
        for (kind, source) in triggers {
            let parsed = output("tests/TriggerTest.php", source);
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{kind} must fire on its trigger: {:?}",
                unknown_kinds(&parsed)
            );
        }
        // The byte and unit bounds get their own triggers because their inputs
        // are too large to inline above.
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let over_bytes = format!("<?php\n// {}\n", "x".repeat(limit));
        assert!(over_bytes.len() > limit);
        assert!(unknown_kinds(&output("tests/BigTest.php", &over_bytes))
            .contains(&"php_source_byte_limit".to_string()));
        let over_units = format!(
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass ManyTest extends TestCase\n{{\n{}\n}}\n",
            (0..MAX_UNITS)
                .map(|index| format!("    public function testM{index}(): void\n    {{\n    }}\n"))
                .collect::<String>()
        );
        let parsed = output("tests/ManyTest.php", &over_units);
        assert!(
            unknown_kinds(&parsed).contains(&"parser_unit_limit".to_string()),
            "{:?}",
            unknown_kinds(&parsed)
        );

        // And the reverse direction: nothing the frontend emits is outside
        // the registry, and the registry's (claim, kind) pairs are unique.
        let mut pairs = PHP_OBLIGATION_REGISTRY
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
        for (kind, source) in triggers {
            for emitted in unknown_kinds(&output("tests/TriggerTest.php", source)) {
                assert!(
                    PHP_OBLIGATION_REGISTRY
                        .iter()
                        .any(|entry| entry.kind == emitted),
                    "emitted kind {emitted} from {kind} trigger is not registered"
                );
            }
        }
    }

    #[test]
    fn byte_and_unit_bounds_are_exact_then_fail_at_plus_one() {
        let limit = usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX);
        let exact = format!("<?php\n// {}\n", "x".repeat(limit - 12));
        assert!(exact.len() <= limit);
        let parsed = output("tests/ExactBytesTest.php", &exact);
        assert!(!unknown_kinds(&parsed).contains(&"php_source_byte_limit".to_string()));

        let exact_units = format!(
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass FullTest extends TestCase\n{{\n{}\n}}\n",
            (0..MAX_UNITS - 2)
                .map(|index| format!("    public function testF{index}(): void\n    {{\n    }}\n"))
                .collect::<String>()
        );
        let parsed = output("tests/FullTest.php", &exact_units);
        assert_eq!(test_methods(&parsed), MAX_UNITS - 2);
        assert!(!unknown_kinds(&parsed).contains(&"parser_unit_limit".to_string()));
    }

    #[test]
    fn registry_strings_satisfy_the_stored_assumption_content_rules() {
        // Assumptions and notes are persisted verbatim and the storage layer
        // rejects values that look like source snippets, absolute paths, or
        // URL schemes.
        for entry in PHP_OBLIGATION_REGISTRY {
            for value in [
                format!("affected_claim={}", entry.affected_claim),
                format!("php_unknown_kind={}", entry.kind),
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
    fn output_is_deterministic_and_counts_are_stable() {
        let source = "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass MixedTest extends TestCase\n{\n    public function testA(): void\n    {\n    }\n\n    /** @test */\n    public function b(): void\n    {\n    }\n\n    public function testC(int $x): void\n    {\n    }\n}\n\nclass Helper\n{\n    public function testLookalike(): void\n    {\n    }\n}\n";
        let first = output("tests/MixedTest.php", source);
        let second = output("tests/MixedTest.php", source);
        assert_eq!(
            format!("{:?}", first.report.semantic_facts),
            format!("{:?}", second.report.semantic_facts),
            "obligation facts must be deterministic"
        );
        assert_eq!(
            format!("{:?}", first.report.units),
            format!("{:?}", second.report.units)
        );
        assert_eq!(test_methods(&first), 2);
        assert_eq!(
            unknown_kinds(&first)
                .iter()
                .filter(|kind| *kind == "test_execution_unproven")
                .count(),
            2
        );
        assert_eq!(
            unknown_kinds(&first)
                .iter()
                .filter(|kind| *kind == "testcase_suffix_is_shape_evidence")
                .count(),
            2
        );
        assert_eq!(
            unknown_kinds(&first)
                .iter()
                .filter(|kind| *kind == "test_method_shape_not_admitted")
                .count(),
            1
        );
        assert_eq!(
            unknown_kinds(&first)
                .iter()
                .filter(|kind| *kind == "test_markers_without_resolved_testcase_base")
                .count(),
            1
        );
    }

    #[test]
    fn ir_contains_containment_edges_for_class_and_method_units() {
        let parsed = output("tests/CatalogTest.php", PREFIX_CLASS);
        // One module node, one class node, three method nodes.
        assert_eq!(parsed.report.ir_nodes.len(), 5);
        // The shared substrate links the module to the class and each method;
        // the class-to-method edge needs the shared class/method tables and is
        // deliberately left to the integration wiring.
        assert_eq!(parsed.report.ir_edges.len(), 4);
    }

    #[test]
    fn the_product_parser_routes_php_source_and_zero_reads_files_without_a_prologue() {
        let parser = crate::adapters::parsing::RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(document("main.php", "inventory only")),
            Err(ParseError::UnsupportedLanguage)
        );
        let report = parser
            .parse(document("tests/CatalogTest.php", PREFIX_CLASS))
            .expect("PHP source reaches the bounded frontend");
        assert!(report.semantic_facts.iter().any(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            == Some(PHP_TEST_METHOD_TARGET)));
    }

    #[test]
    fn release_fixtures_parse_with_expected_support_and_stay_source_free() {
        let root = "src/fixtures/php/release/v0_2";
        let corpus: &[(&str, usize, usize)] = &[
            ("phpunit_exact_tests", 4, 6),
            ("phpunit_lookalikes", 1, 0),
            ("phpunit_low_support", 1, 2),
            ("phpunit_degraded", 0, 0),
        ];
        for (fixture, class_count, method_count) in corpus {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(root)
                .join(fixture)
                .join("tests");
            let mut files: Vec<_> = std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("read {fixture}: {error}"))
                .map(|entry| entry.expect("dir entry").path())
                .collect();
            files.sort();
            let mut parsed_total = 0usize;
            let mut class_total = 0usize;
            let mut method_total = 0usize;
            for file in files {
                let text = std::fs::read_to_string(&file)
                    .unwrap_or_else(|error| panic!("read fixture file: {error}"));
                let relative = format!(
                    "tests/{}",
                    file.file_name().expect("name").to_string_lossy()
                );
                let parsed = output(&relative, &text);
                class_total += test_classes(&parsed);
                method_total += test_methods(&parsed);
                // Source-free discipline: identifiers, literals, and secrets
                // from the fixture never reach the parsed surface, and unit
                // ids carry only the repo-relative path.
                let debug = format!("{parsed:?}");
                for marker in [
                    "loadsTheCatalog",
                    "appliesVolumeDiscount",
                    "totalsNetAmount",
                    "assertTrue",
                    "assertSame",
                    "PHPUNIT_FIXTURE_SECRET",
                    "Acme",
                    "acme",
                    env!("CARGO_MANIFEST_DIR"),
                ] {
                    assert!(
                        !debug.contains(marker),
                        "{fixture} leaked {marker} into the parse surface"
                    );
                }
                parsed_total += 1;
            }
            assert!(parsed_total > 0, "{fixture} has no test files");
            assert_eq!(class_total, *class_count, "{fixture} class anchors");
            assert_eq!(method_total, *method_count, "{fixture} method anchors");
        }
    }

    #[test]
    fn degraded_fixtures_record_their_typed_refusal() {
        for (file, kind) in [
            ("tests/DegradedCloseTagTest.php", "unadmitted_php_construct"),
            (
                "tests/DegradedAttributeSpanTest.php",
                "attribute_spanning_lines",
            ),
        ] {
            let text = fixture_text(&format!(
                "src/fixtures/php/release/v0_2/phpunit_degraded/{file}"
            ));
            let parsed = output(file, &text);
            assert_eq!(test_methods(&parsed), 0, "{file}");
            assert_eq!(parsed.report.units.len(), 1, "{file}");
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{file}: {:?}",
                unknown_kinds(&parsed)
            );
            assert_eq!(
                parsed.report.diagnostics[0].severity,
                ParseDiagnosticSeverity::Error,
                "{file}"
            );
        }
    }

    #[test]
    fn the_exact_fixture_forms_the_family_through_the_framework_registry() {
        let text =
            fixture_text("src/fixtures/php/release/v0_2/phpunit_exact_tests/tests/CatalogTest.php");
        let parsed = output("tests/CatalogTest.php", &text);
        let roles = crate::adapters::frameworks::SyntaxFrameworkRoleDetector
            .detect_roles(&parsed.report.units)
            .expect("detect roles");
        let targets = roles
            .iter()
            .map(|fact| fact.target.as_ref().expect("role target").as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![
                "framework:phpunit.test_method",
                "framework:phpunit.test_method",
                "framework:phpunit.test_method"
            ]
        );
    }
}

/// Temporary ADR-0058 differential harness: the Tree-sitter reader against the
/// legacy parser it replaces. Removed by the coordinator with the legacy
/// parser once the fixture-wide capture is done.
#[cfg(test)]
mod differential_tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::{Path, PathBuf};

    const HASH: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    type Outcome = Result<SourceParseOutput, ParseError>;

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::Php,
            content_hash: ContentHash::new(HASH).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn new_outcome(path: &str, text: &str) -> Outcome {
        parse_output_with(document(path, text), syntax::parse)
    }

    fn legacy_outcome(path: &str, text: &str) -> Outcome {
        parse_output_with(document(path, text), syntax::legacy::parse)
    }

    fn assert_equivalent(name: &str, text: &str) {
        let path = "tests/DifferentialTest.php";
        let old = legacy_outcome(path, text);
        let new = new_outcome(path, text);
        assert_eq!(
            format!("{old:?}"),
            format!("{new:?}"),
            "legacy and Tree-sitter outputs differ for corpus entry {name}: {text}"
        );
    }

    fn unknowns(outcome: &Outcome) -> Vec<String> {
        match outcome {
            Ok(output) => output
                .report
                .semantic_facts
                .iter()
                .flat_map(|fact| fact.assumptions.iter())
                .filter_map(|assumption| assumption.strip_prefix("php_unknown_kind="))
                .map(String::from)
                .collect(),
            Err(error) => vec![format!("{error:?}")],
        }
    }

    fn php_files(root: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                php_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "php") {
                out.push(path);
            }
        }
    }

    #[test]
    fn every_committed_fixture_source_is_identical() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/php");
        let mut files = Vec::new();
        php_files(&root, &mut files);
        assert!(files.len() >= 6, "fixture sources found: {}", files.len());
        for file in &files {
            let text = std::fs::read_to_string(file).expect("fixture");
            assert_equivalent(&file.display().to_string(), &text);
        }
    }

    fn wrap(body: &str) -> String {
        format!(
            "<?php\nuse PHPUnit\\Framework\\TestCase;\nclass ATest extends TestCase\n{{\n{body}\n}}\n"
        )
    }

    fn corpus() -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut add = |name: &str, text: String| entries.push((name.to_string(), text));
        // Anchor shapes and markers.
        add("prefix_class", PREFIX.to_string());
        add("empty_php", "<?php".to_string());
        add("empty_php_newline", "<?php\n".to_string());
        add("bom_prefix", format!("\u{feff}{PREFIX}"));
        add("crlf", PREFIX.replace('\n', "\r\n"));
        for (name, body) in [
            ("doc_test", "/** @test */\npublic function loads(): void {}"),
            ("doc_tests_not_marker", "/** @tests */\npublic function loads(): void {}"),
            ("doc_email_not_marker", "/** my@test */\npublic function loads(): void {}"),
            ("doc_multi_tag", "/**\n * Docs.\n * @group slow\n * @test\n */\npublic function loads(): void {}"),
            ("doc_then_plain", "/** @test */\n/* plain */\npublic function loads(): void {}"),
            ("plain_then_doc", "/* plain */\n/** @test */\npublic function loads(): void {}"),
            ("two_docs_last_wins", "/** @test */\n/** other */\npublic function loads(): void {}"),
            ("empty_doc_masks", "/** @test */\n/**/\npublic function loads(): void {}"),
            ("doc_with_line_comment_between", "/** @test */\n// note\npublic function loads(): void {}"),
            ("doc_with_hash_comment_between", "/** @test */\n# note\npublic function loads(): void {}"),
            ("doc_before_stray_semicolon_member", "/** @test */\npublic $x;\npublic function loads(): void {}"),
            ("attr_test", "#[Test]\npublic function loads(): void {}"),
            ("attr_test_args_not_marker", "#[Test()]\npublic function loads(): void {}"),
            ("attr_test_then_doc", "#[Test]\n/** doc */\npublic function loads(): void {}"),
            ("attr_same_line", "#[Test] public function loads(): void {}"),
            ("attr_two_lines", "#[Test]\n#[Group('a')]\npublic function loads(): void {}"),
            ("attr_two_same_line", "#[Test] #[Group('a')] public function loads(): void {}"),
            ("attr_with_array_arg", "#[TestWith([1, 2])]\npublic function loads(): void {}"),
            ("attr_data_provider", "#[DataProvider('p')]\npublic function testLoads(): void {}"),
            ("attr_data_provider_marker", "#[Test]\n#[DataProvider('p')]\npublic function loads(): void {}"),
            ("attr_string_with_bracket", "#[CoversClass('a]b')]\n#[Test]\npublic function loads(): void {}"),
            ("doc_data_provider", "/** @dataProvider p */\npublic function testLoads(): void {}"),
            ("doc_data_provider_and_test", "/**\n * @test\n * @dataProvider p\n */\npublic function loads(): void {}"),
            ("name_prefix_test", "public function testLoads(): void {}"),
            ("name_prefix_Test_capital_not_marker", "public function TestLoads(): void {}"),
            ("name_prefix_testing", "public function testing(): void {}"),
            ("name_not_test", "public function helper(): void {}"),
            ("implicit_public", "function testLoads(): void {}"),
            ("private", "private function testLoads(): void {}"),
            ("protected", "protected function testLoads(): void {}"),
            ("static", "public static function testLoads(): void {}"),
            ("final_public", "final public function testLoads(): void {}"),
            ("public_final", "public final function testLoads(): void {}"),
            ("uppercase_keywords", "PUBLIC STATIC FUNCTION testLoads(): VOID {}"),
            ("uppercase_public", "PUBLIC FUNCTION testLoads(): VOID {}"),
            ("abstract_method", "abstract public function testLoads(): void;"),
            ("with_param", "public function testLoads(int $x): void {}"),
            ("with_param_default_array", "public function testLoads(array $x = [1, 2]): void {}"),
            ("with_variadic", "public function testLoads(...$xs): void {}"),
            ("with_comment_param", "public function testLoads(/* nothing */): void {}"),
            ("with_line_comment_param", "public function testLoads(// nothing\n): void {}"),
            ("with_attr_param", "public function testLoads(#[Foo] $x): void {}"),
            ("no_return_type", "public function testLoads() {}"),
            ("nullable_return", "public function testLoads(): ?int {}"),
            ("qualified_return", "public function testLoads(): \\Foo\\Bar {}"),
            ("relative_return", "public function testLoads(): namespace\\Bar {}"),
            ("static_return", "public function testLoads(): static {}"),
            ("never_return", "public function testLoads(): never {}"),
            ("union_return", "public function testLoads(): int|string {}"),
            ("nullable_union_return", "public function testLoads(): ?int|string {}"),
            ("intersection_return", "public function testLoads(): A&B {}"),
            ("dnf_return", "public function testLoads(): (A&B)|null {}"),
            ("by_ref_return", "public function &testLoads(): array {}"),
            ("keyword_method_name", "public function list(): void {}\npublic function testClass(): void {}"),
            ("method_named_class", "public function class(): void {}\npublic function testIf(): void {}"),
            ("trait_use", "use Helpers;\npublic function testLoads(): void {}"),
            ("trait_use_multi", "use A, B\\C;\npublic function testLoads(): void {}"),
            ("trait_use_block", "use A { a as b; }\npublic function testLoads(): void {}"),
            ("class_const", "const X = 1;\npublic const Y = [1, 2];\npublic function testLoads(): void {}"),
            ("final_class_const", "final public const X = 1;\npublic function testLoads(): void {}"),
            ("typed_const", "public const int X = 1;\npublic function testLoads(): void {}"),
            ("property_plain", "public $a;\nprotected $b = [1, 'x' => 2];\nprivate static $c = null;\npublic function testLoads(): void {}"),
            ("property_typed", "public ?int $a = null;\nprivate array $b = [];\npublic function testLoads(): void {}"),
            ("property_var", "var $a;\npublic function testLoads(): void {}"),
            ("property_readonly", "public readonly string $a;\npublic function testLoads(): void {}"),
            ("property_attr", "#[Foo]\npublic $a;\npublic function testLoads(): void {}"),
            ("property_attr_lowercase", "#[foo]\npublic $a;\npublic function testLoads(): void {}"),
            ("property_hook", "public $a { get => 1; }\npublic function testLoads(): void {}"),
            ("promoted_constructor", "public function __construct(private int $a = 1) {}\npublic function testLoads(): void {}"),
            ("constructor_and_setup", "protected function setUp(): void { parent::setUp(); }\npublic function testLoads(): void {}"),
            ("body_with_closure_and_match", "public function testLoads(): void {\n    $f = function () { return 1; };\n    $g = fn($x) => $x;\n    $r = match (1) { 1 => 'a', default => 'b' };\n    foreach ([1] as $v) { if ($v) { echo $v; } }\n}"),
            ("body_with_string_braces", "public function testLoads(): void {\n    $a = 'not } a brace';\n    $b = \"interp {$c->m()} end\";\n    $d = <<<EOT\nheredoc } body #[Test]\nEOT;\n    $e = <<<'NOW'\nnow } {\nNOW;\n    if ($a === $b) { echo \"} } {\"; }\n}"),
            ("body_with_anonymous_class", "public function testLoads(): void {\n    $o = new class { public function testInner(): void {} };\n}"),
            ("body_with_attribute_closure", "public function testLoads(): void {\n    $f = #[Pure] fn() => 1;\n}"),
            ("body_with_comments", "public function testLoads(): void {\n    // } brace in comment\n    # } another\n    /* } block */\n}"),
            ("nested_declarations_in_strings", "public function testLoads(): void {\n    $a = 'public function testFake(): void {}';\n    $b = \"class FakeTest extends TestCase\";\n}"),
            ("static_closure_in_body", "public function testLoads(): void {\n    $f = static function () {};\n}"),
            ("multiple_methods_mixed", "/** @test */ public function a(): void {}\n#[Test] public function b(): void {}\npublic function testC(): void {}\nprivate function testD(): void {}\npublic static function testE(): void {}"),
        ] {
            add(name, wrap(body));
        }

        // Class and file shape.
        for (name, text) in [
            ("in_file_chain", "<?php\nclass Base extends TestCase\n{\n    protected function make(): int {}\n}\nfinal class T extends Base\n{\n    public function testA(): void {}\n}\n"),
            ("chain_reverse_order", "<?php\nfinal class T extends Base\n{\n    public function testA(): void {}\n}\nclass Base extends TestCase\n{\n}\n"),
            ("chain_cycle", "<?php\nclass A extends B { public function testA(): void {} }\nclass B extends A { public function testB(): void {} }\n"),
            ("unresolved_base", "<?php\nclass A extends Unknown { public function testA(): void {} }\n"),
            ("no_extends", "<?php\nclass A { public function testA(): void {} }\n"),
            ("abstract_base", "<?php\nabstract class Base extends TestCase { public function testA(): void {} }\nfinal class T extends Base { public function testB(): void {} }\n"),
            ("final_abstract_order", "<?php\nfinal abstract class A extends TestCase { public function testA(): void {} }\n"),
            ("abstract_final_order", "<?php\nabstract final class A extends TestCase { public function testA(): void {} }\n"),
            ("fqn_extends", "<?php\nclass A extends \\PHPUnit\\Framework\\TestCase { public function testA(): void {} }\n"),
            ("relative_extends", "<?php\nclass A extends namespace\\TestCase { public function testA(): void {} }\n"),
            ("implements_list", "<?php\nclass A extends TestCase implements \\I, J\\K { public function testA(): void {} }\n"),
            ("implements_only", "<?php\nclass A implements I { public function testA(): void {} }\n"),
            ("extends_comma_list", "<?php\nclass A extends B, C { }\n"),
            ("namespace_use_alias", "<?php\nnamespace App\\Tests;\nuse PHPUnit\\Framework\\TestCase as BaseCase;\nuse function Foo\\bar;\nuse const Foo\\BAZ;\nuse \\Leading\\Backslash;\nclass A extends BaseCase { public function testA(): void {} }\n"),
            ("declare_strict", "<?php\ndeclare(strict_types=1);\nclass A extends TestCase { public function testA(): void {} }\n"),
            ("declare_block", "<?php\ndeclare(ticks=1) { }\n"),
            ("declare_alt", "<?php\ndeclare(ticks=1):\nenddeclare;\n"),
            ("namespace_braced", "<?php\nnamespace App {\n}\n"),
            ("namespace_unnamed_braced", "<?php\nnamespace {\n}\n"),
            ("use_group", "<?php\nuse PHPUnit\\Framework\\{TestCase, Attributes\\Test};\nclass A extends TestCase {}\n"),
            ("use_comma_list", "<?php\nuse A\\B, C\\D;\nclass A extends TestCase {}\n"),
            ("use_function_comma", "<?php\nuse function A\\b, C\\d;\n"),
            ("interface_decl", "<?php\ninterface Runnable {}\n"),
            ("trait_decl", "<?php\ntrait T { public function testA(): void {} }\n"),
            ("enum_decl", "<?php\nenum Suit {\n    case Hearts;\n}\n"),
            ("readonly_class", "<?php\nreadonly class A extends TestCase {}\n"),
            ("final_readonly_class", "<?php\nfinal readonly class A extends TestCase {}\n"),
            ("anonymous_class_toplevel", "<?php\n$x = new class {};\n"),
            ("toplevel_if", "<?php\nif (PHP_VERSION_ID >= 80100) {\n    class A extends TestCase {}\n}\n"),
            ("toplevel_echo", "<?php\necho 1;\n"),
            ("toplevel_assignment", "<?php\n$x = 1;\nclass A extends TestCase { public function testA(): void {} }\n"),
            ("toplevel_call", "<?php\nrequire_once 'x.php';\nclass A extends TestCase {}\n"),
            ("toplevel_closure", "<?php\nfunction () {};\n"),
            ("free_function", "<?php\nfunction helper(int $a, $b = [1]): int { return 1; }\nclass A extends TestCase { public function testA(): void {} }\n"),
            ("free_function_ref", "<?php\nfunction &helper(): array { static $a = []; return $a; }\n"),
            ("free_function_union", "<?php\nfunction helper(): int|string {}\n"),
            ("free_const", "<?php\nconst A = 1, B = [1, 2];\nclass T extends TestCase { public function testA(): void {} }\n"),
            ("free_define", "<?php\ndefine('A', 1);\n"),
            ("stray_semicolons_toplevel", "<?php\n;;\nclass A extends TestCase { public function testA(): void {} };\n;\n"),
            ("two_classes", "<?php\nclass A extends TestCase { public function testA(): void {} }\nclass B extends TestCase { public function testB(): void {} }\n"),
            ("class_attr_top", "<?php\n#[CoversClass(Foo::class)]\nfinal class A extends TestCase { public function testA(): void {} }\n"),
            ("class_attr_top_doc", "<?php\n/** doc */\n#[Foo]\n/** doc2 */\nfinal class A extends TestCase { public function testA(): void {} }\n"),
            ("class_attr_lowercase", "<?php\n#[test]\nclass A extends TestCase {}\n"),
            ("class_attr_two_in_one", "<?php\n#[Foo, Bar]\nclass A extends TestCase {}\n"),
            ("class_attr_qualified", "<?php\n#[Foo\\Bar]\nclass A extends TestCase {}\n"),
            ("class_attr_leading_backslash", "<?php\n#[\\Foo]\nclass A extends TestCase {}\n"),
            ("class_attr_junk_after_name", "<?php\n#[Foo bar]\nclass A extends TestCase {}\n"),
            ("method_attr_lowercase", "<?php\nclass A extends TestCase {\n    #[test]\n    public function testA(): void {}\n}\n"),
            ("method_attr_two_in_one", "<?php\nclass A extends TestCase {\n    #[Test, Group('x')]\n    public function testA(): void {}\n}\n"),
            ("method_attr_with_space", "<?php\nclass A extends TestCase {\n    #[ Test ]\n    public function testA(): void {}\n}\n"),
            ("const_attr_lowercase", "<?php\nclass A extends TestCase {\n    #[foo]\n    const X = 1;\n}\n"),
            ("class_name_keyword_like", "<?php\nclass Enum extends TestCase { public function testA(): void {} }\n"),
            ("only_comments_after_tag", "<?php\n// nothing\n/* nothing */\n# nothing\n"),
            ("class_with_nothing", "<?php\nclass A extends TestCase {}\n"),
            ("class_with_comments_only", "<?php\nclass A extends TestCase {\n    // c\n    /* d */\n    /** e */\n}\n"),
            ("line_comments_between_members", "<?php\nclass A extends TestCase {\n    // c\n    public function testA(): void {}\n    # h\n    public function testB(): void {}\n}\n"),
            ("block_comment_between_members", "<?php\nclass A extends TestCase {\n    public function testA(): void {}\n    /* c */\n    public function testB(): void {}\n}\n"),
        ] {
            add(name, text.to_string());
        }

        // Block comments inside declaration headers.
        for (name, text) in [
            ("comment_class_header_after_name", "<?php\nclass A /* c */ extends TestCase {}\n"),
            ("comment_class_header_before_brace", "<?php\nclass A extends TestCase /* c */ {}\n"),
            ("comment_class_header_before_name", "<?php\nclass /* c */ A extends TestCase {}\n"),
            ("comment_between_class_modifiers", "<?php\nfinal /* c */ class A extends TestCase {}\n"),
            ("comment_before_class_modifiers", "<?php\n/* c */ final class A extends TestCase { public function testA(): void {} }\n"),
            ("comment_method_before_name", "<?php\nclass A extends TestCase {\n    public function /* c */ testA(): void {}\n}\n"),
            ("comment_method_after_name", "<?php\nclass A extends TestCase {\n    public function testA /* c */ (): void {}\n}\n"),
            ("comment_method_before_colon", "<?php\nclass A extends TestCase {\n    public function testA() /* c */ : void {}\n}\n"),
            ("comment_method_before_type", "<?php\nclass A extends TestCase {\n    public function testA(): /* c */ void {}\n}\n"),
            ("comment_method_before_body", "<?php\nclass A extends TestCase {\n    public function testA(): void /* c */ {}\n}\n"),
            ("comment_method_inside_params", "<?php\nclass A extends TestCase {\n    public function testA(/* c */): void {}\n}\n"),
            ("comment_method_in_body", "<?php\nclass A extends TestCase {\n    public function testA(): void { /* c */ }\n}\n"),
            ("comment_after_function_keyword_abstract", "<?php\nabstract class A extends TestCase {\n    abstract public function /* c */ testA(): void;\n}\n"),
            ("comment_between_modifiers_with_body", "<?php\nclass A extends TestCase {\n    public /* c */ function testA(): void {}\n}\n"),
            ("comment_between_modifiers_abstract", "<?php\nabstract class A extends TestCase {\n    abstract public /* c */ function testA(): void;\n    public function testB(): void {}\n}\n"),
            ("comment_between_attr_and_modifier", "<?php\nclass A extends TestCase {\n    #[Test] /* c */ public function a(): void {}\n}\n"),
            ("comment_in_property", "<?php\nclass A extends TestCase {\n    public /* c */ $x = 1;\n    public function testA(): void {}\n}\n"),
            ("comment_in_const", "<?php\nclass A extends TestCase {\n    const /* c */ X = 1;\n    public function testA(): void {}\n}\n"),
            ("comment_in_trait_use", "<?php\nclass A extends TestCase {\n    use /* c */ T;\n    public function testA(): void {}\n}\n"),
            ("comment_in_namespace", "<?php\nnamespace A /* c */ ;\n"),
            ("comment_in_use", "<?php\nuse A /* c */ \\B;\n"),
            ("comment_in_declare_parens", "<?php\ndeclare(/* c */ strict_types=1);\n"),
            ("comment_in_declare_header", "<?php\ndeclare /* c */ (strict_types=1);\n"),
            ("comment_in_free_function_header", "<?php\nfunction /* c */ f() {}\n"),
            ("comment_in_free_function_params", "<?php\nfunction f(/* c */) {}\n"),
            ("comment_in_toplevel_const", "<?php\nconst /* c */ A = 1;\n"),
            ("doc_in_class_extends", "<?php\nclass A extends /** d */ TestCase {}\n"),
        ] {
            add(name, text.to_string());
        }

        // Lexical refusals and bounds.
        for (name, text) in [
            ("unterminated_single", "<?php\n$x = 'never closed;\nclass A extends TestCase {}\n"),
            ("unterminated_double", "<?php\n$x = \"never closed;\n"),
            ("unterminated_block_comment", "<?php\n/* never closed\n"),
            ("unterminated_heredoc", "<?php\n$x = <<<EOT\nnever closed\n"),
            ("heredoc_marker_prefix", "<?php\n$x = <<<EOT\nbody\nEOTENSION;\n"),
            ("heredoc_missing_name", "<?php\n$x = <<<\n"),
            ("attribute_spanning", "<?php\n#[TestWith([\n    1,\n])]\nfunction f() {}\n"),
            ("attribute_unclosed_eof", "<?php\n#[Test"),
            ("attribute_spanning_in_class", "<?php\nclass A extends TestCase {\n    #[TestWith([\n 1])]\n    public function testA(): void {}\n}\n"),
            ("close_tag_end", "<?php\nclass A extends TestCase {}\n?>\n<p>html</p>\n"),
            ("close_tag_early", "<?php\n?>\n<p>inline html</p>\n"),
            ("non_ascii_identifier", "<?php\n$ünicode = 1;\n"),
            ("non_ascii_class_name", "<?php\nclass Ünï extends TestCase {}\n"),
            ("non_ascii_in_string_and_comment", "<?php\nclass A extends TestCase {\n    // ü\n    public function testA(): void { $x = 'ü'; }\n}\n"),
            ("unterminated_before_unadmitted", "<?php\nenum E {}\n$x = 'open;\n"),
            ("lexical_after_unadmitted_wins", "<?php\ninterface I {}\n?>\n"),
            ("unclosed_class_body", "<?php\nclass A extends TestCase\n{\n    public function testOpen(): void\n    {\n"),
            ("unclosed_class_only", "<?php\nclass A extends TestCase\n{\n    public function testOk(): void {}\n"),
            ("missing_method_body_semicolon", "<?php\nclass A extends TestCase {\n    public function testA(): void\n}\n"),
            ("eof_after_class_keyword", "<?php\nclass"),
            ("eof_after_function", "<?php\nclass A extends TestCase {\n    public function"),
            ("depth_closed_256", &deep_body(255, true)),
            ("depth_closed_257", &deep_body(256, true)),
            ("depth_closed_300", &deep_body(299, true)),
            ("depth_unclosed_512", &deep_body(512, false)),
            ("depth_in_string_is_free", &format!("<?php\nclass A extends TestCase {{\n    public function testA(): void {{\n        $x = '{}';\n    }}\n}}\n", "{".repeat(600))),
            ("depth_then_unadmitted_later", &format!("{}enum E {{}}\n", deep_body(299, true))),
            ("unadmitted_then_depth_later", &format!("<?php\nenum E {{}}\n{}", &deep_body(299, true)["<?php\n".len()..])),
        ] {
            add(name, text.to_string());
        }
        entries
    }

    /// Inputs where the grammar and the legacy parser disagree by design:
    /// the legacy parser skipped the region without reading it. The new
    /// behaviour is pinned here; these are not in the equality corpus.
    #[test]
    fn declared_divergences_are_pinned() {
        let anchor = "class A extends TestCase\n{\n    public function testA(): void {}\n}\n";
        for (name, prefix_or_body) in [
            // ADR-0058 syntax_error_in_skipped_region: a body the legacy
            // parser skipped by brace balance.
            (
                "body_syntax_error",
                "<?php\nclass A extends TestCase\n{\n    public function testA(): void { $x = := 1; }\n}\n".to_string(),
            ),
            (
                "parameter_syntax_error",
                "<?php\nclass A extends TestCase\n{\n    public function testA(int $x = ): void {}\n}\n".to_string(),
            ),
            (
                "property_default_syntax_error",
                "<?php\nclass A extends TestCase\n{\n    public $x = := 1;\n    public function testA(): void {}\n}\n".to_string(),
            ),
            (
                "constant_syntax_error",
                format!("<?php\nconst A = := 1;\n{anchor}"),
            ),
            // Grammar 0.25.1 reads only one `declare` directive, though PHP
            // admits a list; the list sits in a region the legacy parser
            // skipped.
            (
                "declare_directive_list",
                format!("<?php\ndeclare(strict_types=1, ticks=1);\n{anchor}"),
            ),
            // A bodiless free function is not PHP; the legacy parser admitted
            // it as an extent.
            ("bodiless_free_function", format!("<?php\nfunction helper();\n{anchor}")),
            // A stray `;` in a class body is not PHP either.
            (
                "stray_semicolon_in_class_body",
                "<?php\nclass A extends TestCase\n{\n    ;\n    public function testA(): void {}\n}\n".to_string(),
            ),
            // `?>` inside a line comment ends PHP mode; the legacy lexer read
            // it as comment text.
            (
                "close_tag_in_line_comment",
                format!("<?php\n// ends php mode ?>\n{anchor}"),
            ),
        ] {
            let path = "tests/DivergenceTest.php";
            let new = new_outcome(path, &prefix_or_body).expect("parse");
            assert!(
                !new.report.diagnostics.is_empty(),
                "{name}: the grammar must refuse the whole file"
            );
            assert_eq!(unknowns(&Ok(new)), vec!["unadmitted_php_construct"], "{name}");
            let old = legacy_outcome(path, &prefix_or_body).expect("legacy parse");
            assert!(
                old.report.diagnostics.is_empty(),
                "{name}: documents that the legacy parser admitted it"
            );
        }
    }

    fn deep_body(nesting: usize, closed: bool) -> String {
        let close = if closed {
            "}".repeat(nesting)
        } else {
            String::new()
        };
        format!(
            "<?php\nclass A extends TestCase\n{{\n    public function testDeep(): void\n    {{\n        {}{close}\n    }}\n}}\n",
            "{".repeat(nesting)
        )
    }

    const PREFIX: &str = "<?php\n\nnamespace Acme\\Catalog\\Tests;\n\nuse PHPUnit\\Framework\\TestCase;\n\nfinal class CatalogTest extends TestCase\n{\n    public function testLoadsTheCatalog(): void\n    {\n        self::assertTrue(true);\n    }\n\n    public function testFiltersByCategory(): void\n    {\n        self::assertTrue(true);\n    }\n}\n";

    #[test]
    fn the_branch_corpus_is_identical() {
        let entries = corpus();
        assert!(entries.len() > 150, "corpus size {}", entries.len());
        let mut failures = Vec::new();
        for (name, text) in &entries {
            let path = "tests/DifferentialTest.php";
            let old = format!("{:?}", legacy_outcome(path, text));
            let new = format!("{:?}", new_outcome(path, text));
            if old != new {
                failures.push(format!(
                    "{name}: legacy {:?} vs tree-sitter {:?}",
                    unknowns(&legacy_outcome(path, text)),
                    unknowns(&new_outcome(path, text))
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} differ:\n{}",
            failures.len(),
            entries.len(),
            failures.join("\n")
        );
    }

    /// Reads the external corpus named by `REPOGRAMMAR_PHP_DIFFERENTIAL_CORPUS`
    /// (a directory, or a newline-separated list file) and prints one line per
    /// differing file. It never fails on a difference.
    #[test]
    #[ignore = "external corpus; set REPOGRAMMAR_PHP_DIFFERENTIAL_CORPUS"]
    fn external_corpus_differential() {
        let Ok(root) = std::env::var("REPOGRAMMAR_PHP_DIFFERENTIAL_CORPUS") else {
            eprintln!("REPOGRAMMAR_PHP_DIFFERENTIAL_CORPUS is not set");
            return;
        };
        let mut files = Vec::new();
        let root = PathBuf::from(root);
        if root.is_file() {
            let list = std::fs::read_to_string(&root).expect("file list");
            files.extend(
                list.lines()
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from),
            );
        } else {
            php_files(&root, &mut files);
        }
        let (mut compared, mut identical, mut skipped) = (0usize, 0usize, 0usize);
        let mut classes: std::collections::BTreeMap<String, usize> = Default::default();
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                skipped += 1;
                continue;
            };
            let path = file.display().to_string();
            let old = catch_unwind(AssertUnwindSafe(|| legacy_outcome(&path, &text)));
            let new = catch_unwind(AssertUnwindSafe(|| new_outcome(&path, &text)));
            compared += 1;
            let (old, new) = match (old, new) {
                (Ok(old), Ok(new)) => (old, new),
                (old, new) => {
                    let class = format!("panic(legacy={}, new={})", old.is_err(), new.is_err());
                    println!("DIFF {class} {path}");
                    *classes.entry(class).or_default() += 1;
                    continue;
                }
            };
            if format!("{old:?}") == format!("{new:?}") {
                identical += 1;
                continue;
            }
            let class = classify(&old, &new);
            println!(
                "DIFF {class} legacy={:?} new={:?} {path}",
                unknowns(&old),
                unknowns(&new)
            );
            *classes.entry(class).or_default() += 1;
        }
        println!(
            "SUMMARY compared={compared} identical={identical} skipped_unreadable={skipped} differing={}",
            compared - identical
        );
        for (class, count) in classes {
            println!("CLASS {class} {count}");
        }
    }

    /// Mutates every committed PHP source (drop a line, drop a token, insert a
    /// block comment before a token, truncate after a token) and groups the
    /// legacy-versus-Tree-sitter differences by refusal pair, with one example
    /// each. Prints; never fails on a difference.
    #[test]
    #[ignore = "mutation differential; run with --nocapture"]
    fn mutation_differential() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/php");
        let mut files = Vec::new();
        php_files(&root, &mut files);
        let mut sources: Vec<String> = files
            .iter()
            .map(|file| std::fs::read_to_string(file).expect("fixture"))
            .collect();
        sources.extend(corpus().into_iter().map(|(_, text)| text));
        let mut groups: std::collections::BTreeMap<String, (usize, Vec<String>)> =
            Default::default();
        let (mut total, mut differing) = (0usize, 0usize);
        for source in &sources {
            let Some(start) = lexer::prologue_code_start(source) else {
                continue;
            };
            let Ok(tokens) = lexer::lex(&source[start..]) else {
                continue;
            };
            let mut mutants: Vec<(String, String)> = Vec::new();
            for (index, line) in source.split_inclusive('\n').enumerate() {
                if index == 0 {
                    continue;
                }
                let kept: String = source
                    .split_inclusive('\n')
                    .enumerate()
                    .filter(|(at, _)| *at != index)
                    .map(|(_, line)| line)
                    .collect();
                mutants.push((format!("drop line {index}: {}", line.trim()), kept));
            }
            for token in &tokens {
                let (from, to) = (token.start + start, token.end + start);
                let shown = source[from..to].chars().take(20).collect::<String>();
                mutants.push((
                    format!("drop token {shown:?}@{from}"),
                    format!("{}{}", &source[..from], &source[to..]),
                ));
                mutants.push((
                    format!("comment before {shown:?}@{from}"),
                    format!("{}/* c */ {}", &source[..from], &source[from..]),
                ));
                mutants.push((
                    format!("truncate after {shown:?}@{to}"),
                    source[..to].to_string(),
                ));
            }
            for (description, text) in mutants {
                total += 1;
                let path = "tests/MutantTest.php";
                let old = catch_unwind(AssertUnwindSafe(|| legacy_outcome(path, &text)));
                let new = catch_unwind(AssertUnwindSafe(|| new_outcome(path, &text)));
                let (Ok(old), Ok(new)) = (old, new) else {
                    differing += 1;
                    groups
                        .entry("panic".to_string())
                        .or_insert((0, vec![description]))
                        .0 += 1;
                    continue;
                };
                if format!("{old:?}") == format!("{new:?}") {
                    continue;
                }
                differing += 1;
                let key = format!(
                    "{} legacy={:?} new={:?}",
                    classify(&old, &new),
                    unknowns(&old)
                        .into_iter()
                        .filter(|kind| kind.contains("_") && !kind.starts_with("test"))
                        .collect::<Vec<_>>(),
                    unknowns(&new)
                        .into_iter()
                        .filter(|kind| kind.contains("_") && !kind.starts_with("test"))
                        .collect::<Vec<_>>()
                );
                let group = groups.entry(key).or_insert((0, Vec::new()));
                group.0 += 1;
                if group.1.len() < 400 {
                    group
                        .1
                        .push(format!("{description} => {}", text.replace('\n', "\\n")));
                }
            }
        }
        println!("MUTATION total={total} differing={differing}");
        for (key, (count, examples)) in groups {
            println!("GROUP {count} {key}");
            for example in examples {
                println!("  EX {example}");
            }
        }
    }

    fn classify(old: &Outcome, new: &Outcome) -> String {
        let (Ok(old_output), Ok(new_output)) = (old, new) else {
            return "result_kind_changed".to_string();
        };
        let refused = |output: &SourceParseOutput| !output.report.diagnostics.is_empty();
        let units = |output: &SourceParseOutput| {
            output
                .report
                .units
                .iter()
                .map(|unit| (format!("{:?}", unit.kind), unit.range.clone()))
                .collect::<Vec<_>>()
        };
        match (refused(old_output), refused(new_output)) {
            (true, true) => "refusal_class_changed".to_string(),
            (true, false) => "unknown_removed".to_string(),
            (false, true) => "unknown_added".to_string(),
            (false, false) => {
                let (old_units, new_units) = (units(old_output), units(new_output));
                let kinds = |units: &[(String, SourceRange)]| {
                    units
                        .iter()
                        .map(|(kind, _)| kind.clone())
                        .collect::<Vec<_>>()
                };
                if kinds(&old_units) != kinds(&new_units) {
                    "anchor_changed".to_string()
                } else if old_units != new_units {
                    "range_shift".to_string()
                } else {
                    "unknown_facts_changed".to_string()
                }
            }
        }
    }
}
