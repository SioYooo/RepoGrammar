#include <catch2/catch_test_macros.hpp>
#include <doctest/doctest.h>

// TEST_CASE is shared by Catch2 and doctest. With BOTH include evidences the
// framework identity is a blocking ConflictingFacts cpp_test_framework_identity
// UNKNOWN, never a guessed anchor.

TEST_CASE("ambiguous provider")
{
    CHECK(true);
}
