% SPDX-License-Identifier: GPL-3.0-or-later
function [step, read, reset] = mathlab_make_counter(seed)
    current = seed;
    step = @advance;
    read = @read_current;
    reset = @reset_current;
    function result = advance(increment)
        if nargin == 0
            increment = 1;
        end
        current = current + increment;
        result = current;
    end
    function result = read_current()
        result = current;
    end
    function reset_current()
        current = seed;
    end
end
