//! Conservative R framework adapter registry (bounded preview).
//!
//! The one role marks the testthat `test_that` block ADR-0042 admits. Test
//! outcomes, assertions, skips, ordering, fixtures, and helper loading are
//! runtime behavior and carry no role.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_TESTTHAT_TEST: &str = "framework:testthat.test";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<RFrameworkRole> {
    match kind {
        CodeUnitKind::RTestThatBlock => Some(RFrameworkRole {
            target: ROLE_TESTTHAT_TEST,
            note: "bounded R code unit is a top-level testthat test_that block",
            assumption: "testthat outcomes, assertions, skips, and ordering are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:testthat.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_TESTTHAT_TEST => {
            Some(target == crate::adapters::parsing::r::testthat::R_TEST_THAT_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_TESTTHAT_TEST => "testthat.test_that".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_admitted_block_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::RTestThatBlock)
                .expect("test role")
                .target,
            ROLE_TESTTHAT_TEST
        );
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("testthat.test_that", ROLE_TESTTHAT_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("testthat.describe", ROLE_TESTTHAT_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("testthat.test_that", "framework:pytest.test"),
            None
        );
        assert_eq!(
            support_family("testthat.test_that", ROLE_TESTTHAT_TEST),
            "testthat.test_that"
        );
    }
}
