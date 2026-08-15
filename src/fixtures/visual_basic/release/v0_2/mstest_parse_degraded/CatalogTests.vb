Imports Microsoft.VisualStudio.TestTools.UnitTesting

' Three admitted declarations, enough to reach the support threshold of three,
' and one string literal left open at end of line. VB strings do not span
' lines, so this is a decidable well-formedness violation.
'
' The parse fails rather than yielding whatever happened to match: the file
' contributes no anchor, and the operator is told. Without that signal, three
' missing units would read exactly like three absent tests.

<TestClass()>
Public Class CatalogTests

    <TestMethod()>
    Public Sub LoadsCatalog()
        Dim note As String = "never closed
        Assert.IsNotNull(note)
    End Sub

    <TestMethod()>
    Public Sub FiltersCatalog()
        Assert.IsTrue(True)
    End Sub

    <TestMethod()>
    Public Sub OpensProduct()
        Assert.IsTrue(True)
    End Sub

End Class
