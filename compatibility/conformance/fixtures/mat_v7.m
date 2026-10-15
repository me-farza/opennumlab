% SPDX-License-Identifier: GPL-3.0-or-later
% Original typed MAT7 self-roundtrip; optional external file enables interchange.
payload = struct();
payload.exact = uint64(9007199254740992) + uint64(17);
payload.unsigned_limits = [uint64(0), intmax('uint64')];
payload.signed_limits = [intmin('int64'), intmax('int64')];
payload.cube = reshape(uint8(1:24), [2, 3, 4]);
payload.complex_double = complex([1, 0; -2, 0.5], [0, 3; 4, -0.25]);
payload.single = single([0.5, -2; 4, 8]);
payload.logical = [true, false; false, true];
payload.sparse_real = sparse([1, 3], [2, 4], [2, -1], 3, 4);
payload.sparse_complex = sparse([1, 3], [2, 4], complex([2, -1], [3, 4]), 3, 4);
payload.empty = zeros(0, 3, 2);
payload.nonfinite = [NaN, Inf, -Inf];
mat_work_directory = pwd;
if exist('mathlab_conformance_work_directory', 'var') == 1
    mat_work_directory = mathlab_conformance_work_directory;
end
mat_output = fullfile(mat_work_directory, 'conformance-values.mat');
save(mat_output, '-struct', 'payload', '-v7');
mat_input = mat_output;
if exist('mathlab_conformance_mat_input', 'var') == 1 && ~isempty(mathlab_conformance_mat_input)
    mat_input = mathlab_conformance_mat_input;
end
result = load(mat_input);
expected_names = sort(fieldnames(payload));
assert(isequal(sort(fieldnames(result)), expected_names));
for field_index = 1:numel(expected_names)
    field_name = expected_names{field_index};
    expected_value = payload.(field_name);
    actual_value = result.(field_name);
    assert(strcmp(class(actual_value), class(expected_value)));
    assert(isequal(size(actual_value), size(expected_value)));
    assert(issparse(actual_value) == issparse(expected_value));
    assert(isreal(actual_value) == isreal(expected_value));
    assert(isequaln(actual_value, expected_value));
end
