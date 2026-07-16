use super::*;
use crate::core::model::{ContentHash, IrEdgeLabel, RepositoryRevision};

fn parse(text: &str) -> ParseReport {
    let document = SourceDocument {
        path: "src/main/java/com/example/DemoController.java",
        language: Language::Java,
        content_hash: ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash"),
        repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
        text,
    };
    JavaSyntaxParser.parse(document).expect("parse java")
}

fn unit_kinds(report: &ParseReport) -> Vec<&'static str> {
    report.units.iter().map(|unit| unit.kind.as_str()).collect()
}

fn targets(report: &ParseReport) -> Vec<String> {
    let mut targets = report
        .semantic_facts
        .iter()
        .filter(|fact| fact.kind != SemanticFactKind::Unknown)
        .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
        .collect::<Vec<_>>();
    targets.sort();
    targets
}

fn unknown_targets(report: &ParseReport) -> Vec<String> {
    report
        .semantic_facts
        .iter()
        .filter(|fact| fact.kind == SemanticFactKind::Unknown)
        .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
        .collect()
}

fn unknown_affected_claims(report: &ParseReport) -> Vec<String> {
    let mut claims = report
        .semantic_facts
        .iter()
        .filter(|fact| fact.kind == SemanticFactKind::Unknown)
        .flat_map(|fact| fact.assumptions.iter())
        .filter_map(|assumption| assumption.strip_prefix("affected_claim="))
        .map(str::to_string)
        .collect::<Vec<_>>();
    claims.sort();
    claims
}

fn assumption_values(report: &ParseReport, prefix: &str) -> Vec<String> {
    report
        .semantic_facts
        .iter()
        .filter(|fact| fact.kind != SemanticFactKind::Unknown)
        .flat_map(|fact| fact.assumptions.iter())
        .filter_map(|assumption| assumption.strip_prefix(prefix))
        .map(str::to_string)
        .collect()
}

fn route_assumption_values(report: &ParseReport, prefix: &str) -> Vec<String> {
    report
        .semantic_facts
        .iter()
        .filter(|fact| fact.kind != SemanticFactKind::Unknown)
        .filter(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "java_anchor_kind=spring_mvc_route")
        })
        .flat_map(|fact| fact.assumptions.iter())
        .filter_map(|assumption| assumption.strip_prefix(prefix))
        .map(str::to_string)
        .collect()
}

#[test]
fn extracts_spring_mvc_routes_only_inside_exact_controller_classes() {
    let report = parse(
        r#"
package com.example;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/books")
public class DemoController {
    @GetMapping("/{id}")
    public String show(String id) {
        return id;
    }
}
"#,
    );

    assert!(unit_kinds(&report).contains(&"spring_component"));
    assert!(unit_kinds(&report).contains(&"spring_mvc_route"));
    assert_eq!(
        targets(&report),
        vec![
            "spring.web.bind.annotation.GetMapping".to_string(),
            "spring.web.bind.annotation.RestController".to_string()
        ]
    );
    assert!(report.ir_edges.iter().any(|edge| {
        edge.label == IrEdgeLabel::Contains
            && edge
                .from_node_id
                .as_str()
                .contains("spring_component:democontroller")
            && edge.to_node_id.as_str().contains("spring_mvc_route:show")
    }));
}

#[test]
fn route_annotation_without_controller_stays_unknown() {
    let report = parse(
        r#"
package com.example;

public class Utility {
    @org.springframework.web.bind.annotation.GetMapping("/accidental")
    public String accidental() {
        return "no";
    }
}
"#,
    );

    assert!(unit_kinds(&report).contains(&"method"));
    assert!(!unit_kinds(&report).contains(&"spring_mvc_route"));
    assert_eq!(unknown_targets(&report), vec!["FrameworkMagic".to_string()]);
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.assumptions
            .iter()
            .any(|assumption| assumption == "affected_claim=java_spring_controller_identity")
    }));
}

