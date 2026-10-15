% SPDX-License-Identifier: GPL-3.0-or-later
% Original negative regression: file I/O must preserve sparse logical values.
result = struct();
result.original = logical(sparse([1, 3], [2, 4], [1, 1], 3, 4));
mat_work_directory = pwd;
if exist('mathlab_conformance_work_directory', 'var') == 1
    mat_work_directory = mathlab_conformance_work_directory;
end
mat_output = fullfile(mat_work_directory, 'conformance-sparse-logical.mat');
sparse_value = result.original;
save(mat_output, 'sparse_value', '-v7');
loaded_file = load(mat_output);
result.loaded = loaded_file.sparse_value;
assert(strcmp(class(result.loaded), class(result.original)), 'MAT7 sparse logical class must be preserved.');
assert(isequal(size(result.loaded), size(result.original)), 'MAT7 sparse logical shape must be preserved.');
assert(issparse(result.loaded) == issparse(result.original), 'MAT7 sparse logical storage must be preserved.');
assert(isequal(result.loaded, result.original), 'MAT7 sparse logical values must be preserved.');
