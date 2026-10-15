% A deterministic two-tone signal and its single-sided amplitude spectrum.
% Uses the core FFT; no signal-processing package is required.
% SPDX-License-Identifier: GPL-3.0-or-later

%% Sample a 50 Hz and a 120 Hz signal at 1 kHz
fs = 1000;
n = 1000;
t = (0:n-1) / fs;
signal = 0.7 * sin(2 * pi * 50 * t) + sin(2 * pi * 120 * t);

%% Compute a correctly normalized spectrum
transform = fft(signal);
spectrum = abs(transform / n);
amplitude = spectrum(1:n/2+1);
amplitude(2:end-1) = 2 * amplitude(2:end-1);
frequency = fs * (0:n/2) / n;
assert(abs(amplitude(51) - 0.7) < 1e-12);
assert(abs(amplitude(121) - 1.0) < 1e-12);

%% Compare time and frequency domains
figure('Name', 'Signal and spectrum');
subplot(2, 1, 1);
plot(t(1:100), signal(1:100));
grid on;
xlabel('Time (s)');
ylabel('Amplitude');
title('Two-tone signal');
subplot(2, 1, 2);
plot(frequency, amplitude);
grid on;
xlim([0, 200]);
xlabel('Frequency (Hz)');
ylabel('Amplitude');
title('Single-sided amplitude spectrum');
fprintf('Verified peaks: 50 Hz = %.3f, 120 Hz = %.3f\n', amplitude(51), amplitude(121));
