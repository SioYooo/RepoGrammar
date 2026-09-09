with AUnit.Test_Cases;

package body Solo_Tests is

   procedure Register_Tests (T : in out Test_Case) is
   begin
      Register_Routine (T, Loads_Catalog'Access, "loads the catalog");
      Register_Routine (T, Filters_Catalog'Access, "filters the catalog");
   end Register_Tests;

end Solo_Tests;
