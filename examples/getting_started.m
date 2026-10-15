% Open MathLab: solve a system, draw a figure, and round-trip a MAT file.
% This example uses ordinary MATLAB syntax and does not need a toolbox.
% SPDX-License-Identifier: GPL-3.0-or-later

%% Solve A*x = b
A = [3, 1; 1, 2];
b = [9; 8];
x = A \ b;
residual = norm(A * x - b);
assert(residual < 1e-12);
fprintf('Solution: x = [%.6f; %.6f], residual = %.3g\n', x(1), x(2), residual);

%% Draw a damped oscillation
t = linspace(0, 10, 501);
y = exp(-0.2 * t) .* sin(2 * pi * t);
figure('Name', 'Getting started');
plot(t, y, 'b-', 'LineWidth', 1.5);
grid on;
xlabel('Time (s)');
ylabel('Amplitude');
title('Damped oscillation');

%% Save and restore a MATLAB Version 7 MAT file
mat_path = [tempname(), '.mat'];
save(mat_path, 'A', 'b', 'x', 't', 'y', '-v7');
restored = load(mat_path);
assert(isequal(restored.A, A));
assert(isequal(restored.x, x));
assert(isequal(restored.y, y));
fprintf('Verified MAT round trip: %s\n', mat_path);
fprintf('Inspect A, x, t, and y in the Workspace panel.\n');
