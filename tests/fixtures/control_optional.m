% Optional control-package test with a known analytic response.
% SPDX-License-Identifier: GPL-3.0-or-later
if exist('OCTAVE_VERSION', 'builtin')
    pkg('load', 'control');
end
fixture_system = tf(1, [1, 1]);
fixture_t = (0:0.05:6)';
[fixture_response, fixture_time] = step(fixture_system, fixture_t);
assert(max(abs(fixture_response - (1 - exp(-fixture_time)))) < 1e-6);
assert(abs(dcgain(fixture_system) - 1) < 1e-12);
disp('PASS control_optional');
