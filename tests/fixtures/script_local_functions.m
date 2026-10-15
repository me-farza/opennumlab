% SPDX-License-Identifier: GPL-3.0-or-later
% Local definitions resolve before they are encountered and remain file-scoped.
local_input = [1 2 3 4];
[local_sum, local_mean] = local_statistics(local_input);
assert(local_sum == 10);
assert(local_mean == 2.5);
local_handle = @local_statistics;
assert(local_handle([2 4]) == 6);
assert(local_nested(3) == 16);
local_comment = 'function y = fake(); end';
local_transpose = local_input';
assert(isequal(size(local_transpose), [4 1]));

function [total, average] = local_statistics(values)
    total = local_total(values);
    average = total / numel(values);
end

function result = local_total(values)
    result = 0;
    for index = 1:numel(values)
        result = result + values(index);
    end
end

function result = local_nested(value)
    offset = 4;
    result = square(value + 1);
    function squared = square(input)
        squared = input * offset;
    end
end
