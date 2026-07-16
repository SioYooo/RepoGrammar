#include <gtest/gtest.h>

// The gtest include resolves the macro identity and no build variant guards the
// registrations, so each TEST derives a bounded DATAFLOW_DERIVED support fact
// targeting gtest.TEST and the three form one family.

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