#[test]
fn extracts_boot_application_and_jpa_repository_anchors() {
    let app = parse(
        r#"
package com.example;

import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class DemoApplication {
    public static void main(String[] args) {
        SpringApplication.run(DemoApplication.class, args);
    }
}
"#,
    );
    assert!(unit_kinds(&app).contains(&"spring_boot_application"));
    assert!(targets(&app).contains(&"spring.boot.autoconfigure.SpringBootApplication".to_string()));
    assert!(app.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_spring_component_scan")
    }));

    let repository = parse(
        r#"
package com.example;

import org.springframework.data.jpa.repository.JpaRepository;

interface BookRepository extends JpaRepository<Book, Long> {
}
"#,
    );
    assert!(unit_kinds(&repository).contains(&"spring_data_repository"));
    assert_eq!(
        targets(&repository),
        vec!["spring.data.jpa.repository.JpaRepository".to_string()]
    );
    assert!(unknown_affected_claims(&repository)
        .contains(&"java_spring_generated_repository".to_string()));
}

#[test]
fn spring_components_emit_runtime_unknown_subclaims() {
    let report = parse(
        r#"
package com.example;

import org.springframework.stereotype.Service;

@Service
public class BookService {
}
"#,
    );

    assert!(unit_kinds(&report).contains(&"spring_component"));
    let claims = unknown_affected_claims(&report);
    assert!(claims.contains(&"java_spring_component_scan".to_string()));
    assert!(claims.contains(&"java_spring_dependency_injection".to_string()));
    assert!(claims.contains(&"java_spring_proxy_semantics".to_string()));
}

#[test]
fn dynamic_route_paths_are_non_exact_unknown_subclaims() {
    let report = parse(
        r#"
package com.example;

import org.springframework.stereotype.Controller;
import org.springframework.web.bind.annotation.RequestMapping;

@Controller
class DynamicController {
    @RequestMapping(value = Routes.SHOW, method = RequestMethod.GET)
    String show() {
        return "ok";
    }
}
"#,
    );

    assert!(unit_kinds(&report).contains(&"spring_mvc_route"));
    assert!(targets(&report).contains(&"spring.web.bind.annotation.RequestMapping".to_string()));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_spring_route_path")
    }));
}

#[test]
fn route_path_shape_classifies_only_pure_literals_as_literal() {
    let report = parse(
        r#"
package com.example;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
class ShapeController {
    static final String VERSION = "v1";

    @GetMapping("/x")
    String literal() {
        return "ok";
    }

    @GetMapping(Paths.X)
    String constant() {
        return "ok";
    }

    @GetMapping("/api/" + VERSION)
    String concatenated() {
        return "ok";
    }

    @GetMapping({"/a", Paths.B})
    String mixedArray() {
        return "ok";
    }
}
"#,
    );

    assert_eq!(
        route_assumption_values(&report, "route_path_shape="),
        vec![
            "literal".to_string(),
            "dynamic".to_string(),
            "dynamic".to_string(),
            "dynamic".to_string(),
        ]
    );
    assert_eq!(
        unknown_affected_claims(&report)
            .into_iter()
            .filter(|claim| claim == "java_spring_route_path")
            .count(),
        3
    );
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "java_unknown_kind=non_literal_route_path")
    }));
}

#[test]
fn spring_lookalike_simple_annotations_without_import_are_unresolved_unknowns() {
    let report = parse(
        r#"
package com.example;

@RestController
class LookalikeController {
    @GetMapping("/books")
    String list() {
        return "ok";
    }
}
"#,
    );

    assert!(!unit_kinds(&report).contains(&"spring_component"));
    assert!(!unit_kinds(&report).contains(&"spring_mvc_route"));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "UnresolvedImport"
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_spring_annotation_binding")
    }));
}

// --- JUnit 5 --------------------------------------------------------------

#[test]
fn extracts_junit5_test_methods_with_exact_imports() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.api.Test;

class CatalogTest {
    @Test
    void loads() {}

    @Test
    void reads() {}
}
"#,
    );
    assert_eq!(
        unit_kinds(&report)
            .into_iter()
            .filter(|kind| *kind == "junit5_test_method")
            .count(),
        2
    );
    assert_eq!(
        targets(&report),
        vec![
            "junit.jupiter.Test".to_string(),
            "junit.jupiter.Test".to_string()
        ]
    );
    assert_eq!(
        assumption_values(&report, "test_annotation="),
        vec!["Test".to_string(), "Test".to_string()]
    );
}

