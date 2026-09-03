//! Bounded VB.NET MSTest frontend for the ADR-0043 admitted shape.
//!
//! MSTest discovers tests by attribute wherever they compile, so unlike Go and
//! R there is no runner-defined file set to narrow on. Every `.vb` file is
//! decoded and the bound is what this frontend emits: a module unit, an
//! admitted `TestClass`, and its admitted `TestMethod`s. Ordinary declarations
//! produce no unit, so the absence of a unit is not evidence that a file has no
//! code.
//!
//! Nothing here invokes MSBuild, Roslyn, `vbc`, `dotnet`, NuGet, an analyzer, a
//! source generator, a package script, a child process, or the network.
//!
//! This module owns the claim, not the syntax. `super::syntax` decides what the
//! token stream is and refuses whatever falls outside the declared VB.NET
//! subset; everything here is the ADR-0043 D2 rule applied to the declarations
//! that survived. The separation is deliberate: a widened claim and a widened
//! grammar are different decisions and must not be made in the same place.
//!
//! VB.NET lexes unusually cleanly for this. `'` and `REM` start comments and
//! VB has no single-quoted string, so the character that opens a comment is
//! never a delimiter — the ambiguity that rules Ruby out does not exist here.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use super::syntax;
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub const VB_ANCHOR_ENGINE: &str = "repogrammar-vbnet-mstest-parser";
pub const VB_ANCHOR_METHOD: &str = "bounded_vbnet_mstest_declaration_v2";

/// Fixed support target for the one admitted exact anchor.
pub const VB_TEST_METHOD_TARGET: &str = "mstest.TestMethod";

const MSTEST_NAMESPACE: &str = "microsoft.visualstudio.testtools.unittesting";
const MAX_UNITS: usize = 4_096;

#[derive(Debug, Default)]
pub struct VisualBasicMsTestParser;

impl SourceParser for VisualBasicMsTestParser {
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
    if document.language != Language::VisualBasic {
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
            "unit:{}#vb_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::VisualBasic,
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
            "vb_syntax_admission",
            "source_byte_limit",
            full_range,
            "VB.NET source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    // The parser abstains rather than recovering. Outside the admitted subset
    // there is no partial tree to read, so the file keeps its module unit,
    // records why, and yields no anchor at all.
    let parsed = match syntax::parse_file(document.text) {
        Ok(parsed) => parsed,
        Err(refusal) => {
            facts.push(unknown_fact(
                &module,
                refusal_reason(refusal),
                "vb_syntax_admission",
                refusal.unknown_kind(),
                full_range,
                refusal.message(),
            )?);
            return finish(
                units,
                facts,
                vec![degraded(document.path, refusal.message())],
            );
        }
    };

    // An `Imports` selected by an unevaluated constant cannot make a bare
    // attribute name resolve, so only unconditional code binds the spelling.
    // An aliased import binds a different name and never does.
    let imports_mstest = parsed
        .imports
        .iter()
        .any(|import| !import.conditional && !import.aliased && import.name == MSTEST_NAMESPACE);

    let mut scan = AnchorScan::default();
    collect_anchors(&parsed.types, imports_mstest, &mut scan);

    if scan.skipped_conditional {
        // Every branch of a `#If` cannot compile, and RepoGrammar cannot
        // evaluate the constant that picks one, so no branch may anchor.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::BuildVariantAmbiguity,
            "vb_conditional_compilation",
            "declaration_under_conditional_compilation",
            module.range.clone(),
            "an admitted declaration sits inside conditional compilation, so the constant that selects it is unevaluated",
        )?);
    }
    for group in scan.groups {
        if units.len() >= MAX_UNITS {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "vb_syntax_admission",
                "unit_resource_limit",
                module.range.clone(),
                "VB.NET parse exceeded the bounded unit limit",
            )?);
            break;
        }
        let class_unit = declaration_unit(
            &document,
            &provenance,
            CodeUnitKind::VbTestClass,
            group.class_start,
            group.class_end,
        )?;
        facts.push(anchor_fact(
            &class_unit,
            "mstest.TestClass",
            "vb_anchor_kind=mstest_test_class",
            "bounded VB.NET MSTest TestClass attribute anchor",
        )?);
        units.push(class_unit);
        for method in group.methods {
            if units.len() >= MAX_UNITS {
                break;
            }
            let unit = declaration_unit(
                &document,
                &provenance,
                CodeUnitKind::VbTestMethod,
                method.0,
                method.1,
            )?;
            facts.push(anchor_fact(
                &unit,
                VB_TEST_METHOD_TARGET,
                "vb_anchor_kind=mstest_test_method",
                "bounded VB.NET MSTest TestMethod attribute anchor",
            )?);
            units.push(unit);
        }
    }

    if scan.unresolved_attribute {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "vb_mstest_attribute_binding",
            "mstest_attribute_without_import",
            module.range.clone(),
            "an MSTest attribute name appears without an exact import or fully qualified namespace",
        )?);
    }

    finish(units, facts, Vec::new())
}

