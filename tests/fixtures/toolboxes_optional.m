% Original deterministic assertions for an explicitly loaded package profile.
% On MATLAB these APIs require their respective installed MathWorks toolboxes.
% SPDX-License-Identifier: GPL-3.0-or-later
if exist('OCTAVE_VERSION', 'builtin')
    pkg('load', 'control', 'signal', 'image', 'statistics', 'optim');
end

%% Signal: Butterworth cutoff and a known first-order filter response
[fixture_filter_b, fixture_filter_a] = butter(4, 0.2);
fixture_frequency_response = freqz(fixture_filter_b, fixture_filter_a, [0, 0.2*pi, pi]);
assert(abs(abs(fixture_frequency_response(1)) - 1) < 1e-10);
assert(abs(abs(fixture_frequency_response(2)) - 1/sqrt(2)) < 1e-10);
assert(abs(fixture_frequency_response(3)) < 1e-10);
fixture_decay = exp(-0.1);
fixture_filter_response = filter(1-fixture_decay, [1, -fixture_decay], ones(1, 100));
assert(max(abs(fixture_filter_response - (1 - fixture_decay .^ (1:100)))) < 1e-12);
disp('PASS optional signal: butter, freqz, filter');

%% Control: a first-order step response and its static gain
fixture_system = tf(1, [1, 1]);
fixture_requested_time = (0:0.05:6)';
[fixture_response, fixture_response_time] = step(fixture_system, fixture_requested_time);
assert(max(abs(fixture_response - (1 - exp(-fixture_response_time)))) < 1e-6);
assert(abs(dcgain(fixture_system) - 1) < 1e-12);
disp('PASS optional control: tf, step, dcgain');

%% Image: connectivity and dilation of a single foreground pixel
fixture_binary = logical(eye(3));
[fixture_labels4, fixture_components4] = bwlabel(fixture_binary, 4);
[fixture_labels8, fixture_components8] = bwlabel(fixture_binary, 8);
assert(fixture_components4 == 3);
assert(fixture_components8 == 1);
assert(all(fixture_labels4(~fixture_binary) == 0));
assert(all(fixture_labels8(fixture_binary) == 1));
fixture_impulse_image = false(5);
fixture_impulse_image(3, 3) = true;
fixture_expected_dilation = false(5);
fixture_expected_dilation(2:4, 2:4) = true;
fixture_dilation = imdilate(fixture_impulse_image, strel('square', 3));
assert(isequal(fixture_dilation, fixture_expected_dilation));
disp('PASS optional image: bwlabel, strel, imdilate');

%% Statistics: known standard-normal values and exact regression coefficients
assert(abs(normcdf(0) - 0.5) < 1e-12);
assert(abs(normpdf(0) - 1/sqrt(2*pi)) < 1e-12);
assert(abs(norminv(0.5)) < 1e-12);
fixture_regression_time = (0:5)';
fixture_design = [ones(6, 1), fixture_regression_time];
fixture_observations = 2 + 3 * fixture_regression_time;
fixture_coefficients = regress(fixture_observations, fixture_design);
assert(norm(fixture_coefficients - [2; 3]) < 1e-12);
assert(norm(fixture_design * fixture_coefficients - fixture_observations) < 1e-12);
disp('PASS optional statistics: normcdf, normpdf, norminv, regress');

%% Optim: fit the same known model through nonlinear least-squares machinery
fixture_objective = @(parameters) fixture_design * parameters - fixture_observations;
fixture_options = optimset('TolFun', 1e-12, 'MaxIter', 100, 'Display', 'off');
[fixture_fit, fixture_resnorm, fixture_fit_residual] = lsqnonlin(fixture_objective, [0; 0], [], [], fixture_options);
assert(norm(fixture_fit - [2; 3]) < 1e-6);
assert(fixture_resnorm < 1e-12);
assert(norm(fixture_fit_residual) < 1e-6);
disp('PASS optional optim: lsqnonlin');
disp('PASS toolboxes_optional');