#[test]
fn junit5_parameterized_method_source_external_emits_non_blocking_unknown() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class ParamTest {
    @ParameterizedTest
    @MethodSource("com.example.Factory#values")
    void checks(int value) {}
}
"#,
    );
    assert!(unit_kinds(&report).contains(&"junit5_test_method"));
    assert!(assumption_values(&report, "test_data_shape=")
        .contains(&"method_source_external".to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
}

#[test]
fn junit5_local_method_source_emits_bounded_binding_for_literal_and_default_names() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class LocalParamTest {
    @ParameterizedTest
    @MethodSource("localValues")
    void checks(int value) {}

    static int[] localValues() { return new int[] { 1 }; }

    @ParameterizedTest
    @MethodSource
    void byConvention(int value) {}

    static int[] byConvention() { return new int[] { 2 }; }
}
"#,
    );
    assert!(
        assumption_values(&report, "test_data_shape=").contains(&"method_source_local".to_string())
    );
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert_eq!(
        targets(&report)
            .iter()
            .filter(|target| target.as_str() == test_data::JUNIT_METHOD_SOURCE_TARGET)
            .count(),
        2
    );
    assert!(assumption_values(&report, "test_data_reference=")
        .contains(&"default_same_name".to_string()));
}

#[test]
fn junit6_repeatable_and_array_method_sources_resolve_only_as_complete_sets() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class MultipleSourceTest {
    @ParameterizedTest
    @MethodSource("firstRows")
    @MethodSource("secondRows")
    void repeated(int value) {}

    @ParameterizedTest
    @MethodSource({"secondRows", "firstRows"})
    void array(int value) {}

    static int[] firstRows() { return new int[] { 1 }; }
    static int[] secondRows() { return new int[] { 2 }; }
}
"#,
    );

    assert_eq!(
        targets(&report)
            .iter()
            .filter(|target| target.as_str() == test_data::JUNIT_METHOD_SOURCE_TARGET)
            .count(),
        2
    );
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert_eq!(
        assumption_values(&report, "test_data_provider_name=")
            .into_iter()
            .filter(|value| value == "firstRows|secondRows")
            .count(),
        2
    );
}

#[test]
fn method_source_is_resolved_even_when_other_repeatable_sources_are_present() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;
import org.junit.jupiter.params.provider.MethodSource;
import org.junit.jupiter.params.provider.ValueSource;

class MixedSourceKindsTest {
    @ParameterizedTest
    @ValueSource(ints = {1})
    @CsvSource("2")
    @MethodSource("rows")
    void mixed(int value) {}

    static int[] rows() { return new int[] { 3 }; }
}
"#,
    );

    assert!(targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(
        assumption_values(&report, "test_data_shape=").contains(&"multiple_sources".to_string())
    );
}

#[test]
fn mixed_identity_or_partially_missing_method_source_sets_stay_unknown() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class IncompleteMultipleSourceTest {
    @ParameterizedTest
    @MethodSource("firstRows")
    @fake.MethodSource("secondRows")
    void mixedIdentity(int value) {}

    @ParameterizedTest
    @MethodSource({"firstRows", "missingRows"})
    void partiallyMissing(int value) {}

    static int[] firstRows() { return new int[] { 1 }; }
    static int[] secondRows() { return new int[] { 2 }; }
}
"#,
    );

    assert_eq!(
        unknown_affected_claims(&report)
            .iter()
            .filter(|claim| claim.as_str() == "java_test_method_source")
            .count(),
        2
    );
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    for kind in [
        "unresolved_method_source_annotation",
        "missing_local_method_source",
    ] {
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("java_unknown_kind={kind}"))
        }));
    }
}

#[test]
fn parse_degraded_test_data_annotations_never_discharge_provider_unknowns() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class BrokenDataAnnotations {
    @ParameterizedTest
    @MethodSource("junitRows"
    void junit(int value) {}

    static int[] junitRows() { return new int[] { 1 }; }

    @DataProvider(name = "testngRows")
    Object[][] testngRows() { return new Object[][] { { 2 } }; }

    @Test(dataProvider = "testngRows"
    void testng(int value) {}
}
"#,
    );

    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
}

