//! Go role registry (bounded preview).
//!
//! The one role marks the `testing` test-function declarations ADR-0050
//! admits: Test, Benchmark, and Fuzz variants of the exact one-parameter
//! shape, read off a real parse rather than a scan. ADR-0050 supersedes
//! ADR-0041's correction for this family, so the role is family-bearing: the
//! support target is the fixed `go.testing.test_function` token, and the
//! derived-support path is the one the shared family layer owns.
//!
//! Test outcomes, skips, parallelism, subtest identity, and the package graph
//! are runtime or cross-file behavior and carry no role.

use crate::core::model::CodeUnitKind;

/// `framework:go_testing.` is claimed by no other language registry, and the
/// role name is checked against every other prefix by
/// `language_role_prefixes_do_not_claim_each_other`.
pub(crate) const ROLE_GO_TESTING_TEST: &str = "framework:go_testing.test_function";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GoFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<GoFrameworkRole> {
    match kind {
        CodeUnitKind::GoTestFunction => Some(GoFrameworkRole {
            target: ROLE_GO_TESTING_TEST,
            note: "bounded Go code unit declares a testing test-function family declaration",
            assumption:
                "test execution, subtests, and package identity are runtime or cross-file behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:go_testing.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_GO_TESTING_TEST => {
            Some(target == crate::adapters::parsing::go::testing::GO_TESTING_TEST_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_GO_TESTING_TEST => "go.testing.test_function".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_admitted_test_declaration_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::GoTestFunction)
                .expect("test role")
                .target,
            ROLE_GO_TESTING_TEST
        );
        assert!(role_for_code_unit_kind(&CodeUnitKind::GoFunction).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("go.testing.test_function", ROLE_GO_TESTING_TEST),
            Some(true)
        );
        // The retired scanner target named the parameter type, not the family
        // token, and must no longer support the role.
        assert_eq!(
            support_target_is_role_compatible("go.testing.T", ROLE_GO_TESTING_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("go.testing.test_function", "framework:pytest.test"),
            None
        );
        assert_eq!(
            support_family("go.testing.test_function", ROLE_GO_TESTING_TEST),
            "go.testing.test_function"
        );
    }
}
