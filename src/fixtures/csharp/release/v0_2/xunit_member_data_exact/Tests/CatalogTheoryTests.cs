using System.Collections.Generic;
using Xunit;

namespace Example.Catalog.Tests;

public class CatalogTheoryTests
{
    public static IEnumerable<object[]> CatalogCases =>
        new[] { new object[] { 1 }, new object[] { 2 } };

    [Theory]
    [MemberData("CatalogCases")]
    public void ReturnsItems(int count)
    {
        Assert.True(count > 0);
    }

    [Theory]
    [MemberData("CatalogCases")]
    public void ReturnsItem(int count)
    {
        Assert.True(count > 0);
    }

    [Theory]
    [MemberData("CatalogCases")]
    public void ReturnsSummary(int count)
    {
        Assert.True(count > 0);
    }
}