#[test]
fn fully_qualified_junit_and_testng_data_annotations_resolve_in_the_same_class() {
    let report = parse(
        r#"
package com.example;

class FullyQualifiedDataTest {
    @org.junit.jupiter.params.ParameterizedTest
    @org.junit.jupiter.params.provider.MethodSource("junitRows")
    void junit(int value) {}

    static int[] junitRows() { return new int[] { 1 }; }

    @org.testng.annotations.DataProvider(name = "testngRows")
    Object[][] data() { return new Object[][] { { 2 } }; }

    @org.testng.annotations.Test(dataProvider = "testngRows")
    void testng(int value) {}
}
"#,
    );
    assert!(targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(!unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
}

#[test]
fn junit5_method_source_missing_dynamic_non_static_duplicate_and_inherited_names_stay_unknown() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class AmbiguousParamTest {
    static final String SOURCE = "dynamicValues";

    @ParameterizedTest
    @MethodSource("missingValues")
    void missing(int value) {}

    @ParameterizedTest
    @MethodSource(SOURCE)
    void dynamic(int value) {}

    @ParameterizedTest
    @MethodSource("dynamic" + "Values")
    void concatenated(int value) {}

    @ParameterizedTest
    @MethodSource("instanceValues")
    void nonStatic(int value) {}

    int[] instanceValues() { return new int[] { 1 }; }

    @ParameterizedTest
    @MethodSource("duplicateValues")
    void duplicate(int value) {}

    static int[] duplicateValues() { return new int[] { 1 }; }
    static int[] duplicateValues(int ignored) { return new int[] { ignored }; }
}

class BaseParamTest {
    static int[] inheritedValues() { return new int[] { 1 }; }
}

class ChildParamTest extends BaseParamTest {
    @ParameterizedTest
    @MethodSource("inheritedValues")
    void inherited(int value) {}
}
"#,
    );
    assert_eq!(
        unknown_affected_claims(&report)
            .iter()
            .filter(|claim| claim.as_str() == "java_test_method_source")
            .count(),
        6
    );
    assert!(unknown_targets(&report).contains(&"ConflictingFacts".to_string()));
    for kind in [
        "missing_local_method_source",
        "dynamic_or_ambiguous_method_source",
        "non_static_local_method_source",
        "ambiguous_local_method_source",
    ] {
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("java_unknown_kind={kind}"))
        }));
    }
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn junit5_method_source_without_exact_import_stays_typed_unknown() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;

class UnknownMethodSourceIdentity {
    @ParameterizedTest
    @MethodSource("rows")
    void runs(int value) {}

    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );
    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "UnresolvedImport"
            && fact.assumptions.iter().any(|assumption| {
                assumption == "java_unknown_kind=unresolved_method_source_annotation"
            })
    }));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn method_source_on_plain_junit_test_never_resolves_a_provider_binding() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.provider.MethodSource;

class InvalidMethodSourceUse {
    @Test
    @MethodSource("rows")
    void runs(int value) {}

    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );
    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.assumptions.iter().any(|assumption| {
                assumption == "java_unknown_kind=method_source_without_parameterized_test"
            })
    }));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn junit5_method_source_does_not_cross_nested_or_anonymous_class_boundaries() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

class OuterParamTest {
    static int[] outerValues() { return new int[] { 1 }; }

    class InnerParamTest {
        @ParameterizedTest
        @MethodSource("outerValues")
        void nested(int value) {}
    }

    Object anonymous = new Object() {
        @ParameterizedTest
        @MethodSource("outerValues")
        void anonymous(int value) {}
    };
}
"#,
    );
    assert_eq!(
        unknown_affected_claims(&report)
            .iter()
            .filter(|claim| claim.as_str() == "java_test_method_source")
            .count(),
        2
    );
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn junit_method_source_does_not_cross_enum_constant_class_body_boundaries() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

enum EnumParamTest {
    CONSTANT {
        @ParameterizedTest
        @MethodSource("outerRows")
        void nested(int value) {}
    };

    static int[] outerRows() { return new int[] { 1 }; }
}
"#,
    );

    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn same_enum_method_source_uses_the_enum_body_declaration_inventory() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

enum EnumParamTest {
    CONSTANT;

    @ParameterizedTest
    @MethodSource("rows")
    void checks(int value) {}

    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );

    assert!(targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
}

