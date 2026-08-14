classdef CatalogTest < matlab.unittest.TestCase
    methods (Test)
        function loadsCatalog(testCase)
            v = [a' b'];
            w = [a 'end'];
            % function inAComment(testCase)
            testCase.verifyTrue(true);
        end
        function filtersCatalog(testCase)
            if true, x = 1; end
            data = testCase.Items(2:end);
            testCase.verifyNotEmpty(data);
        end
        function sortsCatalog(testCase)
            testCase.verifyTrue(true);
        end
    end

    methods (Access = private)
        function helper(testCase)
        end
    end
end

function localHelper()
end
