//! Conservative VB.NET framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the MSTest test method ADR-0043 admits. The
//! enclosing `TestClass` is recognized as context so a method outside one never
//! anchors, but the class itself carries no role: MSTest discovers methods, and
//! a class without them is not a test.

use crate::core::model::CodeUnitKind;

/// Deliberately not under `framework:mstest.`: the C# registry claims that
/// whole prefix, and a role that collides with it is silently answered by the
/// wrong language and never forms a family.
pub(crate) const ROLE_MSTEST_TEST: &str = "framework:vb_mstest.test_method";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisualBasicFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<VisualBasicFrameworkRole> {
    match kind {
        CodeUnitKind::VbTestMethod => Some(VisualBasicFrameworkRole {
            target: ROLE_MSTEST_TEST,
            note: "bounded VB.NET code unit carries an exact MSTest TestMethod attribute",
            assumption: "MSTest discovery, execution, ordering, and data rows are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:vb_mstest.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_MSTEST_TEST => {
            Some(target == crate::adapters::parsing::visual_basic::mstest::VB_TEST_METHOD_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_MSTEST_TEST => "mstest.vb_test_method".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_method_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::VbTestMethod)
                .expect("test role")
                .target,
            ROLE_MSTEST_TEST
        );
        // The class is context, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::VbTestClass).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("mstest.TestMethod", ROLE_MSTEST_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("mstest.TestClass", ROLE_MSTEST_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("mstest.TestMethod", "framework:mstest.test"),
            None,
            "the C# MSTest role owns the framework:mstest. prefix and is answered there"
        );
        assert_eq!(
            support_family("mstest.TestMethod", ROLE_MSTEST_TEST),
            "mstest.vb_test_method"
        );
    }
}
