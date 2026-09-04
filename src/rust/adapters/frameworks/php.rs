//! Conservative PHP framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the PHPUnit test method ADR-0047 admits.
//! The enclosing `TestCase`-derived class is recognized as context so a
//! method outside one never anchors, but the class itself carries no role:
//! the framework runs methods, and a class without them is not a test.

use crate::core::model::CodeUnitKind;

/// `framework:phpunit.` is claimed by no other language registry, and the
/// role name is checked against every other prefix by the shared prefix
/// exclusivity tests.
pub(crate) const ROLE_PHPUNIT_TEST: &str = "framework:phpunit.test_method";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhpFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<PhpFrameworkRole> {
    match kind {
        CodeUnitKind::PhpTestMethod => Some(PhpFrameworkRole {
            target: ROLE_PHPUNIT_TEST,
            note: "bounded PHP code unit declares a public non-static test-markered method in a TestCase-derived class",
            assumption: "PHPUnit selection, execution, ordering, providers, and suite membership are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:phpunit.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_PHPUNIT_TEST => {
            Some(target == crate::adapters::parsing::php::phpunit::PHP_TEST_METHOD_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_PHPUNIT_TEST => "php.phpunit.test_method".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_method_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::PhpTestMethod)
                .expect("test role")
                .target,
            ROLE_PHPUNIT_TEST
        );
        // The class is context, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::PhpTestClass).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("phpunit.TestMethod", ROLE_PHPUNIT_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("phpunit.TestCase", ROLE_PHPUNIT_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("phpunit.TestMethod", "framework:aunit.x"),
            None
        );
        assert_eq!(
            support_family("phpunit.TestMethod", ROLE_PHPUNIT_TEST),
            "php.phpunit.test_method"
        );
    }
}
