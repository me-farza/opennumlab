% SPDX-License-Identifier: GPL-3.0-or-later
classdef MathLabHandleCounter < handle
    properties (SetAccess = private)
        Value = 0;
    end
    properties (Access = private)
        InitialValue = 0;
    end
    methods
        function obj = MathLabHandleCounter(initial)
            if nargin == 0
                initial = 0;
            end
            obj.Value = initial;
            obj.InitialValue = initial;
        end
        function value = increment(obj, amount)
            if nargin < 2
                amount = 1;
            end
            obj.Value = obj.Value + amount;
            value = obj.Value;
        end
        function reset(obj)
            obj.Value = obj.InitialValue;
        end
    end
end