/// The typed reason a refused file records.
///
/// A lexical divergence is `ConflictingFacts` because the token stream itself
/// stops being decidable, which is ADR-0040's rule for the same situation in
/// SQL. Everything else is `InsufficientSupport`: the construct is real, the
/// frontend simply does not admit it.
///
/// Neither blocks family membership, and neither needs to. A refused file
/// yields no anchor, so the abstention has already removed everything a block
/// could act on; making it blocking would only let one unadmitted file veto
/// declarations proven in another.
fn refusal_reason(refusal: syntax::Refusal) -> UnknownReasonCode {
    match refusal {
        syntax::Refusal::InterpolatedString | syntax::Refusal::XmlLiteral => {
            UnknownReasonCode::ConflictingFacts
        }
        _ => UnknownReasonCode::InsufficientSupport,
    }
}

/// The MSTest anchors found in a parsed file.
#[derive(Default)]
struct AnchorScan {
    groups: Vec<TestClassGroup>,
    /// An MSTest attribute name appeared that no import or qualification binds.
    unresolved_attribute: bool,
    /// An otherwise admitted declaration sat inside conditional compilation and
    /// was not anchored.
    skipped_conditional: bool,
}

struct TestClassGroup {
    class_start: usize,
    class_end: usize,
    methods: Vec<(usize, usize)>,
}

/// Apply ADR-0043 D2 to the parsed declarations.
///
/// A type anchors only when it carries `TestClass`; when it does not, its
/// nested types are still examined, because a test class declared inside an
/// ordinary one is still discovered by MSTest. An admitted class does not
/// descend further: its own members are its anchors.
fn collect_anchors(types: &[syntax::TypeDecl], imports_mstest: bool, scan: &mut AnchorScan) {
    for declaration in types {
        match attribute_binding(&declaration.attributes, "testclass", imports_mstest) {
            Some(true) => {}
            Some(false) => {
                scan.unresolved_attribute = true;
                unadmitted_members(declaration, imports_mstest, scan);
                continue;
            }
            None => {
                unadmitted_members(declaration, imports_mstest, scan);
                continue;
            }
        }
        if declaration.conditional {
            scan.skipped_conditional = true;
            continue;
        }
        let mut methods = Vec::new();
        for method in &declaration.methods {
            match attribute_binding(&method.attributes, "testmethod", imports_mstest) {
                Some(true) => {}
                Some(false) => {
                    scan.unresolved_attribute = true;
                    continue;
                }
                None => continue,
            }
            // A `Function` returns a value and is not an MSTest test.
            if !method.is_sub {
                continue;
            }
            if method.conditional {
                scan.skipped_conditional = true;
                continue;
            }
            methods.push((method.start, method.end));
        }
        scan.groups.push(TestClassGroup {
            class_start: declaration.start,
            class_end: declaration.end,
            methods,
        });
    }
}

/// Descend into a type that did not anchor.
///
/// Its nested types may still carry `TestClass`, and its own methods may still
/// carry an MSTest attribute name that nothing binds — which stays reported
/// rather than silently dropped just because the enclosing class was not
/// admitted.
fn unadmitted_members(declaration: &syntax::TypeDecl, imports_mstest: bool, scan: &mut AnchorScan) {
    for method in &declaration.methods {
        if attribute_binding(&method.attributes, "testmethod", imports_mstest) == Some(false) {
            scan.unresolved_attribute = true;
        }
    }
    collect_anchors(&declaration.nested, imports_mstest, scan);
}

/// Whether an attribute list carries `name` as the MSTest attribute.
///
/// `None` means the name is absent. `Some(false)` means it is present under a
/// namespace that is not MSTest, or bare in a file that does not import MSTest
/// — which is reported rather than claimed.
fn attribute_binding(
    attributes: &[syntax::AttributeRef],
    name: &str,
    imports_mstest: bool,
) -> Option<bool> {
    let mut seen = None;
    for attribute in attributes {
        if attribute.name != name {
            continue;
        }
        let bound = match attribute.qualifier.as_deref() {
            Some(MSTEST_NAMESPACE) => true,
            Some(_) => false,
            None => imports_mstest,
        };
        if bound {
            return Some(true);
        }
        seen = Some(false);
    }
    seen
}

