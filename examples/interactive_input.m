% Enable MATHLAB_INPUT_BRIDGE before launching Open MathLab.
% Expression input uses this script's workspace; string input preserves text.
name = input('Your name: ', 's');
default_gain = 2;
gain = input('Gain (try default_gain+1, or Enter for 2): ');
if isempty(gain)
  gain = default_gain;
end
validateattributes(gain, {'numeric'}, {'real', 'finite', 'scalar'});
t = linspace(0, 2*pi, 400);
signal = gain * sin(t);
plot(t, signal, 'LineWidth', 1.5);
grid on;
xlabel('Time');
ylabel('Amplitude');
title(sprintf('%s: gain = %g', name, gain));
fprintf('Hello %s. The plot uses gain %g.\n', name, gain);
