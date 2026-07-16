#include <gtest/gtest.h>

// The gtest include corroborates the macro identity, but every registration is
// guarded by an undischarged build variant. Each unit takes a blocking
// BuildVariantAmbiguity cpp_build_variant UNKNOWN and no family may form.

#ifdef ENABLE_SLOW_TESTS
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
#endif