#[test]
fn wildcard_colliding_and_locally_shadowed_test_data_annotations_never_resolve() {
    let sources = [
        r#"
package com.example;
import org.junit.jupiter.params.*;
import org.junit.jupiter.params.provider.MethodSource;
class WildcardParameterizedTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.*;
class WildcardMethodSourceTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
@interface MethodSource { String value(); }
class ShadowedMethodSourceTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;
@interface DataProvider { String name(); }
class ShadowedDataProviderTest {
    @DataProvider(name = "rows") Object[][] rows() { return new Object[][] { { 1 } }; }
    @Test(dataProvider = "rows") void checks(int value) {}
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import other.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
class CollidingImportTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
/*
import org.junit.jupiter.params.provider.MethodSource;
*/
class CommentImportTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
class TextBlockImportTest {
    String decoy = """
        import org.junit.jupiter.params.provider.MethodSource;
        """;
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource
class MalformedImportTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
    ];

    for source in sources {
        let report = parse(source);
        assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
        assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
        assert!(unknown_affected_claims(&report).iter().any(|claim| {
            matches!(
                claim.as_str(),
                "java_test_method_source" | "java_testng_data_provider"
            )
        }));
    }
}

#[test]
fn mixed_junit_parameterized_and_testng_annotations_are_conflicting() {
    let report = parse(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.Test;
class MixedFrameworkTest {
    @ParameterizedTest
    @MethodSource("rows")
    @Test(dataProvider = "testngRows")
    void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );

    assert!(unknown_affected_claims(&report).contains(&"java_test_annotation_binding".to_string()));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
}

#[test]
fn malformed_competing_member_keeps_the_class_inventory_open() {
    let report = parse(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
class BrokenInventoryTest {
    @ParameterizedTest @MethodSource("rows") void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
    static int[] rows( { return new int[] { 2 }; }
}
"#,
    );

    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
}

#[test]
fn comments_between_annotation_name_and_arguments_preserve_real_argument_identity() {
    let report = parse(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;
class AnnotationTriviaTest {
    @ParameterizedTest
    @MethodSource /* source name follows */ ("missingRows")
    void byConvention(int value) {}
    static int[] byConvention() { return new int[] { 1 }; }

    @DataProvider /* explicit provider name */ (name = "actualRows")
    Object[][] methodName() { return new Object[][] { { 2 } }; }
    @Test(dataProvider = "methodName")
    void testng(int value) {}
}
"#,
    );

    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
}

#[test]
fn comments_around_testng_assignment_names_preserve_provider_identity() {
    let resolved = parse(
        r#"
package com.example;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;
class CommentedAssignments {
    @DataProvider(name /* declaration trivia */ = "actualRows")
    Object[][] methodName() { return new Object[][] { { 1 } }; }
    @Test(dataProvider /* consumer trivia */ = "actualRows")
    void checks(int value) {}
}
"#,
    );
    assert!(targets(&resolved).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
    assert!(!unknown_affected_claims(&resolved).contains(&"java_testng_data_provider".to_string()));

    let wrong_default = parse(
        r#"
package com.example;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;
class CommentedProviderName {
    @DataProvider(name /* declaration trivia */ = "actualRows")
    Object[][] methodName() { return new Object[][] { { 1 } }; }
    @Test(dataProvider = "methodName")
    void checks(int value) {}
}
"#,
    );
    assert!(!targets(&wrong_default).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
    assert!(
        unknown_affected_claims(&wrong_default).contains(&"java_testng_data_provider".to_string())
    );
}

#[test]
fn type_level_method_source_is_retained_as_typed_unknown() {
    let report = parse(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedClass;
import org.junit.jupiter.params.provider.MethodSource;
@ParameterizedClass
@MethodSource("rows")
class ParameterizedClassLike {
    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );

    assert!(unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "java_unknown_kind=type_level_method_source")
    }));
}

#[test]
fn adversarial_nested_repeatable_method_sources_abstain_with_bounded_work() {
    let annotations =
        "@org.junit.jupiter.params.provider.MethodSource(value = \"rows\")\n".repeat(65);

    assert_eq!(
        test_data::method_source_reference(annotations.as_str(), &JavaImportContext::default()),
        Some(test_data::MethodSourceReference::Unsupported)
    );
}

#[test]
fn nested_annotation_values_and_escaped_text_blocks_never_bind_method_sources() {
    let nested = parse(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
@interface Wrapper { MethodSource value(); }
class NestedAnnotationValueTest {
    @Wrapper(@MethodSource("rows"))
    @ParameterizedTest
    void checks(int value) {}
    static int[] rows() { return new int[] { 1 }; }
}
"#,
    );
    assert!(!targets(&nested).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));

    let text_block = r#"@Note("""
        \""" @org.junit.jupiter.params.provider.MethodSource("rows")
        """)"#;
    assert_eq!(
        test_data::method_source_reference(text_block, &JavaImportContext::default()),
        None
    );
}

#[test]
fn junit4_and_testng_test_methods_are_distinct_roles() {
    let junit4 = parse(
        r#"
package com.example;

import org.junit.Test;

class LegacyTest {
    @Test
    public void runs() {}
}
"#,
    );
    assert!(unit_kinds(&junit4).contains(&"junit4_test_method"));
    assert_eq!(targets(&junit4), vec!["junit4.Test".to_string()]);

    let testng = parse(
        r#"
package com.example;

import org.testng.annotations.Test;

class NgTest {
    @Test(dataProvider = "rows")
    public void runs() {}
}
"#,
    );
    assert!(unit_kinds(&testng).contains(&"testng_test_method"));
    assert_eq!(
        targets(&testng),
        vec!["testng.annotations.Test".to_string()]
    );
    assert!(unknown_affected_claims(&testng).contains(&"java_testng_data_provider".to_string()));
}

#[test]
fn testng_local_data_provider_emits_bounded_binding_for_named_and_default_providers() {
    let report = parse(
        r#"
package com.example;

import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class NgTest {
    @DataProvider(name = "rows")
    Object[][] namedRows() { return new Object[][] { { 1 } }; }

    @Test(dataProvider = "rows")
    public void named(int value) {}

    @DataProvider
    Object[][] defaultRows() { return new Object[][] { { 2 } }; }

    @Test(dataProvider = "defaultRows")
    public void defaultName(int value) {}
}
"#,
    );
    assert_eq!(
        targets(&report)
            .iter()
            .filter(|target| target.as_str() == test_data::TESTNG_DATA_PROVIDER_TARGET)
            .count(),
        2
    );
    assert!(!unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
}

#[test]
fn testng_external_dynamic_missing_duplicate_inherited_and_nested_providers_stay_unknown() {
    let report = parse(
        r#"
package com.example;

import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class NgTest {
    static final String PROVIDER = "dynamicRows";

    @Test(dataProvider = "external", dataProviderClass = ExternalProviders.class)
    void external(int value) {}

    @Test(dataProvider = PROVIDER)
    void dynamic(int value) {}

    @Test(dataProvider = "dynamic" + "Rows")
    void concatenated(int value) {}

    @Test(dataProvider = "missing")
    void missing(int value) {}

    @DataProvider(name = "duplicate")
    Object[][] first() { return new Object[][] { { 1 } }; }

    @DataProvider(name = "duplicate")
    Object[][] second() { return new Object[][] { { 2 } }; }

    @Test(dataProvider = "duplicate")
    void duplicate(int value) {}

    class Inner {
        @DataProvider(name = "innerRows")
        Object[][] rows() { return new Object[][] { { 3 } }; }
    }

    @Test(dataProvider = "innerRows")
    void nested(int value) {}
}

class BaseNgTest {
    @DataProvider(name = "inheritedRows")
    Object[][] rows() { return new Object[][] { { 4 } }; }
}

class ChildNgTest extends BaseNgTest {
    @Test(dataProvider = "inheritedRows")
    void inherited(int value) {}
}
"#,
    );
    assert_eq!(
        unknown_affected_claims(&report)
            .iter()
            .filter(|claim| claim.as_str() == "java_testng_data_provider")
            .count(),
        7
    );
    assert!(unknown_targets(&report).contains(&"ConflictingFacts".to_string()));
    for kind in [
        "external_testng_data_provider",
        "dynamic_testng_data_provider",
        "missing_local_testng_data_provider",
        "ambiguous_local_testng_data_provider",
    ] {
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("java_unknown_kind={kind}"))
        }));
    }
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
}

