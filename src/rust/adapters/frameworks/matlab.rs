//! Conservative MATLAB framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the `matlab.unittest` test method ADR-0046
//! admits. The enclosing `TestCase` class is recognized as context so a method
//! outside one never anchors, but the class itself carries no role: the
//! framework runs methods, and a class without them is not a test.

use crate::core::model::CodeUnitKind;

/// `framework:matlab_unittest.` is claimed by no other language registry, and
/// the role name is checked against every other prefix by
/// `language_role_prefixes_do_not_claim_each_other`.
pub(crate) const ROLE_UNITTEST_TEST: &str = "framework:matlab_unittest.test_method";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatlabFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<MatlabFrameworkRole> {
    match kind {
        CodeUnitKind::MatlabTestMethod => Some(MatlabFrameworkRole {
            target: ROLE_UNITTEST_TEST,
            note: "bounded MATLAB code unit declares a method in a Test methods block of a matlab.unittest.TestCase class",
            assumption: "matlab.unittest discovery, execution, ordering, tags, and parameters are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:matlab_unittest.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_UNITTEST_TEST => {
            Some(target == crate::adapters::parsing::matlab::unittest::MATLAB_TEST_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_UNITTEST_TEST => "matlab_unittest.test_method".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_method_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::MatlabTestMethod)
                .expect("test role")
                .target,
            ROLE_UNITTEST_TEST
        );
        // The class is context, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::MatlabTestClass).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("matlab_unittest.TestMethod", ROLE_UNITTEST_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("matlab_unittest.TestCase", ROLE_UNITTEST_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("matlab_unittest.TestMethod", "framework:aunit.x"),
            None
        );
        assert_eq!(
            support_family("matlab_unittest.TestMethod", ROLE_UNITTEST_TEST),
            "matlab_unittest.test_method"
        );
    }
}
