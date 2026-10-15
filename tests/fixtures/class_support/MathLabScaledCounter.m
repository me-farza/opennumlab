% SPDX-License-Identifier: GPL-3.0-or-later
classdef MathLabScaledCounter < MathLabHandleCounter
    properties
        Scale = 1;
    end
    methods
        function obj = MathLabScaledCounter(initial, scale)
            obj@MathLabHandleCounter(initial);
            obj.Scale = scale;
        end
        function value = increment(obj, amount)
            if nargin < 2
                amount = 1;
            end
            value = increment@MathLabHandleCounter(obj, amount * obj.Scale);
        end
    end
end
