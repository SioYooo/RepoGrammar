#include <catch2/catch_test_macros.hpp>

// Three exact Catch2 registrations corroborated by the catch2 include and NOT
// by any doctest include, so each derives one bounded catch2.TEST_CASE support
// fact and the three form a single family.

TEST_CASE("catalog returns items")
{
    CHECK(true);
}

TEST_CASE("catalog returns one item")
{
    CHECK(true);
}

TEST_CASE("catalog returns a summary")
{
    CHECK(true);
}
