% SPDX-License-Identifier: GPL-3.0-or-later
classdef MathLabValueVector
    properties (Access = private)
        Components = [];
    end
    properties (Dependent, SetAccess = private)
        Magnitude
    end
    properties (Dependent)
        Elements
    end
    methods
        function obj = MathLabValueVector(values)
            if nargin > 0
                obj.Components = values;
            end
        end
        function value = get.Magnitude(obj)
            value = norm(obj.Components);
        end
        function value = get.Elements(obj)
            value = obj.Components;
        end
        function obj = set.Elements(obj, values)
            obj.Components = values;
        end
        function obj = shift(obj, offset)
            obj.Components = obj.Components + offset;
        end
        function result = plus(left, right)
            result = MathLabValueVector(left.Components + right.Components);
        end
    end
    methods (Static)
        function obj = unit(count)
            obj = MathLabValueVector(ones(1, count));
        end
    end
end
