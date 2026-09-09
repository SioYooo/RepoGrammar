//! Bounded Swift XCTest frontend for the ADR-0048 admitted shape.
//!
//! XCTest discovers tests by class shape wherever they are compiled, so like
//! VB.NET's MSTest and MATLAB's `matlab.unittest` there is no runner-defined
//! file set to narrow on. Every discovered `.swift` file is decoded and the
//! bound is what this frontend emits: a module unit, one unit per admitted
//! test class, and one per admitted test method. Ordinary declarations
//! produce no unit, so the absence of a unit is not evidence that a file has
//! no code.
//!
//! Nothing here invokes `swift`, `swiftc`, `swift-frontend`,
//! `sourcekit-lsp`, SwiftPM, Xcode, `xcodebuild`, a macro, a plugin, or a
//! package script. ADR-0025's execution prohibitions are carried forward.
//!
//! This module owns the claim, not the syntax. `super::syntax` decides what
//! the token stream is and refuses whatever falls outside the declared Swift
//! subset; everything here is the ADR-0048 D2 rule applied to the
//! declarations that survived. The separation is deliberate: a widened claim
//! and a widened grammar are different decisions and must not be made in the
//! same place.
//!
//! The anchor is deliberately narrower than XCTest's runtime contract:
//! `XCTestCase` identity here is the bare source-visible superclass spelling
//! bound by the file's own `import XCTest`, not the module-qualified
//! `XCTest.XCTestCase` identity ADR-0025 D3 reserves for a qualified
//! semantic verifier. What the anchor loses by that narrowing is stated as
//! non-claims rather than filled in.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use super::syntax;
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, SourceDocument,
    SourceParseOutput,
};

pub const SWIFT_ANCHOR_ENGINE: &str = "repogrammar-swift-xctest-parser";
pub const SWIFT_ANCHOR_METHOD: &str = "bounded_swift_xctest_declaration_v1";

/// Fixed support target for the one admitted exact anchor.
pub const SWIFT_TEST_METHOD_TARGET: &str = "swift.xctest.test_method";

const SWIFT_TEST_CLASS_TARGET: &str = "swift.xctest.test_class";
const XCODE_TEST_MODULE: &str = "XCTest";
const MAX_UNITS: usize = 4_096;

/// Product dispatch entrypoint: parse a Swift source document to a report.
pub fn parse_report(document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
    parse_output(document).map(|output| output.report)
}

