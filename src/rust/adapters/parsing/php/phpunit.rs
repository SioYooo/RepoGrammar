//! Bounded PHP PHPUnit frontend for the ADR-0047 admitted anchor.
//!
//! Nothing here invokes PHP, Composer, PHPUnit, Artisan, a vendor binary, an
//! autoloader, a plugin, or any repository script. ADR-0024 D3's execution
//! prohibitions all name running PHP or Composer and are unreached: nothing
//! runs.
//!
//! This is a hand-written lexer and recursive-descent parser over the declared
//! PHP subset of ADR-0047 D4, not a text scan. It reads only files whose bytes
//! begin with the `<?php` prologue; every other `.php` file is HTML output
//! with no PHP code region and never crosses the source-store boundary. The
//! anchor is a question about the parsed declaration structure: a named,
//! non-abstract class whose in-file `extends` chain terminates at a
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
    let parsed = syntax::parse(document.text, &lexed);
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
