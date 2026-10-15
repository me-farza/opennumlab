% SPDX-License-Identifier: GPL-3.0-or-later
% Original bounded complex, sparse, logical and empty-shape observations.
result = struct();
result.complex_double = complex([1, -2; 0, 0.5], [3, 0; -4, 0.25]);
result.complex_single = complex(single([1, -2; 0, 0.5]), single([3, 0; -4, 0.25]));
result.sparse_real = sparse([1, 3, 2], [1, 2, 4], [3, -4, 0.5], 3, 4);
result.sparse_complex = sparse([1, 3], [2, 4], complex([2, -1], [3, 4]), 3, 4);
result.sparse_logical = logical(sparse([1, 3], [2, 4], [1, 1], 3, 4));
result.logical = [true, false; false, true];
result.empty = zeros(0, 3);

assert(isa(result.complex_double, 'double') && ~isreal(result.complex_double));
assert(isa(result.complex_single, 'single') && ~isreal(result.complex_single));
assert(isequal(size(result.complex_single), [2, 2]));
assert(result.complex_double(2, 1) == complex(0, -4));
assert(isequal(double(result.complex_single), result.complex_double));
assert(issparse(result.sparse_real) && isa(result.sparse_real, 'double'));
assert(isequal(size(result.sparse_real), [3, 4]) && nnz(result.sparse_real) == 3);
assert(isequal(full(result.sparse_real), [3, 0, 0, 0; 0, 0, 0, 0.5; 0, -4, 0, 0]));
assert(issparse(result.sparse_complex) && ~isreal(result.sparse_complex));
assert(result.sparse_complex(3, 4) == complex(-1, 4));
assert(issparse(result.sparse_logical) && islogical(result.sparse_logical));
assert(isequal(size(result.empty), [0, 3]));
