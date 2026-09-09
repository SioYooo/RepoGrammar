//! Conservative Ruby framework adapter registry (bounded preview).
//!
//! The one family-bearing role is the Minitest `test_*` instance method
//! ADR-0049 admits. The enclosing `Minitest::Test`/`ActiveSupport::TestCase`
//! class is recognized as identity evidence so a method outside one never
//! anchors, but the class itself carries no role: the framework runs methods,
//! and a class without them is not a test.

use crate::core::model::CodeUnitKind;

/// `framework:minitest.` is claimed by no other language registry, and the
/// role name is checked against every other prefix by
/// `language_role_prefixes_do_not_claim_each_other`.
pub(crate) const ROLE_MINITEST_TEST: &str = "framework:minitest.test_method";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RubyFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<RubyFrameworkRole> {
    match kind {
        CodeUnitKind::RubyMinitestTestMethod => Some(RubyFrameworkRole {
            target: ROLE_MINITEST_TEST,
            note: "bounded Ruby code unit declares a direct zero-parameter test_* instance method of a class whose in-file superclass chain reaches Minitest::Test or ActiveSupport::TestCase",
            assumption: "Minitest discovery, execution, ordering, hooks, and assertions are runtime behavior",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:minitest.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_MINITEST_TEST => {
            Some(target == crate::adapters::parsing::ruby::minitest::RUBY_MINITEST_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_MINITEST_TEST => "ruby.minitest.test_method".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_test_method_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::RubyMinitestTestMethod)
                .expect("test role")
                .target,
            ROLE_MINITEST_TEST
        );
        // The class is identity evidence, not a claim of its own.
        assert!(role_for_code_unit_kind(&CodeUnitKind::RubyMinitestTestClass).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("ruby.minitest.test_method", ROLE_MINITEST_TEST),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("ruby.minitest.test_class", ROLE_MINITEST_TEST),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("ruby.minitest.test_method", "framework:pytest.test"),
            None
        );
        assert_eq!(
            support_target_is_role_compatible("ruby.minitest.test_method", "framework:mstest.test"),
            None,
            "the mstest prefix must not claim the minitest role"
        );
        assert_eq!(
            support_family("ruby.minitest.test_method", ROLE_MINITEST_TEST),
            "ruby.minitest.test_method"
        );
    }
}
