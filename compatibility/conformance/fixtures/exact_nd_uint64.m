% SPDX-License-Identifier: GPL-3.0-or-later
% Original exact-value fixture, shared without rewriting between all routes.
result = struct();
result.exact = uint64(9007199254740992) + uint64(17);
result.unsigned_limits = [uint64(0), intmax('uint64')];
result.signed_limits = [intmin('int64'), intmax('int64')];
result.original = reshape(1:60, [3, 4, 5]);
result.copy = result.original;
result.selected_before = result.copy(2, 3, 4);
result.copy(2, 3, 4) = 444;
result.plane = result.original(:, :, 4);
result.selected_after = result.copy(2, 3, 4);

assert(isa(result.exact, 'uint64'));
assert(result.exact - uint64(9007199254740992) == uint64(17));
assert(isequal(size(result.original), [3, 4, 5]));
assert(isequal(result.plane, [37, 40, 43, 46; 38, 41, 44, 47; 39, 42, 45, 48]));
assert(result.selected_before == 44 && result.selected_after == 444);
assert(result.original(2, 3, 4) == 44);
assert(nnz(result.original ~= result.copy) == 1);
