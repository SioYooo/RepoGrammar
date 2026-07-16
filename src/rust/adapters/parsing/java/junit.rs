//! JUnit 5, JUnit 4, TestNG test-method recognition and Mockito test-context
//! detection.
//!
//! Test annotations gate on an exact imported simple name (or wildcard import of
//! the exact package) or an inline FQN, mirroring the Spring gate. Mockito mocks
//! are bytecode-generated at runtime, so they contribute only a typed UNKNOWN and
//! a `mockito_context=` metadata assumption, never a support anchor.

use super::{
    annotation_segment_exact, contains_annotation_simple_name, has_exact_annotation,
    has_exact_direct_annotation, java_visibility_shape, test_data, JavaImportContext,
};
use crate::core::model::CodeUnitKind;

const JUNIT5_API_PACKAGE: &str = "org.junit.jupiter.api";
const JUNIT5_PARAMS_PACKAGE: &str = "org.junit.jupiter.params";
const JUNIT5_PARAMS_PROVIDER_PACKAGE: &str = "org.junit.jupiter.params.provider";
const JUNIT4_PACKAGE: &str = "org.junit";
const TESTNG_PACKAGE: &str = "org.testng.annotations";
const MOCKITO_PACKAGE: &str = "org.mockito";
const MOCKITO_JUNIT_JUPITER_PACKAGE: &str = "org.mockito.junit.jupiter";
const JUNIT5_EXTENSION_PACKAGE: &str = "org.junit.jupiter.api.extension";

const MOCKITO_FIELD_ANNOTATIONS: &[&str] = &["Mock", "Spy", "InjectMocks", "Captor"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestMethodAnchor {
    pub(crate) kind: CodeUnitKind,
    pub(crate) target: &'static str,
    pub(crate) anchor_kind: &'static str,
    pub(crate) test_annotation: &'static str,
    data_shape: &'static str,
    pub(crate) data_reference: test_data::TestDataReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestClassification {
    Resolved(TestMethodAnchor),
    /// `@Test` resolves to more than one distinct test framework at once.
    Conflict,
    /// A known test annotation simple name without any exact import/FQN evidence.
    Lookalike,
    None,
}

pub(crate) fn classify_test_method(
    annotations: &str,
    imports: &JavaImportContext,
) -> TestClassification {
    let junit5_param = has_exact_direct_annotation(
        annotations,
        "ParameterizedTest",
        &[JUNIT5_PARAMS_PACKAGE],
        imports,
    );
    let junit5_test =
        has_exact_direct_annotation(annotations, "Test", &[JUNIT5_API_PACKAGE], imports);
    let junit4_test = has_exact_direct_annotation(annotations, "Test", &[JUNIT4_PACKAGE], imports);
    let testng_test = has_exact_direct_annotation(annotations, "Test", &[TESTNG_PACKAGE], imports);

    let junit5_binding = junit5_param || junit5_test;
    let test_bindings = [junit5_binding, junit4_test, testng_test]
        .into_iter()
        .filter(|resolved| *resolved)
        .count();
    if test_bindings > 1 || (junit5_param && junit5_test) {
        return TestClassification::Conflict;
    }

    if junit5_param {
        let (mut data_shape, mut data_reference) = junit5_data_shape(annotations, imports);
        if !matches!(data_reference, test_data::TestDataReference::None)
            && !test_data::has_strict_annotation_identity(
                annotations,
                "ParameterizedTest",
                JUNIT5_PARAMS_PACKAGE,
                imports,
            )
        {
            data_reference = test_data::TestDataReference::Junit(
                test_data::MethodSourceReference::UnknownTestIdentity,
            );
            data_shape = test_data::data_shape(&data_reference);
        }
        return TestClassification::Resolved(TestMethodAnchor {
            kind: CodeUnitKind::Junit5TestMethod,
            target: "junit.jupiter.ParameterizedTest",
            anchor_kind: "junit5_test_method",
            test_annotation: "ParameterizedTest",
            data_shape,
            data_reference,
        });
    }
    if junit5_test {
        let data_reference = test_data::method_source_reference(annotations, imports)
            .map(|_| {
                test_data::TestDataReference::Junit(
                    test_data::MethodSourceReference::InvalidTestKind,
                )
            })
            .unwrap_or(test_data::TestDataReference::None);
        return TestClassification::Resolved(TestMethodAnchor {
            kind: CodeUnitKind::Junit5TestMethod,
            target: "junit.jupiter.Test",
            anchor_kind: "junit5_test_method",
            test_annotation: "Test",
            data_shape: test_data::data_shape(&data_reference),
            data_reference,
        });
    }
    if junit4_test {
        return TestClassification::Resolved(TestMethodAnchor {
            kind: CodeUnitKind::Junit4TestMethod,
            target: "junit4.Test",
            anchor_kind: "junit4_test_method",
            test_annotation: "Test",
            data_shape: "none",
            data_reference: test_data::TestDataReference::None,
        });
    }
    if testng_test {
        let data_reference = test_data::data_provider_reference(annotations, imports)
            .map(test_data::TestDataReference::Testng)
            .unwrap_or(test_data::TestDataReference::None);
        return TestClassification::Resolved(TestMethodAnchor {
            kind: CodeUnitKind::TestngTestMethod,
            target: "testng.annotations.Test",
            anchor_kind: "testng_test_method",
            test_annotation: "Test",
            data_shape: "none",
            data_reference,
        });
    }

    if contains_annotation_simple_name(annotations, &["Test", "ParameterizedTest"]) {
        return TestClassification::Lookalike;
    }
    TestClassification::None
}

pub(crate) fn test_method_assumptions(
    anchor: &TestMethodAnchor,
    mockito_context: Option<&'static str>,
    annotations: &str,
    _slice: &str,
) -> Vec<String> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        format!("java_anchor_kind={}", anchor.anchor_kind),
        format!("test_annotation={}", anchor.test_annotation),
        format!("test_data_shape={}", anchor.data_shape),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
    ];
    if let Some(context) = mockito_context {
        assumptions.push(format!("mockito_context={context}"));
    }
    assumptions
}

