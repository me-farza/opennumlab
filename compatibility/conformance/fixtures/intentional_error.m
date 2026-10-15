% SPDX-License-Identifier: GPL-3.0-or-later
% The runner must retain typed assignments preceding an authentic user error.
result = struct();
result.pre_error = uint32([7, 11]);
error('MathLab:ConformanceIntentional', 'Original conformance failure for transport testing.');
