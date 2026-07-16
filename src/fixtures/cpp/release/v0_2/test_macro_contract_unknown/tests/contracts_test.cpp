#include <boost/test/unit_test.hpp>
#include <catch2/catch_test_macros.hpp>
#include <gtest/gtest.h>

// Each registration has an exact framework include but violates the audited
// macro contract. These must remain typed cpp_test_framework_identity UNKNOWNs
// and must never form a family.

TEST(Catalog_Test, ReadsItems)
{
    EXPECT_TRUE(true);
}

TEST_CASE("free-form second argument", "not tags")
{
    CHECK(true);
}

TEST_CASE("unbalanced tags", "[catalog")
{
    CHECK(true);
}

TEST_CASE("trailing tag garbage", "[catalog] trailing")
{
    CHECK(true);
}

BOOST_AUTO_TEST_CASE(template_only_decorator, *boost::unit_test::enable_if<true>())
{
    BOOST_TEST(true);
}

BOOST_AUTO_TEST_CASE(wrong_decorator_arity, *boost::unit_test::label())
{
    BOOST_TEST(true);
}
