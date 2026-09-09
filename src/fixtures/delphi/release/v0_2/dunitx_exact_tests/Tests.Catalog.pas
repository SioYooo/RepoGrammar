unit Tests.Catalog;

interface

uses
  System.SysUtils,
  DUnitX.TestFramework;

type
  [TestFixture]
  TCatalogTests = class
  public
    [Test]
    procedure LoadsCatalog;
    [Test]
    procedure FiltersCatalog;
    [Test]
    procedure SortsCatalog;
    // A helper is not a test: it carries no attribute.
    procedure BuildFixtureData;
    // A function is not a DUnitX test procedure.
    [Test]
    function CatalogSize: Integer;
  end;

  // A plain class clears the fixture state, so its attributed procedure is
  // not discovered.
  TCatalogHelper = class
  public
    [Test]
    procedure NotDiscovered;
  end;

implementation

{ The attribute text below is a comment and must never anchor.
  [Test]
  procedure InsideABlockComment;
}

procedure TCatalogTests.LoadsCatalog;
begin
  Assert.AreEqual('[Test]', '[Test]');
end;

procedure TCatalogTests.FiltersCatalog;
begin
  Assert.IsTrue(True);
end;

procedure TCatalogTests.SortsCatalog;
begin
  Assert.IsTrue(True);
end;

procedure TCatalogTests.BuildFixtureData;
begin
end;

function TCatalogTests.CatalogSize: Integer;
begin
  Result := 0;
end;

procedure TCatalogHelper.NotDiscovered;
begin
end;

initialization
  TDUnitX.RegisterTestFixture(TCatalogTests);

end.