fn declaration_unit(
    document: &SourceDocument<'_>,
    provenance: &Provenance,
    kind: CodeUnitKind,
    start: usize,
    end: usize,
) -> Result<CodeUnit, ParseError> {
    Ok(CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#{}:{}-{}",
            document.path,
            kind.as_str(),
            start,
            end
        ))
        .map_err(ParseError::Internal)?,
        language: Language::VisualBasic,
        kind,
        range: SourceRange::new(start, end).map_err(ParseError::Internal)?,
        provenance: provenance.clone(),
    })
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
            format!("vb_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: VB_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: VB_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        VisualBasicMsTestParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/CatalogTests.vb",
                    language: Language::VisualBasic,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse VB.NET source")
    }

    fn methods(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(VB_TEST_METHOD_TARGET)
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
                    .strip_prefix("vb_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const IMPORTED: &str = "Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\n";

    #[test]
    fn admitted_attributes_anchor_the_class_and_its_methods() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\n\
             <TestMethod>\nPublic Sub FiltersCatalog()\nEnd Sub\n\
             Public Sub Helper()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 2, "the helper is not a test");
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::VbTestClass)
                .count(),
            1
        );
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = output(
            "imports microsoft.visualstudio.testtools.unittesting\n\n\
             <testclass()>\npublic class CatalogTests\n\
             <testmethod()>\npublic sub LoadsCatalog()\nend sub\nend class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_fully_qualified_attribute_needs_no_import() {
        let parsed = output(
            "<Microsoft.VisualStudio.TestTools.UnitTesting.TestClass()>\n\
             Public Class CatalogTests\n\
             <Microsoft.VisualStudio.TestTools.UnitTesting.TestMethod()>\n\
             Public Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn without_an_import_or_qualification_nothing_anchors() {
        let parsed = output(
            "<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_foreign_namespace_is_reported_rather_than_claimed() {
        let parsed = output(
            "<Other.Framework.TestClass()>\nPublic Class CatalogTests\n\
             <Other.Framework.TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"mstest_attribute_without_import".to_string()));
    }

    #[test]
    fn attribute_text_in_comments_and_strings_never_anchors() {
        let parsed = output(&format!(
            "{IMPORTED}' <TestClass()>\nREM <TestClass()>\n\
             Dim sample As String = \"<TestClass()>\"\n\
             Public Class CatalogTests\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn a_function_is_not_a_test_method() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Function LoadsCatalog() As Boolean\nReturn True\nEnd Function\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 0, "an MSTest test method returns nothing");
    }

    #[test]
    fn a_test_method_outside_a_test_class_is_not_discovered() {
        let parsed = output(&format!(
            "{IMPORTED}Public Class PlainClass\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn an_aliased_import_does_not_bind_the_bare_spelling() {
        let parsed = output(
            "Imports MS = Microsoft.VisualStudio.TestTools.UnitTesting\n\n\
             <TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn nested_classes_close_at_their_own_end() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             Public Class Nested\nEnd Class\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(
            methods(&parsed),
            1,
            "the method after a nested class still counts"
        );
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn an_unterminated_string_reports_a_degraded_parse() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Dim note As String = \"never closed\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
    }

    #[test]
    fn conditional_compilation_branches_must_not_both_anchor() {
        // `#If` selects one branch at compile time from a constant this
        // frontend does not evaluate. Admitting both invents a member that
        // never compiles; admitting either asserts a constant we do not know.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalogDebug()\n    End Sub\n\
             #Else\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalogRelease()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(
            methods(&parsed),
            0,
            "neither branch may anchor: the constant that selects one is unevaluated"
        );
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn declarations_outside_a_conditional_still_anchor() {
        // The skipped branch understates support; it does not unprove what
        // sits in unconditional code.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub OnlyInDebug()\n    End Sub\n\
             #End If\n\
             \x20   <TestMethod()>\n    Public Sub FiltersCatalog()\n    End Sub\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 2);
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_conditional_test_class_does_not_anchor() {
        // The same rule applies one level up: a whole class compiled only
        // under a constant is a build variant, not a proven declaration.
        let parsed = output(&format!(
            "{IMPORTED}#If DEBUG Then\n<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n#End If\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::VbTestClass)
                .count(),
            0
        );
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_region_is_not_a_conditional() {
        // `#Region` is an editor fold. It compiles unconditionally, so it must
        // not cost an anchor -- and `#End Region` must not be read as the
        // `#End If` that would reopen one.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #Region \"Tests\"\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End Region\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_external_source_directive_is_not_a_conditional() {
        // `#ExternalSource` remaps debugger line numbers. It selects no branch.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #ExternalSource(\"Catalog.vb\", 1)\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End ExternalSource\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_conditional_import_does_not_bind_the_bare_spelling() {
        // The same unsoundness one level up: an `Imports` selected by an
        // unevaluated constant cannot make a bare attribute name resolve.
        let parsed = output(
            "#If CUSTOM_BUILD Then\n\
             Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\
             #End If\n\n\
             <TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n",
        );
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"mstest_attribute_without_import".to_string()));
    }

    #[test]
    fn a_fully_qualified_attribute_survives_a_conditional_import() {
        // A fully qualified name never needed the import, so a conditional one
        // costs it nothing.
        let parsed = output(
            "#If CUSTOM_BUILD Then\n\
             Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\
             #End If\n\n\
             <Microsoft.VisualStudio.TestTools.UnitTesting.TestClass()>\n\
             Public Class CatalogTests\n\
             \x20   <Microsoft.VisualStudio.TestTools.UnitTesting.TestMethod()>\n\
             \x20   Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn an_end_region_does_not_close_an_open_conditional() {
        // Reading any `#End ...` as a close would reopen the file mid-branch
        // and anchor a declaration the build may not contain.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n#Region \"Tests\"\n#End Region\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_end_external_source_does_not_close_an_open_conditional() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n#ExternalSource(\"Catalog.vb\", 1)\n#End ExternalSource\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn directive_matching_follows_the_language_case_rules() {
        // VB.NET is case-insensitive, so a lowercase directive selects a
        // branch exactly as the documented spelling does.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #if debug then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #end if\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_conditional_inside_a_method_body_still_anchors_it() {
        // The declaration itself is unconditional; only statements inside it
        // vary. Abstaining here would understate support for no reason.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             #If DEBUG Then\n        Assert.IsTrue(True)\n#End If\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_unclosed_conditional_keeps_abstaining() {
        // A `#If` with no `#End If` leaves every later declaration selected by
        // an unevaluated constant. Understating support is the safe direction.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_well_formed_class_reports_no_diagnostic() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert!(parsed.report.diagnostics.is_empty());
    }

    /// An XML literal may contain any text at all, including lines that read
    /// exactly like a declaration. Reading one as code is the same class of
    /// defect as anchoring both branches of a `#If`: it invents a member.
    #[test]
    fn an_xml_literal_never_yields_a_declaration() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub RealTest()\n\
             \x20       Dim markup = <config>\n\
             \x20           <TestMethod()>\n            Public Sub FakeTest()\n\
             \x20           End Sub\n        </config>\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0, "no declaration inside markup is real");
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_xml_literal".to_string()));
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn ordinary_comparisons_are_not_xml_literals() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub Compares()\n\
             \x20       Dim less = 1 < 2\n        Dim atMost = 1 <= 2\n\
             \x20       Dim unequal = 1 <> 2\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    /// `$"…"` is VB 14 and later. Under VB 10 to 13 the same characters lex as
    /// an operator and an ordinary string, so the token stream is not the same
    /// under every version this frontend admits.
    #[test]
    fn an_interpolated_string_is_outside_the_declared_version_set() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub Interpolates()\n\
             \x20       Dim note = $\"value {{1}}\"\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_interpolated_string".to_string()));
    }

    /// The property a scanner could not have: malformed VB now fails instead of
    /// silently yielding fewer anchors.
    #[test]
    fn an_unclosed_class_abstains_and_says_so() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_declaration_shape".to_string()));
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
    }

    #[test]
    fn a_mismatched_end_keyword_abstains() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20   End Function\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_declaration_shape".to_string()));
    }

    /// A multi-line lambda carries its own `End Sub`. Missing it would close
    /// the enclosing method early and give the anchor a range that stops in the
    /// middle of the declaration it names.
    #[test]
    fn a_multi_line_lambda_does_not_close_its_enclosing_method() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub UsesLambda()\n\
             \x20       Dim action = Sub()\n                         Assert.IsTrue(True)\n\
             \x20                    End Sub\n        action()\n    End Sub\n\
             \x20   <TestMethod()>\n    Public Sub Second()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 2);
    }

    #[test]
    fn exit_sub_is_not_a_lambda() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       If True Then Exit Sub\n        Assert.IsTrue(True)\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    /// Every block statement in the declared subset, so that none of them can
    /// be mistaken for the method's own `End`.
    #[test]
    fn block_statements_in_a_body_do_not_move_the_method_boundary() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub Complex()\n\
             \x20       If True Then\n            Assert.IsTrue(True)\n        End If\n\
             \x20       For index = 1 To 3\n            Assert.IsTrue(True)\n        Next\n\
             \x20       Try\n            Assert.IsTrue(True)\n        Catch error1 As Exception\n\
             \x20       Finally\n        End Try\n\
             \x20       Using scope = New Object()\n        End Using\n\
             \x20       Select Case 1\n            Case 1\n        End Select\n\
             \x20       Do\n            Exit Do\n        Loop\n\
             \x20       While False\n        End While\n\
             \x20       SyncLock Me\n        End SyncLock\n\
             \x20       With Me\n        End With\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_single_line_if_needs_no_end_if() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       If True Then Assert.IsTrue(True)\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    /// `Public Property TestContext As TestContext` is the most common member
    /// of a real MSTest class, and it has no `End Property`.
    #[test]
    fn an_auto_property_has_no_end_property() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   Public Property TestContext As TestContext\n\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_property_with_accessors_closes_at_end_property() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   Private _value As Integer\n\
             \x20   Public Property Value As Integer\n        Get\n            Return _value\n\
             \x20       End Get\n        Set(newValue As Integer)\n            _value = newValue\n\
             \x20       End Set\n    End Property\n\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn interface_and_must_override_members_declare_no_body() {
        let parsed = output(&format!(
            "{IMPORTED}Public Interface ICatalog\n    Sub Load()\n\
             \x20   Function Count() As Integer\nEnd Interface\n\
             Public MustInherit Class Base\n    Public MustOverride Sub Run()\nEnd Class\n\
             <TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    /// The attribute binds by grammar, not by line position, so the one-line
    /// form is the same declaration as the two-line one.
    #[test]
    fn an_attribute_on_the_declaration_line_binds_it() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()> Public Class CatalogTests\n\
             <TestMethod()> Public Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_test_class_inside_a_namespace_still_anchors() {
        let parsed = output(&format!(
            "{IMPORTED}Namespace Catalog.Tests\n<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\nEnd Namespace\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    /// `Option Strict`, `Option Explicit`, and `Option Infer` change binding,
    /// overload resolution, and inference. They change no token boundary and no
    /// declaration shape, so they cannot change what this frontend admits.
    #[test]
    fn option_statements_do_not_change_the_admitted_parse() {
        let with_options = output(&format!(
            "Option Strict On\nOption Explicit On\nOption Infer Off\nOption Compare Text\n\
             {IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        let without_options = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&with_options), methods(&without_options));
        assert_eq!(methods(&with_options), 1);
    }

    /// A `Partial` part that carries `<TestClass()>` proves the attribute for
    /// itself. A part without it understates rather than guessing, because this
    /// frontend never assembles a type across files.
    #[test]
    fn a_partial_part_anchors_only_when_it_carries_the_attribute() {
        let attributed = output(&format!(
            "{IMPORTED}<TestClass()>\nPartial Public Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&attributed), 1);
        let bare = output(&format!(
            "{IMPORTED}Partial Public Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub FiltersCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&bare), 0);
    }

    #[test]
    fn an_escaped_identifier_is_never_a_keyword() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Dim [class] As Integer = 1\n        Dim [end] As Integer = 2\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_date_literal_does_not_disturb_the_token_stream() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Dim stamp As Date = #1/15/2026 12:30:00 PM#\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn both_admitted_line_continuations_join_a_statement() {
        let explicit = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Assert.AreEqual( _\n            1, _\n            1)\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&explicit), 1);
        let implicit = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Assert.AreEqual(\n            1,\n            1)\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&implicit), 1);
    }

    /// A named abstention rather than a discovered one. VB 10 continues a line
    /// implicitly between LINQ query operators; this frontend does not admit
    /// that, so the query splits into statements, its `Select` clause reads as
    /// a `Select` block, and the file abstains. Safe, decidable, and a real
    /// recall cost.
    #[test]
    fn a_multi_line_query_expression_abstains() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Dim result = From item In items\n\
             \x20                    Where item > 1\n\
             \x20                    Select item\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_declaration_shape".to_string()));
    }

    #[test]
    fn parser_rejects_other_languages() {
        assert_eq!(
            VisualBasicMsTestParser.parse(SourceDocument {
                path: "tests/CatalogTests.vb",
                language: Language::CSharp,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                text: "",
            }),
            Err(ParseError::UnsupportedLanguage)
        );
    }
}
