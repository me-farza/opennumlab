% Original deterministic fixture for basic MATLAB-style array semantics.
% SPDX-License-Identifier: GPL-3.0-or-later
fixture_matrix = reshape(1:6, 2, 3);
assert(isequal(fixture_matrix, [1, 3, 5; 2, 4, 6]));
assert(fixture_matrix(4) == 4);
assert(isequal(fixture_matrix(:, 2), [3; 4]));
assert(isequal(fixture_matrix(1, end), 5));
fixture_complex = [1+2i, 3-4i];
assert(isequal(fixture_complex', [1-2i; 3+4i]));
assert(isequal(fixture_complex.', [1+2i; 3-4i]));
assert(isequal(fixture_matrix .* fixture_matrix, [1, 9, 25; 4, 16, 36]));
assert(isequal(size(fixture_matrix), [2, 3]));
assert(isequal(sum(fixture_matrix, 1), [3, 7, 11]));
assert(isequal(fixture_matrix(fixture_matrix > 4), [5; 6]));
fixture_cell = {42, 'text'};
fixture_struct = struct('value', fixture_cell{1}, 'label', fixture_cell{2});
assert(fixture_struct.value == 42);
assert(strcmp(fixture_struct.label, 'text'));
fixture_total = 0;
for fixture_index = 1:5
    if mod(fixture_index, 2) == 1
        fixture_total = fixture_total + fixture_index;
    end
end
assert(fixture_total == 9);
fixture_square = @(value) value .^ 2;
assert(isequal(fixture_square([2, 3]), [4, 9]));
fixture_caught = false;
try
    error('MathLab:Fixture', 'expected fixture error');
catch fixture_error
    fixture_caught = strcmp(fixture_error.identifier, 'MathLab:Fixture');
end
assert(fixture_caught);
disp('PASS language_core');
