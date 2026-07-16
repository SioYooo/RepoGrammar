// No gtest include and the registrations are guarded by an undischarged build
// variant, so each TEST stays a blocking cpp_test_framework_identity and
// cpp_build_variant UNKNOWN and no family forms.

#ifdef ENABLE_TESTS
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
