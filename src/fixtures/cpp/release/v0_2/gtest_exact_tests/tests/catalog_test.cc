#include <gtest/gtest.h>

// Three exact GoogleTest registrations corroborated by the gtest include.
// Each derives one bounded gtest.TEST support fact and the three form a single
// family. RepoGrammar never runs the GoogleTest runtime registration.

TEST(CatalogTest, ReturnsItems)
{
    EXPECT_TRUE(true);
}

TEST(CatalogTest, ReturnsItem)
{
    EXPECT_EQ(1, 1);
}

TEST(CatalogTest, ReturnsSummary)
{
    EXPECT_TRUE(true);
}
