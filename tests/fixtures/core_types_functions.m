% Original assertions for numerical/container types, functions, and classdef.
% Tested against Octave; this is not a MATLAB reference conformance claim.
% SPDX-License-Identifier: GPL-3.0-or-later
core_support_directory = fullfile(fileparts(mfilename('fullpath')), 'class_support');
core_original_path = path();
addpath(core_support_directory);
try
    % Column-major, trailing-dimension folding, logical indexing, and deletion.
    core_matrix = reshape(1:12, 3, 4);
    assert(isequal(core_matrix([3 1], [4 2]), [12 6; 10 4]));
    assert(isequal(core_matrix(end-1, 1:end-1), [2 5 8]));
    core_tensor = reshape(1:24, 2, 3, 4);
    assert(core_tensor(2, end) == 24);
    assert(isequal(core_tensor(:, end), [23; 24]));
    assert(isequal(core_matrix(core_matrix > 9), [10; 11; 12]));
    core_deleted = core_matrix;
    core_deleted(2, :) = [];
    assert(isequal(core_deleted, [1 4 7 10; 3 6 9 12]));
    core_expanded = zeros(1, 2);
    core_expanded(4) = 7;
    assert(isequal(core_expanded, [0 0 0 7]));

    % Cell contents and cell slices have distinct types and comma-list behavior.
    core_cells = {uint8(7), 'text', [2 4]};
    [core_first, core_second] = core_cells{[3 1]};
    assert(isequal(core_first, [2 4]));
    assert(isa(core_second, 'uint8') && core_second == 7);
    assert(iscell(core_cells(2)) && ischar(core_cells{2}));
    core_cells(2) = {99};
    core_cells{end+1} = struct('id', 4);
    assert(core_cells{2} == 99 && core_cells{4}.id == 4);
    core_records = struct('name', {'first', 'second'}, 'score', {2, 5});
    assert(isequal([core_records.score], [2 5]));
    core_field = 'score';
    assert(core_records(1).(core_field) == 2);
    core_record_copy = core_records;
    core_record_copy(1).score = 99;
    assert(core_records(1).score == 2 && core_record_copy(1).score == 99);

    % Integer saturation, precision classes, conjugation, and sparse algebra.
    core_unsigned = uint8([0 128 255]) + uint8([1 128 1]);
    assert(isa(core_unsigned, 'uint8'));
    assert(isequal(core_unsigned, uint8([1 255 255])));
    assert(isequal(int8([1.5 -1.5]), int8([2 -2])));
    core_single = single([1 2; 3 4]);
    assert(isa(core_single * single(2), 'single'));
    core_complex = complex(single([1 2]), single([3 4]));
    assert(isa(core_complex, 'single') && ~isreal(core_complex));
    assert(isequal(core_complex', conj(core_complex.')));
    core_sparse = sparse([1 1 2], [2 2 1], [2 3 4], 2, 2);
    assert(issparse(core_sparse) && nnz(core_sparse) == 2);
    assert(isequal(full(core_sparse), [0 5; 4 0]));
    core_sparse(1, 2) = 0;
    assert(nnz(core_sparse) == 1);
    core_sparse_complex = spdiags([2; 3], 0, 2, 2) + 1i * speye(2);
    core_sparse_rhs = [2+1i; 3+1i];
    assert(norm(core_sparse_complex \ core_sparse_rhs - ones(2, 1)) < 1e-12);
    core_sparse_logical = sparse(logical([1 0; 0 1]));
    assert(issparse(core_sparse_logical) && islogical(core_sparse_logical));
    assert(isequaln([NaN Inf -Inf], [NaN Inf -Inf]));

    % Anonymous functions capture values; handle-object captures retain identity.
    core_offset = 10;
    core_anonymous = @(value) value + core_offset;
    core_offset = 100;
    assert(core_anonymous(3) == 13);
    assert(feval(core_anonymous, 4) == 14);
    assert(isequal(arrayfun(@(value) value^2, 1:4), [1 4 9 16]));
    [core_step, core_read, core_reset] = mathlab_make_counter(3);
    [core_other_step, core_other_read] = mathlab_make_counter(100);
    assert(core_step(2) == 5 && core_read() == 5);
    assert(core_other_step() == 101 && core_other_read() == 101);
    core_reset();
    assert(core_read() == 3 && core_other_read() == 101);

    % Multiple outputs and varargs report their actual invocation arity.
    [core_total, core_metadata, core_extra_one, core_extra_two] = ...
        mathlab_core_outputs(1, 2, 3);
    assert(core_total == 6 && core_extra_one == 7 && core_extra_two == 8);
    assert(core_metadata.input_count == 3 && core_metadata.output_count == 4);
    assert(isequal(core_metadata.tail, {2, 3}));
    assert(mathlab_core_outputs(10) == 10);
    assert(nargin(@mathlab_core_outputs) == -2);
    assert(nargout(@mathlab_core_outputs) == -3);
    [core_total, core_metadata] = feval('mathlab_core_outputs', 4, 5);
    assert(core_total == 9 && core_metadata.output_count == 2);
    core_named_input = 42;
    [core_total, core_metadata] = mathlab_core_outputs(core_named_input);
    assert(core_total == 42 && strcmp(core_metadata.first_name, 'core_named_input'));
    [~, core_metadata, ~] = mathlab_core_outputs(2, 3);
    assert(core_metadata.output_count == 3 && core_metadata.input_count == 2);

    % Value classes copy state, dependent setters update a returned value,
    % private properties enforce access, and operators/static methods dispatch.
    core_value = MathLabValueVector([3 4]);
    core_value_copy = core_value;
    core_value_copy.Elements = [6 8];
    assert(core_value.Magnitude == 5 && core_value_copy.Magnitude == 10);
    core_shifted = core_value.shift(1);
    assert(isequal(core_shifted.Elements, [4 5]));
    assert(isequal(core_value.Elements, [3 4]));
    core_sum_value = core_value + core_value_copy;
    assert(isequal(core_sum_value.Elements, [9 12]));
    core_object_array = [core_value core_value_copy];
    assert(isa(core_object_array, 'MathLabValueVector'));
    assert(core_object_array(1).Magnitude == 5 && core_object_array(2).Magnitude == 10);
    core_object_column = reshape(core_object_array, 2, 1);
    assert(isequal(size(core_object_column), [2 1]));
    assert(core_object_column(2).Magnitude == 10);
    core_unit = MathLabValueVector.unit(3);
    assert(isequal(core_unit.Elements, ones(1, 3)));
    core_private_rejected = false;
    try
        core_private_value = core_value.Components;
    catch
        core_private_rejected = true;
    end
    assert(core_private_rejected);

    % Handle aliasing, captured object identity, inheritance, and super calls.
    core_handle = MathLabHandleCounter(5);
    core_handle_alias = core_handle;
    core_handle_read = @() core_handle.Value;
    assert(core_handle_alias.increment(2) == 7);
    assert(core_handle.Value == 7 && core_handle_read() == 7);
    core_private_set_rejected = false;
    try
        core_handle.Value = 999;
    catch
        core_private_set_rejected = true;
    end
    assert(core_private_set_rejected && core_handle.Value == 7);
    core_handle.reset();
    assert(core_handle_alias.Value == 5 && core_handle_read() == 5);
    core_scaled = MathLabScaledCounter(1, 3);
    assert(isa(core_scaled, 'MathLabHandleCounter') && isa(core_scaled, 'handle'));
    assert(core_scaled.increment(2) == 7);
    core_scaled.reset();
    assert(core_scaled.Value == 1);
catch core_error
    path(core_original_path);
    rethrow(core_error);
end
path(core_original_path);
disp('PASS core_types_functions');