#[test]
fn testng_data_provider_lookalike_does_not_resolve() {
    let report = parse(
        r#"
package com.example;

import org.testng.annotations.Test;

class NgTest {
    @DataProvider(name = "rows")
    Object[][] rows() { return new Object[][] { { 1 } }; }

    @Test(dataProvider = "rows")
    void runs(int value) {}
}
"#,
    );
    assert!(unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
}

#[test]
fn annotation_names_inside_literals_or_comments_never_prove_test_data_bindings() {
    let report = parse(
        r##"
package com.example;

import org.junit.jupiter.api.Tag;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class SpoofedAnnotations {
    @Tag("@MethodSource(\"rows\")")
    @ParameterizedTest
    void junit(int value) {}

    static int[] rows() { return new int[] { 1 }; }

    @Deprecated(since = "@DataProvider(name=\"fake\")")
    Object[][] notAProvider() { return new Object[][] { { 1 } }; }

    // @DataProvider(name = "alsoFake")
    @Test(dataProvider = "fake")
    void testng(int value) {}
}
"##,
    );
    assert!(!targets(&report).contains(&test_data::JUNIT_METHOD_SOURCE_TARGET.to_string()));
    assert!(!targets(&report).contains(&test_data::TESTNG_DATA_PROVIDER_TARGET.to_string()));
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_testng_data_provider".to_string()));
}

#[test]
fn test_data_registry_handles_large_decoy_and_lookup_sets_without_per_test_rescans() {
    let mut source = String::from(
        r#"
package com.example;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
class LargeParamTest {
"#,
    );
    for index in 0..2_048 {
        source.push_str(&format!("void decoy{index}() {{}}\n"));
    }
    for index in 0..512 {
        source.push_str(&format!(
            "static int[] rows{index}() {{ return new int[] {{ {index} }}; }}\n"
        ));
        source.push_str(&format!(
            "@ParameterizedTest @MethodSource(\"rows{index}\") void test{index}(int value) {{}}\n"
        ));
    }
    source.push_str("}\n");

    let report = parse(&source);
    assert_eq!(
        targets(&report)
            .iter()
            .filter(|target| target.as_str() == test_data::JUNIT_METHOD_SOURCE_TARGET)
            .count(),
        512
    );
    assert!(!unknown_affected_claims(&report).contains(&"java_test_method_source".to_string()));
}

#[test]
fn mixed_fqn_test_annotations_conflict_and_block() {
    let report = parse(
        r#"
package com.example;

class MixedTest {
    @org.junit.jupiter.api.Test
    void a() {}
}
class OtherTest {
    @org.junit.Test
    void b() {}
}
"#,
    );
    // Each individual method resolves cleanly; force ambiguity via wildcard roots.
    let ambiguous = parse(
        r#"
package com.example;

import org.junit.*;
import org.junit.jupiter.api.*;

class AmbiguousTest {
    @Test
    void a() {}
}
"#,
    );
    assert!(unit_kinds(&report).contains(&"junit5_test_method"));
    assert!(unit_kinds(&report).contains(&"junit4_test_method"));
    assert!(
        unknown_affected_claims(&ambiguous).contains(&"java_test_annotation_binding".to_string())
    );
    assert!(ambiguous.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "ConflictingFacts"
    }));
}

