% Impulse transform, inverse transform, and known-frequency amplitude checks.
% SPDX-License-Identifier: GPL-3.0-or-later
fixture_impulse = [1, zeros(1, 15)];
assert(norm(fft(fixture_impulse) - ones(1, 16)) < 1e-12);
fixture_n = 64;
fixture_t = (0:fixture_n-1) / fixture_n;
fixture_signal = sin(2 * pi * 5 * fixture_t);
fixture_fft = fft(fixture_signal);
assert(max(abs(ifft(fixture_fft) - fixture_signal)) < 1e-12);
assert(abs(abs(fixture_fft(6)) / fixture_n - 0.5) < 1e-12);
assert(abs(abs(fixture_fft(fixture_n-4)) / fixture_n - 0.5) < 1e-12);
fixture_fft([6, fixture_n-4]) = 0;
assert(max(abs(fixture_fft)) < 1e-10);
disp('PASS fft_core');
