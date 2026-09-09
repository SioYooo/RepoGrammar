unit Tests.Solo;

interface

uses
  DUnitX.TestFramework;

type
  [TestFixture]
  TSoloTests = class
  public
    [Test]
    procedure LoadsCatalog;
    [Test]
    procedure FiltersCatalog;
  end;

implementation

procedure TSoloTests.LoadsCatalog;
begin
end;

procedure TSoloTests.FiltersCatalog;
begin
end;

end.
