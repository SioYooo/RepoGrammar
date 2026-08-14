//! Conservative Delphi framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the DUnitX test procedure ADR-0044 admits. The
//! enclosing `TestFixture` is recognized as context so a procedure outside one
//! never anchors, but the fixture itself carries no role: DUnitX discovers
//! procedures, and a fixture without them is not a test.

use crate::core::model::CodeUnitKind;

/// `framework:dunitx.` is claimed by no other language registry, and the role
/// name is checked against every other prefix by
/// `language_role_prefixes_do_not_claim_each_other`.
pub(crate) const ROLE_DUNITX_TEST: &str = "framework:dunitx.test_procedure";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DelphiFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<DelphiFrameworkRole> {
    match kind {
        CodeUnitKind::DelphiTestProcedure => Some(DelphiFrameworkRole {
            target: ROLE_DUNITX_TEST,
            note: "bounded Object Pascal code unit carries an exact DUnitX Test attribute under a DUnitX import",
            assumption: "DUnitX discovery, execution, ordering, and parameterised rows are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:dunitx.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_DUNITX_TEST => {
            Some(target == crate::adapters::parsing::delphi::dunitx::DELPHI_TEST_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_DUNITX_TEST => "dunitx.test_procedure".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_procedure_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::DelphiTestProcedure)
                .expect("test role")
                .target,
            ROLE_DUNITX_TEST
        );
        // The fixture is context, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::DelphiTestFixture).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("dunitx.Test", ROLE_DUNITX_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("dunitx.TestFixture", ROLE_DUNITX_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("dunitx.Test", "framework:mstest.test"),
            None
        );
        assert_eq!(
            support_family("dunitx.Test", ROLE_DUNITX_TEST),
            "dunitx.test_procedure"
        );
    }
}