fn junit5_data_shape(
    annotations: &str,
    imports: &JavaImportContext,
) -> (&'static str, test_data::TestDataReference) {
    let value_source = has_exact_direct_annotation(
        annotations,
        "ValueSource",
        &[JUNIT5_PARAMS_PROVIDER_PACKAGE],
        imports,
    );
    let csv_source = has_exact_direct_annotation(
        annotations,
        "CsvSource",
        &[JUNIT5_PARAMS_PROVIDER_PACKAGE],
        imports,
    );
    let reference = test_data::method_source_reference(annotations, imports)
        .map(test_data::TestDataReference::Junit)
        .unwrap_or(test_data::TestDataReference::None);
    let method_source = !matches!(reference, test_data::TestDataReference::None);
    let source_kinds =
        usize::from(value_source) + usize::from(csv_source) + usize::from(method_source);
    let shape = if source_kinds > 1 {
        "multiple_sources"
    } else if value_source {
        "value_source"
    } else if csv_source {
        "csv_source"
    } else if method_source {
        test_data::data_shape(&reference)
    } else {
        "none"
    };
    (shape, reference)
}

/// Detects a Mockito test context on a class: field mocks (`@Mock`/`@Spy`/
/// `@InjectMocks`/`@Captor`) or `@ExtendWith(MockitoExtension.class)`.
pub(crate) fn mockito_context(
    class_slice: &str,
    imports: &JavaImportContext,
) -> Option<&'static str> {
    let has_field_mock = MOCKITO_FIELD_ANNOTATIONS.iter().any(|annotation| {
        has_exact_annotation(class_slice, annotation, &[MOCKITO_PACKAGE], imports)
    });
    if has_field_mock {
        return Some("field_injection");
    }
    if mockito_extension_present(class_slice, imports) {
        return Some("extension");
    }
    None
}

fn mockito_extension_present(class_slice: &str, imports: &JavaImportContext) -> bool {
    let Some(segment) = annotation_segment_exact(
        class_slice,
        "ExtendWith",
        &[JUNIT5_EXTENSION_PACKAGE],
        imports,
    ) else {
        return false;
    };
    segment.contains("MockitoExtension")
        && (imports.has_import_for("MockitoExtension", &[MOCKITO_JUNIT_JUPITER_PACKAGE])
            || segment.contains("org.mockito.junit.jupiter.MockitoExtension"))
}
