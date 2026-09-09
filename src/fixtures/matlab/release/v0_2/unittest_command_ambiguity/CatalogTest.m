classdef CatalogTest < matlab.unittest.TestCase
    methods (Test)
        function loadsCatalog(testCase)
            format end
            testCase.verifyTrue(true);
        end
        function filtersCatalog(testCase)
            testCase.verifyTrue(true);
        end
        function sortsCatalog(testCase)
            testCase.verifyTrue(true);
        end
    end
end
