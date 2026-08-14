with Ada.Text_IO;

package body Catalog_Tests is

   procedure Register_Tests (T : in out Test_Case) is
   begin
      Register_Routine (T, Loads_Catalog'Access, "loads the catalog");
      Register_Routine (T, Filters_Catalog'Access, "filters the catalog");
      Register_Routine (T, Sorts_Catalog'Access, "sorts the catalog");
   end Register_Tests;

end Catalog_Tests;
