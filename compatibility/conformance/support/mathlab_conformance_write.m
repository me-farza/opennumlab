% SPDX-License-Identifier: GPL-3.0-or-later
function mathlab_conformance_write(output_path, case_id, result, error_identifier, error_message)
% Original bounded-fixture codec shared by MATLAB and Octave drivers.
% This is a record writer, not the application's variable transport protocol.
    if ~isstruct(result) || ~isscalar(result)
        error('MathLab:ConformanceRecord', 'The fixture result must be a scalar struct.');
    end
    names = sort(fieldnames(result));
    values = cell(1, numel(names));
    for field_index = 1:numel(names)
        value = result.(names{field_index});
        if ~(isnumeric(value) || islogical(value))
            error('MathLab:ConformanceRecord', 'Only numeric and logical fixture fields are supported.');
        end
        entry = struct();
        entry.name = names{field_index};
        entry.class = class(value);
        entry.shape = num2cell(size(value));
        entry.complex = ~isreal(value);
        if issparse(value)
            entry.storage = 'sparse';
            [rows, columns, components] = find(value);
            entry.rows = num2cell(reshape(rows, 1, []));
            entry.columns = num2cell(reshape(columns, 1, []));
        else
            entry.storage = 'dense';
            components = value(:);
            entry.rows = {};
            entry.columns = {};
        end
        entry.real = component_strings(real(components));
        if entry.complex
            entry.imag = component_strings(imag(components));
        else
            entry.imag = {};
        end
        values{field_index} = entry;
    end
    record = struct();
    record.schema_version = 1;
    record.case_id = case_id;
    if isempty(error_identifier) && isempty(error_message)
        record.outcome = 'success';
    else
        record.outcome = 'error';
    end
    record.values = values;
    record.error = struct('identifier', error_identifier, 'message', error_message);
    bytes = unicode2native(jsonencode(record), 'UTF-8');
    if numel(bytes) > 1024 * 1024
        error('MathLab:ConformanceWrite', 'The typed fixture record exceeds 1 MiB.');
    end
    directory = fileparts(output_path);
    if isempty(directory)
        directory = '.';
    end
    temporary = tempname(directory);
    stream = fopen(temporary, 'wb');
    if stream < 0
        error('MathLab:ConformanceWrite', 'Cannot create the typed record.');
    end
    try
        count = fwrite(stream, bytes, 'uint8');
        if count ~= numel(bytes)
            error('MathLab:ConformanceWrite', 'Incomplete typed record write.');
        end
        status = fclose(stream);
        stream = -1;
        if status ~= 0
            error('MathLab:ConformanceWrite', 'Cannot close the typed record.');
        end
        [ok, message] = movefile(temporary, output_path, 'f');
        if ~ok
            error('MathLab:ConformanceWrite', 'Cannot publish the typed record: %s', message);
        end
    catch caught
        if stream >= 0
            fclose(stream);
        end
        if exist(temporary, 'file') == 2
            delete(temporary);
        end
        rethrow(caught);
    end
end

function strings = component_strings(values)
    strings = cell(1, numel(values));
    for index = 1:numel(values)
        value = values(index);
        if isinteger(value)
            if value < 0
                % Add before negating so intmin('int64') never overflows.
                magnitude = uint64(-(int64(value) + int64(1))) + uint64(1);
                strings{index} = ['-', unsigned_decimal(magnitude)];
            else
                strings{index} = unsigned_decimal(uint64(value));
            end
        elseif islogical(value)
            strings{index} = unsigned_decimal(uint64(value));
        elseif isnan(value)
            strings{index} = 'NaN';
        elseif isinf(value)
            if value < 0
                strings{index} = '-Inf';
            else
                strings{index} = 'Inf';
            end
        elseif isa(value, 'single')
            strings{index} = sprintf('%.9g', value);
        else
            strings{index} = sprintf('%.17g', value);
        end
    end
end

function text = unsigned_decimal(value)
    % Binary long conversion keeps every intermediate decimal digit below 20.
    % It avoids both floating conversion and runtime-specific uint64 division.
    digits = 0;
    for bit_index = 64:-1:1
        carry = double(bitget(value, bit_index));
        for digit_index = numel(digits):-1:1
            next = 2 * digits(digit_index) + carry;
            digits(digit_index) = rem(next, 10);
            carry = floor(next / 10);
        end
        if carry ~= 0
            digits = [carry, digits]; %#ok<AGROW>
        end
    end
    text = char(digits + double('0'));
end