#[test]
fn test_annotation_lookalike_without_import_blocks() {
    let report = parse(
        r#"
package com.example;

class LooseTest {
    @Test
    void runs() {}
}
"#,
    );
    assert!(!unit_kinds(&report).contains(&"junit5_test_method"));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "UnresolvedImport"
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_test_annotation_binding")
    }));
}

// --- Mockito --------------------------------------------------------------

#[test]
fn mockito_field_mocks_add_context_and_non_blocking_unknown() {
    let report = parse(
        r#"
package com.example;

import org.junit.jupiter.api.Test;
import org.mockito.Mock;

class ServiceTest {
    @Mock
    Repo repo;

    @Test
    void runs() {}
}
"#,
    );
    assert!(unit_kinds(&report).contains(&"junit5_test_method"));
    assert!(assumption_values(&report, "mockito_context=").contains(&"field_injection".to_string()));
    assert!(unknown_affected_claims(&report).contains(&"java_mockito_runtime_mocks".to_string()));
}

// --- JPA ------------------------------------------------------------------

#[test]
fn jpa_entities_recognized_under_both_roots_without_clustering_conflation() {
    let jakarta = parse(
        r#"
package com.example;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.OneToMany;

@Entity
public class Book {
    @Id
    Long id;
    @OneToMany
    java.util.List<Page> pages;
}
"#,
    );
    assert!(unit_kinds(&jakarta).contains(&"jpa_entity"));
    assert_eq!(
        targets(&jakarta),
        vec!["jpa.persistence.Entity".to_string()]
    );
    assert!(assumption_values(&jakarta, "jpa_namespace_root=").contains(&"jakarta".to_string()));
    assert!(assumption_values(&jakarta, "jpa_id_present=").contains(&"true".to_string()));
    assert!(assumption_values(&jakarta, "jpa_relationship_shape=").contains(&"to_many".to_string()));
    assert!(unknown_affected_claims(&jakarta).contains(&"java_jpa_runtime_mapping".to_string()));

    let javax = parse(
        r#"
package com.example;

import javax.persistence.Entity;

@Entity
public class Author {
}
"#,
    );
    assert!(assumption_values(&javax, "jpa_namespace_root=").contains(&"javax".to_string()));
}

