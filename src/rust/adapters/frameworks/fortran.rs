//! Conservative Fortran framework adapter registry (bounded preview).
//!
//! The one role marks the test-drive test subroutine ADR-0051 admits:
//! a module-scope or top-level `subroutine test_<name>` whose first dummy is
//! `type(error_type)` with `intent(out)`, proven by an in-scope
//! `use testdrive`. Registration wiring, execution, outcomes, skips, and
//! ordering are runtime behavior and carry no role.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_TESTDRIVE_TEST: &str = "framework:testdrive.test_subroutine";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FortranFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<FortranFrameworkRole> {
    match kind {
        CodeUnitKind::FortranTestDriveSubroutine => Some(FortranFrameworkRole {
            target: ROLE_TESTDRIVE_TEST,
            note: "bounded Fortran code unit carries an exact test-drive error_type test subroutine under a proven use testdrive",
            assumption: "test-drive registration, execution, outcomes, and ordering are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:testdrive.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_TESTDRIVE_TEST => {
            Some(target == crate::adapters::parsing::fortran::testdrive::FORTRAN_TEST_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_TESTDRIVE_TEST => "fortran.testdrive.test_subroutine".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_admitted_test_subroutine_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::FortranTestDriveSubroutine)
                .expect("test role")
                .target,
            ROLE_TESTDRIVE_TEST
        );
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("testdrive.test_subroutine", ROLE_TESTDRIVE_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("testdrive.collect", ROLE_TESTDRIVE_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("testdrive.test_subroutine", "framework:pytest.test"),
            None
        );
        assert_eq!(
            support_family("testdrive.test_subroutine", ROLE_TESTDRIVE_TEST),
            "fortran.testdrive.test_subroutine"
        );
    }
}
