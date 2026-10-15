% First-order control-system response; requires Octave control or MATLAB's
% Control System Toolbox. MathLab does not install optional packages for you.
% SPDX-License-Identifier: GPL-3.0-or-later

if exist('OCTAVE_VERSION', 'builtin')
    pkg('load', 'control');
end

%% A unit-gain first-order system: G(s) = 1 / (s + 1)
system_model = tf(1, [1, 1]);
requested_time = (0:0.02:6)';
[response, response_time] = step(system_model, requested_time);
analytic_response = 1 - exp(-response_time);
assert(max(abs(response - analytic_response)) < 1e-6);

figure('Name', 'Control response');
plot(response_time, response, 'b-', response_time, analytic_response, 'r--');
grid on;
xlabel('Time (s)');
ylabel('Output');
title('Unit step response: G(s) = 1 / (s + 1)');
legend('Computed response', 'Analytic response', 'Location', 'southeast');
fprintf('Final response at %.2f s: %.6f\n', response_time(end), response(end));
