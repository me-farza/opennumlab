% MATLAB scripts can call their local functions before the definitions.
samples = linspace(0, 2*pi, 101);
[signal, amplitude] = make_signal(samples);
fprintf('Signal amplitude: %.3f\n', amplitude);
plot(samples, signal);
xlabel('Time'); ylabel('Amplitude'); title('Local function example');
grid on;

function [signal, amplitude] = make_signal(time)
    signal = sin(time) + 0.25 * sin(3*time);
    amplitude = measure_amplitude(signal);
end

function result = measure_amplitude(signal)
    result = (max(signal) - min(signal)) / 2;
end