#[test]
fn jpa_entity_lookalike_without_import_blocks() {
    let report = parse(
        r#"
package com.example;

@Entity
public class Loose {
}
"#,
    );
    assert!(!unit_kinds(&report).contains(&"jpa_entity"));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "UnresolvedImport"
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_jpa_entity_identity")
    }));
}

// --- JAX-RS ---------------------------------------------------------------

#[test]
fn jaxrs_resource_methods_inside_path_class_are_recognized() {
    let report = parse(
        r#"
package com.example;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;

@Path("/books")
public class BookResource {
    @GET
    public String list() {
        return "ok";
    }

    @GET
    @Path("/count")
    public String count() {
        return "1";
    }
}
"#,
    );
    assert!(unit_kinds(&report).contains(&"jaxrs_resource_class"));
    assert_eq!(
        unit_kinds(&report)
            .into_iter()
            .filter(|kind| *kind == "jaxrs_resource_method")
            .count(),
        2
    );
    assert!(targets(&report).contains(&"jaxrs.ws.rs.Path".to_string()));
    assert!(targets(&report).contains(&"jaxrs.ws.rs.GET".to_string()));
    assert!(assumption_values(&report, "http_method=").contains(&"GET".to_string()));
}

#[test]
fn jaxrs_verb_outside_path_class_blocks() {
    let report = parse(
        r#"
package com.example;

import jakarta.ws.rs.GET;

public class Loose {
    @GET
    public String get() {
        return "ok";
    }
}
"#,
    );
    assert!(!unit_kinds(&report).contains(&"jaxrs_resource_method"));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "FrameworkMagic"
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_jaxrs_resource_identity")
    }));
}

// --- Lombok + Spring Data derived queries --------------------------------

#[test]
fn lombok_annotations_emit_non_blocking_generated_members_unknown() {
    let report = parse(
        r#"
package com.example;

import lombok.Data;

@Data
public class Dto {
    private String name;
}
"#,
    );
    assert!(unit_kinds(&report).contains(&"class"));
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.kind == SemanticFactKind::Unknown
            && fact.target.as_ref().expect("target").as_str() == "MacroOrPreprocessor"
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_generated_members")
    }));
}

#[test]
fn spring_data_derived_query_methods_emit_metadata_unknown() {
    let report = parse(
        r#"
package com.example;

import org.springframework.data.jpa.repository.JpaRepository;

interface BookRepository extends JpaRepository<Book, Long> {
    Book findByTitle(String title);
    long countByAuthor(String author);
    void save(Book book);
}
"#,
    );
    let derived = report
        .semantic_facts
        .iter()
        .filter(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_spring_data_query_derivation")
        })
        .count();
    assert_eq!(
        derived, 2,
        "findByTitle and countByAuthor are derived queries"
    );
    assert!(report.semantic_facts.iter().any(|fact| {
        fact.assumptions
            .iter()
            .any(|assumption| assumption == "spring_data_derived_query=matched")
    }));
}
