Imports Microsoft.VisualStudio.TestTools.UnitTesting

' Three admitted MSTest declarations, enough to reach the support threshold of
' three. The commented and quoted attributes below must anchor nothing.

' <TestMethod()>
REM <TestMethod()>

<TestClass()>
Public Class CatalogTests

    <TestMethod()>
    Public Sub LoadsCatalog()
        Dim sample As String = "<TestMethod()>"
        Assert.IsNotNull(sample)
    End Sub

    <TestMethod>
    Public Sub FiltersCatalog()
        Assert.IsTrue(True)
    End Sub

    <TestMethod()>
    Public Sub OpensProduct()
        Assert.IsTrue(True)
    End Sub

    ' Not a test: no attribute, and a Function returns a value.
    Public Sub Helper()
    End Sub

    <TestMethod()>
    Public Function NotATest() As Boolean
        Return True
    End Function

End Class
