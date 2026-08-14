unit Tests.Catalog;

interface

uses
  System.SysUtils,
  System.Classes;

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
  end;

implementation

procedure TCatalogTests.LoadsCatalog;
begin
end;

procedure TCatalogTests.FiltersCatalog;
begin
end;

procedure TCatalogTests.SortsCatalog;
begin
end;

end.
