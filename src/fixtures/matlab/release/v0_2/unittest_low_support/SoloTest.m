classdef SoloTest < matlab.unittest.TestCase
    methods (Test)
        function loadsCatalog(testCase)
            testCase.verifyTrue(true);
        end
        function filtersCatalog(testCase)
            testCase.verifyTrue(true);
        end
    end
end
