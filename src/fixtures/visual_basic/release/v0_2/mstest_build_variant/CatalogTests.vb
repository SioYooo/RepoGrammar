Imports Microsoft.VisualStudio.TestTools.UnitTesting

' Four attributed declarations, and not one of them is proven. `#If` selects a
' branch at compile time from a constant RepoGrammar does not evaluate, so
' anchoring every branch would invent two members that never compile, and
' anchoring one would assert a constant we do not know.
'
' Read naively, four anchors would clear the support threshold of three and
' form a family. The correct answer is no family at all, and a typed
' BuildVariantAmbiguity saying why.

<TestClass()>
Public Class CatalogTests

#If DEBUG Then

    <TestMethod()>
    Public Sub LoadsCatalogDebug()
        Assert.IsTrue(True)
    End Sub

    <TestMethod()>
    Public Sub FiltersCatalogDebug()
        Assert.IsTrue(True)
    End Sub

#Else

    <TestMethod()>
    Public Sub LoadsCatalogRelease()
        Assert.IsTrue(True)
    End Sub

    <TestMethod()>
    Public Sub FiltersCatalogRelease()
        Assert.IsTrue(True)
    End Sub

#End If

End Class
