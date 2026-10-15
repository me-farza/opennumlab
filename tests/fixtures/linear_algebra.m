% Residual and reconstruction checks tolerate normal floating-point rounding.
% SPDX-License-Identifier: GPL-3.0-or-later
fixture_A = [4, 1, 2; 1, 3, 0; 2, 0, 5];
fixture_b = [7; 4; 7];
fixture_solution = fixture_A \ fixture_b;
assert(norm(fixture_solution - ones(3, 1)) < 1e-12);
assert(norm(fixture_A * fixture_solution - fixture_b) < 1e-12);
[fixture_Q, fixture_R] = qr(fixture_A);
assert(norm(fixture_Q * fixture_R - fixture_A, 'fro') < 1e-12);
assert(norm(fixture_Q' * fixture_Q - eye(3), 'fro') < 1e-12);
[fixture_U, fixture_S, fixture_V] = svd(fixture_A);
assert(norm(fixture_U * fixture_S * fixture_V' - fixture_A, 'fro') < 1e-12);
fixture_chol = chol(fixture_A);
assert(norm(fixture_chol' * fixture_chol - fixture_A, 'fro') < 1e-12);
fixture_sparse = sparse([1, 2, 3], [1, 2, 3], [2, 3, 4], 3, 3);
assert(nnz(fixture_sparse) == 3);
assert(isequal(full(fixture_sparse), diag([2, 3, 4])));
assert(norm(fixture_sparse \ [2; 3; 4] - ones(3, 1)) < 1e-12);
disp('PASS linear_algebra');
