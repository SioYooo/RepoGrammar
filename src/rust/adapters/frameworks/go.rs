//! Conservative Go framework adapter registry (bounded preview).
//!
//! The only role is the `testing` package's test-function declaration, which is
//! the one shape ADR-0041 admits. Subtests, parallelism, helper registration,
//! skips, and fixtures are runtime behavior and carry no role.

use crate::core::model::CodeUnitKind;

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
            note: "bounded Go code unit declares a testing test function",
            assumption: "Go subtests, parallelism, helpers, and skips are runtime behavior",
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
            Some(target == crate::adapters::parsing::go::source::GO_TEST_FUNCTION_TARGET)
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
        // An ordinary declaration is inventory, so it can never join the family.
        assert!(role_for_code_unit_kind(&CodeUnitKind::GoFunction).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("go.testing.T", ROLE_GO_TESTING_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("go.testing.B", ROLE_GO_TESTING_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("go.testing.T", "framework:go_testing.other"),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("go.testing.T", "framework:pytest.test"),
            None
        );
        assert_eq!(
            support_family("go.testing.T", ROLE_GO_TESTING_TEST),
            "go.testing.test_function"
        );
    }
}
