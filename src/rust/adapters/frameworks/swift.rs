//! Conservative Swift framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the XCTest test method ADR-0048 admits.
//! The enclosing `XCTestCase` class is recognized as context so a method
//! outside one never anchors, but the class itself carries no role: the
//! framework runs methods, and a class without them is not a test. XCTest
//! outcomes, assertions, expectations, fixtures, and parallelization are
//! runtime behavior and carry no role.

use crate::core::model::CodeUnitKind;

/// `framework:xctest.` is claimed by no other language registry. The token is
/// the framework, not the language, matching `testthat`, `phpunit`, `dunitx`,
/// and `aunit`; `XCTest` needs no language prefix because nothing else claims
/// it, unlike `vb_mstest` and `matlab_unittest`.
pub(crate) const ROLE_XCTEST_TEST: &str = "framework:xctest.test";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SwiftFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<SwiftFrameworkRole> {
    match kind {
        CodeUnitKind::SwiftTestMethod => Some(SwiftFrameworkRole {
            target: ROLE_XCTEST_TEST,
            note: "bounded Swift code unit declares an instance test-prefixed method of an XCTestCase subclass",
            assumption: "XCTest discovery, execution, expectations, and ordering are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:xctest.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_XCTEST_TEST => {
            Some(target == crate::adapters::parsing::swift::xctest::SWIFT_TEST_METHOD_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_XCTEST_TEST => "swift.xctest.test_method".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_method_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::SwiftTestMethod)
                .expect("test role")
                .target,
            ROLE_XCTEST_TEST
        );
        // The class is context, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::SwiftTestClass).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("swift.xctest.test_method", ROLE_XCTEST_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("swift.xctest.test_class", ROLE_XCTEST_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("swift.xctest.test_method", "framework:mstest.test"),
            None
        );
        assert_eq!(
            support_family("swift.xctest.test_method", ROLE_XCTEST_TEST),
            "swift.xctest.test_method"
        );
    }

    #[test]
    fn the_role_prefix_claims_no_other_language_role() {
        assert!(!"framework:matlab_unittest.test_method".starts_with("framework:swift."));
        assert!(!ROLE_XCTEST_TEST.starts_with("framework:mstest."));
    }
}
