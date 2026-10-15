% SPDX-License-Identifier: GPL-3.0-or-later
% Original forward-local script and retained anonymous-closure fixture.
result = struct();
[initial, retained_offsetter] = make_offsetter(4);
result.initial = initial;
result.first = retained_offsetter(3);
result.second = retained_offsetter(5);
factor = 2;
captured_scale = @(value) value * factor;
factor = 99;
result.anonymous_capture = captured_scale(6);
result.forward = increment_local(8);
assert(result.initial == 4 && result.first == 7 && result.second == 9);
assert(result.anonymous_capture == 12 && result.forward == 9);

function [initial, offsetter] = make_offsetter(seed)
    initial = seed;
    offsetter = @(step) seed + step;
end

function output = increment_local(value)
    output = value + 1;
end