/// Product dispatch entrypoint: parse a Swift source document to units,
/// facts, IR, and diagnostics. The project context carries no Swift project
/// model — ADR-0048 admits none — so it is deliberately unread.
pub fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Swift {
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
            "unit:{}#swift_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Swift,
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
            "swift_syntax_admission",
            "source_byte_limit",
            full_range,
            "Swift source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    // The parser abstains rather than recovering. Outside the declared subset
    // there is no partial tree to read, so the file keeps its module unit,
    // records why, and yields no anchor at all.
    let parsed = match syntax::parse_file(document.text) {
        Ok(parsed) => parsed,
        Err(refusal) => {
            facts.push(unknown_fact(
                &module,
                refusal_reason(refusal),
                refusal.claim(),
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

    // `XCTestCase` binds through the file's own import of the XCTest module.
    // A plain `import XCTest`, an `@testable import XCTest`, and a kinded
    // `import class XCTest.XCTestCase` all bind it; any other module does
    // not. Conditional imports cannot survive here: conditional compilation
    // refuses the file before this point.
    let imports_xctest = parsed
        .imports
        .iter()
        .any(|import| import.module == XCODE_TEST_MODULE);

    let mut scan = AnchorScan::default();
    scan_classes(&parsed.classes, imports_xctest, &mut scan);

    if parsed.free_test_function {
        // A free function is not discovered by XCTest no matter how it is
        // spelled, and reporting it keeps the missing anchor informative.
        scan.observations.insert(Observation::FreeTestFunction);
    }
    if !imports_xctest && scan.derives_without_import {
        scan.observations
            .insert(Observation::TestcaseWithoutXctestImport);
    }

    for group in scan.groups {
        if units.len() >= MAX_UNITS {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "swift_syntax_admission",
                "unit_resource_limit",
                module.range.clone(),
                "Swift parse exceeded the bounded unit limit",
            )?);
            break;
        }
        let class_unit = declaration_unit(
            &document,
            &provenance,
            CodeUnitKind::SwiftTestClass,
            group.class_start,
            group.class_end,
        )?;
        facts.push(anchor_fact(
            &class_unit,
            SWIFT_TEST_CLASS_TARGET,
            "swift_anchor_kind=xctest_test_class",
            "bounded Swift XCTestCase subclass anchor",
        )?);
        units.push(class_unit);
        for method in group.methods {
            if units.len() >= MAX_UNITS {
                break;
            }
            let unit = declaration_unit(
                &document,
                &provenance,
                CodeUnitKind::SwiftTestMethod,
                method.0,
                method.1,
            )?;
            facts.push(anchor_fact(
                &unit,
                SWIFT_TEST_METHOD_TARGET,
                "swift_anchor_kind=xctest_test_method",
                "bounded Swift XCTest test-method anchor",
            )?);
            units.push(unit);
        }
    }

    for observation in scan.observations {
        let (reason, claim, kind, note) = match observation {
            Observation::TestMethodsWithoutTestcaseBase => (
                UnknownReasonCode::UnresolvedImport,
                "swift_xctest_class_binding",
                "test_methods_without_testcase_base",
                "instance test-prefixed methods appear in a class that does not derive from XCTestCase, so the framework is unproven",
            ),
            Observation::TestcaseWithoutXctestImport => (
                UnknownReasonCode::UnresolvedImport,
                "swift_xctest_import_binding",
                "testcase_without_xctest_import",
                "a class derives from the bare XCTestCase spelling in a file with no XCTest import",
            ),
            Observation::StaticTestMethod => (
                UnknownReasonCode::InsufficientSupport,
                "swift_xctest_method_shape",
                "static_test_method",
                "a test-prefixed method in a test class is static or class-scoped, and XCTest discovers only instance methods",
            ),
            Observation::TestMethodWithParameters => (
                UnknownReasonCode::InsufficientSupport,
                "swift_xctest_method_shape",
                "test_method_with_parameters",
                "a test-prefixed method in a test class declares parameters, and an XCTest test method takes none",
            ),
            Observation::TestMethodNonVoidReturn => (
                UnknownReasonCode::InsufficientSupport,
                "swift_xctest_method_shape",
                "test_method_non_void_return",
                "a test-prefixed method in a test class returns a value, and an XCTest test method returns nothing",
            ),
            Observation::TestMethodRethrows => (
                UnknownReasonCode::InsufficientSupport,
                "swift_xctest_method_shape",
                "test_method_rethrows",
                "a test-prefixed method in a test class is declared rethrows, which is outside the admitted effect set",
            ),
            Observation::FreeTestFunction => (
                UnknownReasonCode::InsufficientSupport,
                "swift_xctest_method_shape",
                "free_test_function",
                "a free function at file level carries the test-method shape, and XCTest discovers only instance methods of a test class",
            ),
        };
        facts.push(unknown_fact(
            &module,
            reason,
            claim,
            kind,
            module.range.clone(),
            note,
        )?);
    }

    finish(units, facts, Vec::new())
}

/// The typed reason a refused file records.
///
/// A declared-set lexical divergence is `ConflictingFacts`, mirroring the
/// `matlab_dialect_invariance` shape: the token stream itself stops being
/// decidable across the Swift 5.x set. Conditional compilation is
/// `BuildVariantAmbiguity`, mirroring the VB.NET rule. Everything else is
/// `InsufficientSupport`: the construct is real Swift, and this frontend
/// simply does not admit it.
///
/// None of these block family membership, and none need to: a refused file
/// yields no anchor, so the abstention has already removed everything a block
/// could act on; making it blocking would only let one unadmitted file veto
/// declarations proven in another.
fn refusal_reason(refusal: syntax::Refusal) -> UnknownReasonCode {
    match refusal {
        syntax::Refusal::RegexLiteral => UnknownReasonCode::ConflictingFacts,
        syntax::Refusal::ConditionalCompilation => UnknownReasonCode::BuildVariantAmbiguity,
        _ => UnknownReasonCode::InsufficientSupport,
    }
}

/// The XCTest anchors found in a parsed file.
#[derive(Default)]
struct AnchorScan {
    groups: Vec<TestClassGroup>,
    /// Bounded observation kinds collected while scanning. The set keeps one
    /// fact per kind, so a file of lookalikes reports a bounded vocabulary.
    observations: std::collections::BTreeSet<Observation>,
    /// A class derived from the bare `XCTestCase` spelling while the file
    /// does not import XCTest.
    derives_without_import: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Observation {
    TestMethodsWithoutTestcaseBase,
    TestcaseWithoutXctestImport,
    StaticTestMethod,
    TestMethodWithParameters,
    TestMethodNonVoidReturn,
    TestMethodRethrows,
    FreeTestFunction,
}

struct TestClassGroup {
    class_start: usize,
    class_end: usize,
    methods: Vec<(usize, usize)>,
}

/// Apply the ADR-0048 D2 rule to the parsed declarations.
///
/// A class anchors only when its inheritance list carried the bare
/// `XCTestCase` spelling and the file imports XCTest; a class that does not
/// anchor is still examined for nested classes and for lookalike methods,
/// because a test class declared inside an ordinary one is still discovered
/// by XCTest.
fn scan_classes(declarations: &[syntax::ClassDecl], imports_xctest: bool, scan: &mut AnchorScan) {
    for class in declarations {
        let is_test_class = class.derives_xctestcase && imports_xctest;
        if class.derives_xctestcase && !imports_xctest {
            scan.derives_without_import = true;
        }
        let mut methods = Vec::new();
        for method in &class.methods {
            if !method.is_test_prefixed {
                continue;
            }
            if is_test_class {
                let anchor_shape = method.is_static
                    || !method.empty_params
                    || !method.void_return
                    || method.rethrows;
                if method.is_static {
                    scan.observations.insert(Observation::StaticTestMethod);
                } else if !method.empty_params {
                    scan.observations
                        .insert(Observation::TestMethodWithParameters);
                } else if !method.void_return {
                    scan.observations
                        .insert(Observation::TestMethodNonVoidReturn);
                } else if method.rethrows {
                    scan.observations.insert(Observation::TestMethodRethrows);
                }
                if !anchor_shape {
                    methods.push((method.start, method.end));
                }
            } else if !method.is_static && method.empty_params && method.void_return {
                // The method has the complete test shape and only the class
                // around it is unproven, which is the informative case.
                scan.observations
                    .insert(Observation::TestMethodsWithoutTestcaseBase);
            }
        }
        if is_test_class {
            scan.groups.push(TestClassGroup {
                class_start: class.start,
                class_end: class.end,
                methods,
            });
        }
        scan_classes(&class.nested, imports_xctest, scan);
    }
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
        language: Language::Swift,
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
            format!("swift_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: SWIFT_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: SWIFT_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
    use crate::adapters::parsing::RepoGrammarSourceParser;
    use crate::core::model::{ContentHash, RepositoryRevision};
    use crate::ports::framework_roles::FrameworkRoleDetector;
    use crate::ports::parser::{ParserProjectContext, SourceParser};

    fn output(path: &str, text: &str) -> SourceParseOutput {
        parse_output(document(path, text)).expect("parse Swift source")
    }

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::Swift,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn methods_found(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(SWIFT_TEST_METHOD_TARGET)
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
                    .strip_prefix("swift_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const EXACT_TESTS: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_exact_tests/CatalogTests.swift",
    );
    const SETUP_OVERRIDE: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_setup_override/CatalogTests.swift",
    );
    const THROWING_TESTS: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_throwing_tests/CatalogPersistenceTests.swift",
    );
    const LOOKALIKES: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_lookalikes/LookalikeTests.swift",
    );
    const CONDITIONAL: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_conditional/ConditionalTests.swift",
    );
    const DEGRADED: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_degraded/DegradedTests.swift",
    );
    const UNBOUND_IMPORT: &str = include_str!(
        "../../../../fixtures/swift/release/v0_2/xctest_unbound_import/UnboundTests.swift",
    );
    const LOW_SUPPORT: &str =
        include_str!("../../../../fixtures/swift/release/v0_2/xctest_low_support/SoloTest.swift",);

    // -- admissions: the shape ADR-0048 declares ------------------------------

    #[test]
    fn committed_exact_fixture_anchors_three_methods_and_not_the_helper() {
        let parsed = output("tests/CatalogTests.swift", EXACT_TESTS);
        assert_eq!(methods_found(&parsed), 3);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SwiftTestClass)
                .count(),
            1
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn an_overridden_setup_and_teardown_are_not_tests_but_do_not_cost_anchors() {
        let parsed = output("tests/CatalogTests.swift", SETUP_OVERRIDE);
        assert_eq!(methods_found(&parsed), 2);
        assert!(unknown_kinds(&parsed).is_empty());
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn throwing_test_methods_still_anchor() {
        let parsed = output("tests/CatalogPersistenceTests.swift", THROWING_TESTS);
        assert_eq!(methods_found(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn admitted_method_shapes_and_their_bounds() {
        let parsed = output(
            "tests/Shapes.swift",
            "import XCTest\nfinal class ShapeTests: XCTestCase {\n\
             \x20   private func testPrivate() { }\n\
             \x20   func testInternal() -> Void { }\n\
             \x20   func testUnitReturn() -> () { }\n\
             \x20   func testQualifiedVoid() -> Swift.Void { }\n\
             \x20   override func setUp() { }\n\
             \x20   func helper() { }\n\
             }\n",
        );
        assert_eq!(methods_found(&parsed), 4, "setUp and helper are not tests");
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_nested_test_class_inside_a_test_class_anchors_its_own_methods() {
        let parsed = output(
            "tests/Nested.swift",
            "import XCTest\nclass Outer: XCTestCase {\n\
             \x20   class Inner: XCTestCase {\n\
             \x20       func testInner() { }\n\
             \x20   }\n\
             \x20   func testOuter() { }\n\
             }\n",
        );
        assert_eq!(methods_found(&parsed), 2);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SwiftTestClass)
                .count(),
            2
        );
    }

    #[test]
    fn a_kind_specified_or_testable_import_still_binds_xctest() {
        for imports in [
            "import class XCTest.XCTestCase\n",
            "@testable import XCTest\n",
        ] {
            let parsed = output(
                "tests/Kinded.swift",
                &format!(
                    "{imports}class KindedTests: XCTestCase {{\n    func testKinded() {{ }}\n}}\n"
                ),
            );
            assert_eq!(methods_found(&parsed), 1, "{imports}");
        }
    }

    #[test]
    fn a_foreign_module_does_not_bind_the_superclass_spelling() {
        let parsed = output(
            "tests/Foreign.swift",
            "import CatalogKit\nclass ForeignTests: XCTestCase {\n    func testForeign() { }\n}\n",
        );
        assert_eq!(methods_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"testcase_without_xctest_import".to_string()));
    }

    // -- exclusions: the lookalikes that must not anchor ----------------------

    #[test]
    fn committed_lookalike_fixture_anchors_no_method_and_reports_why() {
        let parsed = output("tests/LookalikeTests.swift", LOOKALIKES);
        assert_eq!(methods_found(&parsed), 0);
        // `LookalikeTests` itself is a real XCTestCase subclass with no
        // anchorable method, so the class unit stands and says so.
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SwiftTestClass)
                .count(),
            1
        );
        let kinds = unknown_kinds(&parsed);
        assert!(kinds.contains(&"test_methods_without_testcase_base".to_string()));
        assert!(kinds.contains(&"static_test_method".to_string()));
        assert!(kinds.contains(&"test_method_with_parameters".to_string()));
        assert!(kinds.contains(&"free_test_function".to_string()));
    }

    #[test]
    fn static_class_and_non_void_test_prefixed_methods_do_not_anchor() {
        let parsed = output(
            "tests/Static.swift",
            "import XCTest\nfinal class StaticTests: XCTestCase {\n\
             \x20   static func testStatic() { }\n\
             \x20   class func testClassScoped() { }\n\
             \x20   func testReturnsValue() -> Int { 1 }\n\
             \x20   func testRethrowing() rethrows { }\n\
             \x20   func `testBackticked`() { }\n\
             \x20   func test() { }\n\
             \x20   func TestCapitalized() { }\n\
             \x20   func testReal() { }\n\
             }\n",
        );
        // `test` alone carries no following identifier character,
        // `TestCapitalized` misses the exact lowercase prefix, and the
        // backticked name is not the bare spelling.
        assert_eq!(methods_found(&parsed), 1);
        let kinds = unknown_kinds(&parsed);
        assert!(kinds.contains(&"static_test_method".to_string()));
        assert!(kinds.contains(&"test_method_non_void_return".to_string()));
        assert!(kinds.contains(&"test_method_rethrows".to_string()));
    }

    #[test]
    fn attribute_text_in_comments_and_strings_never_anchors() {
        let parsed = output(
            "tests/Prose.swift",
            "import XCTest\n// class FakeTests: XCTestCase {\n\
             \x20   let sample = \"class FakeTests: XCTestCase { func testFake() { } }\"\n\
             \x20   /* class Hidden: XCTestCase { func testHidden() { } } */\n\
             \x20   final class RealTests: XCTestCase {\n\
             \x20       func testReal() { }\n\
             \x20   }\n",
        );
        assert_eq!(methods_found(&parsed), 1);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SwiftTestClass)
                .count(),
            1
        );
    }

    #[test]
    fn an_extension_cannot_reopen_an_admitted_class() {
        let parsed = output(
            "tests/Extended.swift",
            "import XCTest\nfinal class ExtendedTests: XCTestCase {\n    func testOne() { }\n}\n\
             extension ExtendedTests {\n    func testAddedLater() { }\n}\n",
        );
        assert_eq!(
            methods_found(&parsed),
            1,
            "an extension cannot re-open a class this frontend admitted"
        );
    }

    // -- refusals: outside the declared subset --------------------------------

    #[test]
    fn committed_conditional_fixture_abstains_whole_file() {
        let parsed = output("tests/ConditionalTests.swift", CONDITIONAL);
        assert_eq!(methods_found(&parsed), 0);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("BuildVariantAmbiguity")
        }));
        assert!(unknown_kinds(&parsed).contains(&"conditional_compilation_region".to_string()));
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
    }

    #[test]
    fn committed_degraded_fixture_abstains_with_a_typed_refusal() {
        let parsed = output("tests/DegradedTests.swift", DEGRADED);
        assert_eq!(methods_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unadmitted_attribute_argument".to_string()));
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
    }

    #[test]
    fn committed_unbound_fixture_anchors_nothing_and_records_the_binding() {
        let parsed = output("tests/UnboundTests.swift", UNBOUND_IMPORT);
        assert_eq!(methods_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"testcase_without_xctest_import".to_string()));
    }

    #[test]
    fn committed_low_support_fixture_yields_two_anchors() {
        let parsed = output("tests/SoloTest.swift", LOW_SUPPORT);
        assert_eq!(methods_found(&parsed), 2);
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn a_regex_position_records_a_dialect_invariance_conflict() {
        let parsed = output(
            "tests/Regex.swift",
            "import XCTest\nfinal class RegexTests: XCTestCase {\n\
             \x20   func testMatches() {\n\
             \x20       let pattern = /ab+c/\n\
             \x20   }\n\
             }\n",
        );
        assert_eq!(methods_found(&parsed), 0);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .contains(&"affected_claim=swift_dialect_invariance".to_string())
        }));
    }

    #[test]
    fn an_unbalanced_or_hostile_file_degrades_instead_of_panicking() {
        for text in [
            "import XCTest\nclass A: XCTestCase {\n",
            "import XCTest\n}\n",
            "import XCTest\nclass A {\n    var computed: Int {\n        1\n    }\n}\n",
            "class A: XCTestCase {\n",
            "",
        ] {
            let parsed = output("tests/Hostile.swift", text);
            assert_eq!(methods_found(&parsed), 0, "{text:?}");
            assert!(!parsed.report.units.is_empty(), "module unit survives");
        }

        // Ten thousand nested closures in one body are consumed iteratively;
        // ten thousand nested classes hit the recursion bound and abstain.
        let deep_body = format!(
            "import XCTest\nfinal class Deep: XCTestCase {{\n    func testDeep() {{\n{}{}\n    }}\n}}\n",
            "        if true {\n".repeat(4_000),
            "        }\n".repeat(4_000),
        );
        let parsed = output("tests/Deep.swift", &deep_body);
        assert_eq!(methods_found(&parsed), 1);

        let deep_classes = format!(
            "import XCTest\nfinal class Deep: XCTestCase {{\n{}{}\n}}\n",
            "    class Inner {\n".repeat(4_000),
            "    }\n".repeat(4_000),
        );
        let parsed = output("tests/DeepClasses.swift", &deep_classes);
        assert_eq!(methods_found(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"parser_resource_limit".to_string()));
    }

    #[test]
    fn a_well_formed_class_reports_no_diagnostic() {
        let parsed = output(
            "tests/Plain.swift",
            "import XCTest\nfinal class PlainTests: XCTestCase {\n    func testPlain() { }\n}\n",
        );
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output("tests/CatalogTests.swift", EXACT_TESTS);
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    /// Every refusal and observation names a bounded class and never copies
    /// the source that triggered it.
    #[test]
    fn an_abstention_never_copies_the_divergent_source_text() {
        let parsed = output(
            "tests/Private.swift",
            "import XCTest\nfinal class PrivateSecrets: XCTestCase {\n\
             \x20   func testLoadsSecretVault() {\n\
             \x20       let pattern = /open-sesame/\n\
             \x20   }\n\
             }\n",
        );
        for fact in &parsed.report.semantic_facts {
            let rendered = format!("{:?}{:?}", fact.assumptions, fact.evidence.note);
            assert!(!rendered.contains("open-sesame"), "{rendered}");
            assert!(!rendered.contains("SecretVault"), "{rendered}");
        }
        for diagnostic in &parsed.report.diagnostics {
            assert!(
                !diagnostic.message.contains("open-sesame"),
                "{diagnostic:?}"
            );
        }
    }

    #[test]
    fn only_swift_sources_are_admitted() {
        let document = SourceDocument {
            path: "Package.swift",
            language: Language::SwiftConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text: "",
        };
        assert_eq!(parse_output(document), Err(ParseError::UnsupportedLanguage));
    }

    // -- product readiness, at the module boundary ----------------------------

    /// Gate 7 evidence at the module boundary: the product dispatch path and
    /// the framework-role detector expose only bounded tokens, states,
    /// counts, and provenance over both an anchored workspace and an unbound
    /// one. The assertions are non-vacuous: the positive workspace must
    /// really yield the anchor and its framework role, and the unbound one
    /// must really yield the lane's typed `UNKNOWN`.
    #[test]
    fn readiness_surfaces_stay_source_free_and_low_cardinality() {
        let parser = RepoGrammarSourceParser::default();
        let positive = parser
            .parse_with_context_output(
                document("tests/CatalogTests.swift", EXACT_TESTS),
                &ParserProjectContext::default(),
            )
            .expect("product parser routes Swift source to the xctest frontend");
        assert_eq!(methods_found(&positive), 3);
        let unbound = parser
            .parse_with_context_output(
                document("tests/UnboundTests.swift", UNBOUND_IMPORT),
                &ParserProjectContext::default(),
            )
            .expect("product parser routes the unbound workspace");
        assert_eq!(methods_found(&unbound), 0);
        assert!(unknown_kinds(&unbound).contains(&"testcase_without_xctest_import".to_string()));

        let role_facts = SyntaxFrameworkRoleDetector
            .detect_roles(&positive.report.units)
            .expect("detect roles");
        assert!(role_facts.iter().any(|fact| {
            fact.target
                .as_ref()
                .is_some_and(|target| target.as_str() == "framework:xctest.test")
        }));

        let markers = [
            "CatalogTests",
            "testLoadsCatalog",
            "testFiltersCatalog",
            "testSortsCatalog",
            "XCTAssertEqual",
            "import XCTest",
            "/Users/",
            "..",
        ];
        for parsed in [&positive, &unbound] {
            for fact in &parsed.report.semantic_facts {
                let rendered = format!(
                    "{:?}{:?}{:?}",
                    fact.assumptions, fact.evidence.note, fact.target
                );
                for marker in markers {
                    assert!(!rendered.contains(marker), "leaked {marker}: {rendered}");
                }
            }
            for diagnostic in &parsed.report.diagnostics {
                for marker in markers {
                    assert!(
                        !diagnostic.message.contains(marker),
                        "diagnostic leaked {marker}"
                    );
                }
            }
        }
        for fact in &role_facts {
            let rendered = format!("{:?}{:?}", fact.assumptions, fact.evidence.note);
            for marker in markers {
                assert!(
                    !rendered.contains(marker),
                    "role leaked {marker}: {rendered}"
                );
            }
        }

        // The fixed vocabulary: every note and assumption this frontend can
        // emit comes from the bounded sets declared above.
        let notes = positive
            .report
            .semantic_facts
            .iter()
            .map(|fact| fact.evidence.note.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            notes,
            [
                "bounded Swift XCTest test-method anchor",
                "bounded Swift XCTestCase subclass anchor"
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        );
    }

    #[test]
    fn parse_output_is_deterministic() {
        let first = output("tests/CatalogTests.swift", EXACT_TESTS);
        let second = output("tests/CatalogTests.swift", EXACT_TESTS);
        assert_eq!(
            format!("{:?}", first.report.semantic_facts),
            format!("{:?}", second.report.semantic_facts)
        );
        assert_eq!(first.report.units, second.report.units);
    }

    #[test]
    fn a_class_unit_contains_its_method_units_in_ir() {
        let parsed = output("tests/CatalogTests.swift", EXACT_TESTS);
        let node_for_unit = parsed
            .report
            .ir_nodes
            .iter()
            .map(|node| {
                (
                    node.code_unit_id.as_str().to_string(),
                    node.id.as_str().to_string(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let class_node = parsed
            .report
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::SwiftTestClass)
            .expect("test class unit")
            .id
            .as_str();
        let method_nodes = parsed
            .report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::SwiftTestMethod)
            .filter_map(|unit| node_for_unit.get(unit.id.as_str()))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(method_nodes.len(), 3);
        let contained = parsed
            .report
            .ir_edges
            .iter()
            .filter(|edge| {
                edge.from_node_id.as_str() == node_for_unit[class_node]
                    && method_nodes.contains(edge.to_node_id.as_str())
            })
            .count();
        assert_eq!(
            contained, 3,
            "each anchored method is contained by its test class"
        );
    }
}
