% Set a breakpoint on line 4, then inspect, step and continue.
% SPDX-License-Identifier: GPL-3.0-or-later
debug_input = 3;
debug_squared = debug_input ^ 2;
debug_result = debug_squared + 1;
assert(debug_result == 10);
disp('Debugger example completed.');
