//! Conservative C/C++ framework adapter registry (bounded v0.2 preview).
//!
//! These adapters identify only exact include-evidence-gated GoogleTest,
//! Catch2, doctest, and Boost.Test registration-macro roles. They never expand
//! macros, evaluate the preprocessor, run a build, or generate moc/protoc
//! output. Qt `Q_OBJECT` classes are structural context only and carry no
//! framework role.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_GTEST_TEST: &str = "framework:gtest.test";
pub(crate) const ROLE_GTEST_FIXTURE: &str = "framework:gtest.fixture";
pub(crate) const ROLE_CATCH2_TEST: &str = "framework:catch2.test";
pub(crate) const ROLE_DOCTEST_TEST: &str = "framework:doctest.test";
pub(crate) const ROLE_BOOST_TEST: &str = "framework:boost_test.test";
pub(crate) const ROLE_BOOST_SUITE: &str = "framework:boost_test.suite";

pub(crate) const GTEST_TEST_TARGETS: &[&str] = &[
    "gtest.TEST",
    "gtest.TEST_F",
    "gtest.TEST_P",
    "gtest.TYPED_TEST",
];

pub(crate) const CATCH2_TEST_TARGETS: &[&str] = &["catch2.TEST_CASE", "catch2.SCENARIO"];

pub(crate) const BOOST_TEST_TARGETS: &[&str] = &[
    "boost_test.BOOST_AUTO_TEST_CASE",
    "boost_test.BOOST_FIXTURE_TEST_CASE",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CppFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<CppFrameworkRole> {
    match kind {
        CodeUnitKind::GtestTestCase => Some(CppFrameworkRole {
            target: ROLE_GTEST_TEST,
            note: "Tree-sitter C/C++ code unit indicates exact GoogleTest test macro role",
            assumption: "GoogleTest runtime registration is not evaluated",
        }),
        CodeUnitKind::GtestTestFixture => Some(CppFrameworkRole {
            target: ROLE_GTEST_FIXTURE,
            note: "Tree-sitter C/C++ code unit indicates exact GoogleTest fixture base role",
            assumption: "GoogleTest fixture lifecycle is not evaluated",
        }),
        CodeUnitKind::Catch2TestCase => Some(CppFrameworkRole {
            target: ROLE_CATCH2_TEST,
            note: "Tree-sitter C/C++ code unit indicates exact Catch2 test macro role",
            assumption: "Catch2 runtime registration is not evaluated",
        }),
        CodeUnitKind::DoctestTestCase => Some(CppFrameworkRole {
            target: ROLE_DOCTEST_TEST,
            note: "Tree-sitter C/C++ code unit indicates exact doctest test macro role",
            assumption: "doctest runtime registration is not evaluated",
        }),
        CodeUnitKind::BoostTestCase => Some(CppFrameworkRole {
            target: ROLE_BOOST_TEST,
            note: "Tree-sitter C/C++ code unit indicates exact Boost.Test case macro role",
            assumption: "Boost.Test runtime registration is not evaluated",
        }),
        CodeUnitKind::BoostTestSuite => Some(CppFrameworkRole {
            target: ROLE_BOOST_SUITE,
            note: "Tree-sitter C/C++ code unit indicates exact Boost.Test suite macro role",
            assumption: "Boost.Test suite registration is not evaluated",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:gtest.")
        || framework_role.starts_with("framework:catch2.")
        || framework_role.starts_with("framework:doctest.")
        || framework_role.starts_with("framework:boost_test.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_GTEST_TEST => Some(GTEST_TEST_TARGETS.contains(&target)),
        ROLE_GTEST_FIXTURE => Some(target == "gtest.testing.Test"),
        ROLE_CATCH2_TEST => Some(CATCH2_TEST_TARGETS.contains(&target)),
        ROLE_DOCTEST_TEST => Some(target == "doctest.TEST_CASE"),
        ROLE_BOOST_TEST => Some(BOOST_TEST_TARGETS.contains(&target)),
        ROLE_BOOST_SUITE => Some(target == "boost_test.BOOST_AUTO_TEST_SUITE"),
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_GTEST_TEST => "gtest.test_macro".to_string(),
        ROLE_GTEST_FIXTURE => "gtest.test_fixture".to_string(),
        ROLE_CATCH2_TEST => "catch2.test_case".to_string(),
        ROLE_DOCTEST_TEST => "doctest.test_case".to_string(),
        ROLE_BOOST_TEST => "boost_test.test_case".to_string(),
        ROLE_BOOST_SUITE => "boost_test.test_suite".to_string(),
        _ => target.to_string(),
    }
}
