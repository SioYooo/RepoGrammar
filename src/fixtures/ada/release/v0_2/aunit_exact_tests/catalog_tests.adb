with AUnit.Assertions;
with AUnit.Test_Cases;

package body Catalog_Tests is

   Tick  : constant Character := ''';
   Paren : constant Character := '(';
   Note  : constant String := "Register_Routine (T, In_A_String'Access, ""no"")";

   procedure Loads_Catalog (T : in out AUnit.Test_Cases.Test_Case'Class) is
   begin
      AUnit.Assertions.Assert (True, "loads");
   end Loads_Catalog;

   procedure Filters_Catalog (T : in out AUnit.Test_Cases.Test_Case'Class) is
   begin
      AUnit.Assertions.Assert (Items (1)'Length > 0, "filters");
   end Filters_Catalog;

   procedure Sorts_Catalog (T : in out AUnit.Test_Cases.Test_Case'Class) is
   begin
      AUnit.Assertions.Assert (True, "sorts");
   end Sorts_Catalog;

   procedure Helper (T : in out AUnit.Test_Cases.Test_Case'Class) is
   begin
      null;
   end Helper;

   procedure Register_Tests (T : in out Test_Case) is
   begin
      --  Register_Routine (T, In_A_Comment'Access, "no");
      Register_Routine (T, Loads_Catalog'Access, "loads the catalog");
      Registration.Register_Routine
        (T, Filters_Catalog'Access, "filters the catalog");
      AUnit.Test_Cases.Registration.Register_Routine
        (T, Sorts_Catalog'Access, "sorts the catalog");
      --  A variable is not an access attribute, so this is not admitted.
      Register_Routine (T, Helper_Ptr, "helper");
   end Register_Tests;

end Catalog_Tests;
