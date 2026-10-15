%% A familiar scientific workflow
% Open MathLab runs MATLAB-style scripts using GNU Octave.
% Press F5 to run this file, or Ctrl+Enter to run a section.

%% Solve a linear system
A = [4 1 2; 1 3 0; 2 0 5];
b = [7; 4; 9];
x = A \ b;
disp('Solution of A*x = b:');
disp(x);
residual = norm(A*x - b);

%% Visualize a damped oscillation
t = linspace(0, 8, 400);
y = exp(-0.25*t) .* cos(2*pi*t);
figure(1);
plot(t, y, 'LineWidth', 2);
grid on;
xlabel('Time (s)');
ylabel('Amplitude');
title('Damped oscillation');

%% Save and restore data
% Use the Save Workspace button to save a MAT file.
% Use Load Data to restore variables into this same workspace.
