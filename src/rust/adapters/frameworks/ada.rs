//! Conservative Ada framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the AUnit routine registration ADR-0045
//! admits. AUnit runs what `Register_Tests` registers, so the registration call
//! is the anchor; the enclosing procedure carries no role of its own.

use crate::core::model::CodeUnitKind;

/// `framework:aunit.` is claimed by no other language registry, and the role
/// name is checked against every other prefix by
/// `language_role_prefixes_do_not_claim_each_other`.
pub(crate) const ROLE_AUNIT_TEST: &str = "framework:aunit.test_registration";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdaFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<AdaFrameworkRole> {
    match kind {
        CodeUnitKind::AdaTestRegistration => Some(AdaFrameworkRole {
            target: ROLE_AUNIT_TEST,
            note: "bounded Ada code unit registers a named routine with AUnit under an AUnit with clause",
            assumption: "AUnit execution, outcomes, ordering, and suite collection are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:aunit.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_AUNIT_TEST => Some(target == crate::adapters::parsing::ada::aunit::ADA_TEST_TARGET),
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_AUNIT_TEST => "aunit.test_registration".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_registration_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::AdaTestRegistration)
                .expect("test role")
                .target,
            ROLE_AUNIT_TEST
        );
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("aunit.Register_Routine", ROLE_AUNIT_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("aunit.Add_Test", ROLE_AUNIT_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("aunit.Register_Routine", "framework:dunitx.test"),
            None
        );
        assert_eq!(
            support_family("aunit.Register_Routine", ROLE_AUNIT_TEST),
            "aunit.test_registration"
        );
    }
}
