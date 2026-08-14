' No Imports and no fully qualified namespace, so these attribute names are not
' the MSTest ones and nothing may anchor.

<TestClass()>
Public Class CatalogTests

    <TestMethod()>
    Public Sub LoadsCatalog()
    End Sub

    <TestMethod()>
    Public Sub FiltersCatalog()
    End Sub

    <TestMethod()>
    Public Sub OpensProduct()
    End Sub

End Class
