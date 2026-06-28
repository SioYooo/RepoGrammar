pub(crate) const ROLE_SUITE: &str = "framework:jest_vitest.suite";
pub(crate) const ROLE_TEST: &str = "framework:jest_vitest.test";

pub(crate) const SUITE_TARGETS: &[&str] = &[
    "package:vitest",
    "package:@jest/globals",
    "jest_vitest.describe",
];

pub(crate) const TEST_TARGETS: &[&str] = &[
    "package:vitest",
    "package:@jest/globals",
    "jest_vitest.it",
    "jest_vitest.test",
];
