// No framework includes at all, so the registration-macro shapes are
// lookalikes: each stays a blocking cpp_test_framework_identity UNKNOWN and no
// family may form.

TEST(CatalogTest, ReturnsItems)
{
    EXPECT_TRUE(true);
}

TEST_CASE("catalog returns items")
{
    CHECK(true);
}
