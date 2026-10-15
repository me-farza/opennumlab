## SPDX-License-Identifier: GPL-3.0-or-later
## Private worker subfunctions appended to the owned worker by Engine startup.
## Append beside the owned mathlab_page_* helpers, never on a user load path.
## Caller MUST check the complete root BindingToken against current context in
## the same serialized operation before this function; it does not authorize a
## binding, advance revisions, assign in base, or mutate any history/ledger.
##
## [replacement_root, patch] = mathlab_prepare_value_edit(request)
## patch retains ONLY bounded native old/new selected pages plus normalized
## target metadata. Dense complex pages are genuine {real,imag} component pairs;
## real/sparse patches remain numerical pages. Never retain a parent/full root.
## Floating edit payloads use exact ieee754_hex, unlike decimal READ pages.
## Public-to-owner signatures:
##   [replacement_root, patch] = mathlab_prepare_value_edit(request)
##   data = mathlab_edit_patch_data(native_patch, type, complex_value,
##                                 storage = '', rows = [], columns = [])
## Dense-complex history requires storage='dense' and its exact window shape.
## Its REAL component pairs encode directly to existing canonical IEEE hex.
## patch fields: schema_version, token (PRECOMMIT), path, window, expected_leaf,
## old_value, new_value, native_patch_bytes, logical_leaf_bytes,
## ancestor_reference_units. Owner supplies current token when preparing undo.
## Limits: 2000 selected cells; inherited rank<=32/path<=16/fields<=256;
## 1048576 total ancestor reference units; 128MiB logical leaf construction;
## sparse nzmax+worst-case inserted cells<=1048576 and columns<=1048575;
## 256KiB old+new native patch accounting; each stored sparse patch capacity
## <=its selected logical cell count. Dense REAL patches are independently
## rebuilt from bounded exact component strings. Dense COMPLEX before/after
## pairs are fresh bounded REAL buffers returned by the native adapter. Neither
## path retains a selected source ArrayRep view.
## Count/byte accounting excludes allocator
## overhead and does not constrain OS RSS or native sparse growth transients.
## Required conversion: decimal 1.000000059604644775390626 parses directly
## to f32 bits 3f800001 in Rust; a decimal->double->single route gives 3f800000.
## Worker must receive 3f800001 and never perform that decimal conversion.
##
## Admission below measures logical leaf bytes, sparse capacity/column counts,
## and ancestor reference units. These are NOT physical allocation/RSS bounds.
## Sparse growth heuristics, native COW peaks, complex-zero storage, builtin
## availability and interrupt boundaries require actual runtime qualification.
function [replacement_root, patch] = mathlab_prepare_value_edit (request, value_bridge = [])
  mathlab_edit_fields (request, {'schema_version','path','window','expected_leaf','data'}, {'command','id'});
  if (! builtin('isnumeric', request.schema_version) || ! builtin('isreal', request.schema_version) || ...
      builtin('numel', request.schema_version) != 1 || request.schema_version != 1)
    builtin('error', 'MathLab:ValueEditSchema', 'Only typed edit schema version 1 is supported.');
  endif
  if (builtin('isfield', request, 'command') && ...
      (! builtin('ischar', request.command) || ! builtin('strcmp', request.command, 'write_value_range_checked')))
    builtin('error', 'MathLab:ValueEditSchema', 'Unexpected typed edit command.');
  endif
  mathlab_edit_fields (request.path, {'root','steps'}, {});
  mathlab_edit_fields (request.path.root, {'context','name'}, {});
  mathlab_edit_fields (request.path.root.context, {'worker_id','generation','invocation','revision'}, {});
  token = mathlab_page_token (request.path.root);
  steps = mathlab_page_steps (request.path.steps);
  depth = builtin('numel', steps);
  parents = builtin('cell', 1, depth);
  parent_indices = builtin('cell', 1, depth);
  parent_shapes = builtin('cell', 1, depth);
  ancestor_references = 0;
  max_ancestor_references = 1048576;
  value = mathlab_named_value (token.name);

  ## Qualify and admit every ancestor BEFORE the scalar selection or a later
  ## COW reconstruction can copy its reference slots. No object is traversed.
  for index = 1:depth
    step = steps{index};
    if (! builtin('isstruct', step) || builtin('numel', step) != 1 || ...
        ! builtin('isfield', step, 'kind') || ! builtin('ischar', step.kind))
      builtin('error', 'MathLab:ValueEditSchema', 'Expected a typed container path segment.');
    endif
    if (builtin('strcmp', step.kind, 'cell'))
      mathlab_edit_fields (step, {'kind','indices'}, {});
      if (! builtin('iscell', value) || ! builtin('strcmp', builtin('class', value), 'cell'))
        builtin('error', 'MathLab:ValueEditUnsupported', 'Edit navigation requires a genuine cell container.');
      endif
      dimensions = mathlab_page_dimensions (value);
      references = mathlab_edit_product (dimensions, max_ancestor_references - ancestor_references);
      selection = mathlab_page_container_indices (step, value);
      parents{index} = value;
      parent_indices{index} = selection;
      parent_shapes{index} = dimensions;
      ancestor_references += references;
      value = value{selection{:}};
      steps{index} = builtin('struct', 'kind', 'cell', ...
                            'indices', {mathlab_page_index_strings(step.indices)});
    elseif (builtin('strcmp', step.kind, 'field'))
      mathlab_edit_fields (step, {'kind','indices','name'}, {});
      if (! builtin('isstruct', value) || ! builtin('strcmp', builtin('class', value), 'struct'))
        builtin('error', 'MathLab:ValueEditUnsupported', 'Edit navigation requires a genuine struct container.');
      endif
      if (! mathlab_page_identifier (step.name))
        builtin('error', 'MathLab:ValueEditSchema', 'Expected a bounded ASCII field identifier.');
      endif
      fields = builtin('numfields', value);
      if (fields < 1 || fields > 256)
        builtin('error', 'MathLab:ValueEditLimit', 'Edit field navigation requires between 1 and 256 fields.');
      endif
      dimensions = mathlab_page_dimensions (value);
      ## Include scalar-field selection references as well as parent array slots.
      remaining = max_ancestor_references - ancestor_references - fields;
      if (remaining < 0)
        builtin('error', 'MathLab:ValueEditLimit', 'Edit ancestors exceed the reference-unit budget.');
      endif
      elements = mathlab_edit_product (dimensions, builtin('floor', remaining / fields));
      selection = mathlab_page_container_indices (step, value);
      parents{index} = value;
      parent_indices{index} = selection;
      parent_shapes{index} = dimensions;
      ancestor_references += elements * fields + fields;
      element = value(selection{:});
      if (! builtin('isfield', element, step.name))
        builtin('error', 'MathLab:ValueEditSelection', 'The selected struct field does not exist.');
      endif
      value = element.(step.name);
      steps{index} = builtin('struct', 'kind', 'field', ...
                            'indices', {mathlab_page_index_strings(step.indices)}, 'name', step.name);
    else
      builtin('error', 'MathLab:ValueEditUnsupported', 'This edit path segment is unsupported.');
    endif
  endfor

  type = builtin('class', value);
  classes = {'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64','logical'};
  if (! builtin('any', builtin('strcmp', type, classes)) || ...
      ! (builtin('isnumeric', value) || builtin('islogical', value)))
    builtin('error', 'MathLab:ValueEditUnsupported', 'A typed edit requires a primitive numeric/logical leaf.');
  endif
  dimensions = mathlab_page_dimensions (value);
  sparse_value = builtin('issparse', value);
  complex_value = ! builtin('isreal', value);
  if (sparse_value && (builtin('numel', dimensions) != 2 || ...
      ! builtin('any', builtin('strcmp', type, {'double','logical'}))))
    builtin('error', 'MathLab:ValueEditUnsupported', 'Sparse edits require a rank-2 double or logical leaf.');
  endif
  if (complex_value && ! builtin('any', builtin('strcmp', type, {'double','single'})))
    builtin('error', 'MathLab:ValueEditUnsupported', 'Complex edits require a single or double leaf.');
  endif
  if (sparse_value); storage = 'sparse'; else; storage = 'dense'; endif

  mathlab_edit_fields (request.expected_leaf, {'class','source_shape','source_storage','complex'}, {});
  expected = request.expected_leaf;
  if (! builtin('ischar', expected.class) || ! builtin('strcmp', expected.class, type) || ...
      ! builtin('ischar', expected.source_storage) || ! builtin('strcmp', expected.source_storage, storage) || ...
      ! builtin('islogical', expected.complex) || builtin('numel', expected.complex) != 1 || ...
      expected.complex != complex_value)
    builtin('error', 'MathLab:ValueEditStaleLeaf', 'The edit leaf type, storage or complex metadata changed.');
  endif
  encoded_shape = mathlab_page_index_strings (expected.source_shape);
  if (builtin('numel', encoded_shape) != builtin('numel', dimensions))
    builtin('error', 'MathLab:ValueEditStaleLeaf', 'The edit leaf rank changed.');
  endif
  shape = builtin('cell', 1, builtin('numel', dimensions));
  for axis = 1:builtin('numel', dimensions)
    if (mathlab_page_index (encoded_shape{axis}, builtin('true')) != dimensions(axis))
      builtin('error', 'MathLab:ValueEditStaleLeaf', 'The edit leaf dimensions changed.');
    endif
    shape{axis} = builtin('sprintf', '%.0f', dimensions(axis));
  endfor
  expected = builtin('struct', 'class', type, 'source_shape', {shape}, ...
                     'source_storage', storage, 'complex', complex_value);

  ## Unlike the read window, every write extent MUST fit, without clipping.
  mathlab_edit_fields (request.window, {'row_start','column_start','rows','columns','higher_indices'}, {});
  row_start = mathlab_page_index (request.window.row_start, builtin('false'));
  column_start = mathlab_page_index (request.window.column_start, builtin('false'));
  rows = mathlab_page_count (request.window.rows);
  columns = mathlab_page_count (request.window.columns);
  if (rows > 2000 / columns)
    builtin('error', 'MathLab:ValueEditLimit', 'Typed range edits are limited to 2000 cells.');
  endif
  count = rows * columns;
  if (row_start > dimensions(1) || column_start > dimensions(2) || ...
      rows > dimensions(1) - row_start + 1 || columns > dimensions(2) - column_start + 1)
    builtin('error', 'MathLab:ValueEditSelection', 'The entire edit range must fit the existing leaf.');
  endif
  higher = mathlab_page_index_strings (request.window.higher_indices);
  if (builtin('numel', higher) != builtin('numel', dimensions) - 2)
    builtin('error', 'MathLab:ValueEditSelection', 'Every higher axis requires one explicit selector.');
  endif
  subscripts = builtin('cell', 1, builtin('numel', dimensions));
  subscripts{1} = row_start:row_start + rows - 1;
  subscripts{2} = column_start:column_start + columns - 1;
  for axis = 3:builtin('numel', dimensions)
    selection = mathlab_page_index (higher{axis - 2}, builtin('false'));
    if (selection > dimensions(axis))
      builtin('error', 'MathLab:ValueEditSelection', 'A higher-axis selector lies outside the leaf.');
    endif
    subscripts{axis} = selection;
  endfor
  window = builtin('struct', 'row_start', builtin('sprintf', '%.0f', row_start), ...
                   'column_start', builtin('sprintf', '%.0f', column_start), ...
                   'rows', rows, 'columns', columns, 'higher_indices', {higher});
  logical_leaf_bytes = mathlab_edit_leaf_admission (value, dimensions, type, sparse_value, complex_value, count);

  ## Parse the complete RHS before making any candidate assignment. IEEE hex is
  ## edit-only and avoids decimal->double->single midpoint double rounding.
  mathlab_edit_fields (request.data, {'kind','encoding','real','imag'}, {});
  if (! builtin('ischar', request.data.kind) || ! builtin('strcmp', request.data.kind, 'dense') || ...
      ! builtin('ischar', request.data.encoding))
    builtin('error', 'MathLab:ValueEditPayload', 'Typed edit data requires a dense encoded payload.');
  endif
  floating = builtin('any', builtin('strcmp', type, {'double','single'}));
  if ((floating && ! builtin('strcmp', request.data.encoding, 'ieee754_hex')) || ...
      (! floating && ! builtin('strcmp', request.data.encoding, 'decimal')))
    builtin('error', 'MathLab:ValueEditPayload', 'The payload encoding does not match its declared class.');
  endif
  real_tokens = mathlab_edit_tokens (request.data.real, count);
  if (complex_value); imaginary_count = count; else; imaginary_count = 0; endif
  imag_tokens = mathlab_edit_tokens (request.data.imag, imaginary_count);
  dense_complex = complex_value && ! sparse_value;
  if (dense_complex)
    if (! builtin('is_function_handle', value_bridge))
      builtin('error', 'MathLab:ValueEditPrecisionUnavailable', ...
              'Dense complex edits require the verified native component adapter.');
    endif
    ## Admit all four bounded REAL before/after buffers before native COW.
    if (count > 262144 / (2 * mathlab_edit_width(type, builtin('true'))))
      builtin('error', 'MathLab:ValueEditLimit', 'Native component pairs exceed their patch budget.');
    endif
    real_values = builtin('reshape', mathlab_edit_components(real_tokens, type, floating), rows, columns);
    imag_values = builtin('reshape', mathlab_edit_components(imag_tokens, type, floating), rows, columns);
    ## Do not construct a complex RHS, select a complex before-value, or call
    ## complex_policy here. Native component copies preserve genuine state/bits.
    [replacement, old_patch, new_patch] = value_bridge('replace_components', value, subscripts, real_values, imag_values);
    mathlab_value_pair_validate (old_patch, type, rows, columns, builtin('false'), 'MathLab:ValueEditInvariant');
    mathlab_value_pair_validate (new_patch, type, rows, columns, builtin('false'), 'MathLab:ValueEditInvariant');
    patch_bytes = mathlab_edit_patch_bytes (old_patch, type, complex_value, storage, rows, columns) + ...
                  mathlab_edit_patch_bytes (new_patch, type, complex_value, storage, rows, columns);
    if (patch_bytes > 262144)
      builtin('error', 'MathLab:ValueEditLimit', 'Native old/new patches exceed their 256 KiB accounting budget.');
    endif
  else
    real_values = mathlab_edit_components (real_tokens, type, floating);
    if (complex_value)
      imag_values = mathlab_edit_components (imag_tokens, type, floating);
      rhs = builtin('complex', real_values, imag_values);
    else
      rhs = real_values;
    endif
    rhs = builtin('reshape', rhs, rows, columns);
    new_patch = mathlab_edit_patch (rhs, type, sparse_value, complex_value, rows, columns);

    ## The existing read helper independently validates/extracts the bounded
    ## before-page. Never reconstruct native undo values from its decimal text.
    read_request = builtin('struct', 'path', builtin('struct', 'root', token, 'steps', {steps}), ...
                           'window', window);
    before_page = mathlab_value_page (read_request, value_bridge);
    if (! builtin('strcmp', before_page.class, type) || ...
        ! builtin('strcmp', before_page.source_storage, storage) || before_page.complex != complex_value || ...
        before_page.selection.rows != rows || before_page.selection.columns != columns)
      builtin('error', 'MathLab:ValueEditInvariant', 'The bounded before-page no longer matches the admitted edit.');
    endif
    old_patch = mathlab_edit_patch (value(subscripts{:}), type, sparse_value, complex_value, rows, columns);
    patch_bytes = mathlab_edit_patch_bytes (old_patch, type, complex_value, storage, rows, columns) + ...
                  mathlab_edit_patch_bytes (new_patch, type, complex_value, storage, rows, columns);
    if (patch_bytes > 262144)
      builtin('error', 'MathLab:ValueEditLimit', 'Native old/new patches exceed their 256 KiB accounting budget.');
    endif

    ## Candidate mutation is confined to admitted primitive data and ancestors.
    ## Sparse primitive assignment replaces implicit zero positions as well as
    ## nonzeros; no full-source full()/dense conversion is requested anywhere.
    replacement = value;
    replacement(subscripts{:}) = new_patch;
    replacement = mathlab_edit_complex_policy (replacement, complex_value);
  endif
  mathlab_edit_signature (replacement, type, sparse_value, complex_value, dimensions);
  mathlab_edit_leaf_admission (replacement, dimensions, type, sparse_value, complex_value, 0);
  for index = depth:-1:1
    parent = parents{index};
    selection = parent_indices{index};
    step = steps{index};
    if (builtin('strcmp', step.kind, 'cell'))
      parent{selection{:}} = replacement;
    else
      element = parent(selection{:});
      element.(step.name) = replacement;
      parent(selection{:}) = element;
    endif
    if (! mathlab_edit_same_dimensions (parent, parent_shapes{index}))
      builtin('error', 'MathLab:ValueEditInvariant', 'Ancestor reconstruction changed its dimensions.');
    endif
    replacement = parent;
  endfor

  patch = builtin('struct', 'schema_version', 1, 'token', token, 'path', {steps}, ...
                  'window', window, 'expected_leaf', expected, ...
                  'old_value', old_patch, 'new_value', new_patch, ...
                  'native_patch_bytes', patch_bytes, 'logical_leaf_bytes', logical_leaf_bytes, ...
                  'ancestor_reference_units', ancestor_references);
  replacement_root = replacement;
endfunction

function mathlab_edit_fields (value, required, optional)
  if (! builtin('isstruct', value) || builtin('numel', value) != 1 || ...
      builtin('numfields', value) > builtin('numel', required) + builtin('numel', optional))
    builtin('error', 'MathLab:ValueEditSchema', 'Expected a bounded schema object.');
  endif
  for index = 1:builtin('numel', required)
    if (! builtin('isfield', value, required{index}))
      builtin('error', 'MathLab:ValueEditSchema', 'A required edit field is missing.');
    endif
  endfor
  fields = builtin('__fieldnames__', value);
  allowed = [required optional];
  for index = 1:builtin('numel', fields)
    if (! builtin('any', builtin('strcmp', fields{index}, allowed)))
      builtin('error', 'MathLab:ValueEditSchema', 'An unknown edit field is unsupported.');
    endif
  endfor
endfunction

function count = mathlab_edit_product (dimensions, maximum)
  count = 1;
  if (builtin('any', dimensions == 0)); count = 0; return; endif
  for axis = 1:builtin('numel', dimensions)
    if (dimensions(axis) > maximum / count)
      builtin('error', 'MathLab:ValueEditLimit', 'Edit construction exceeds its admitted size.');
    endif
    count *= dimensions(axis);
  endfor
endfunction

function width = mathlab_edit_width (type, complex_value)
  if (builtin('any', builtin('strcmp', type, {'logical','int8','uint8'})))
    width = 1;
  elseif (builtin('any', builtin('strcmp', type, {'int16','uint16'})))
    width = 2;
  elseif (builtin('any', builtin('strcmp', type, {'single','int32','uint32'})))
    width = 4;
  else
    width = 8;
  endif
  if (complex_value); width *= 2; endif
endfunction

function bytes = mathlab_edit_leaf_admission (value, dimensions, type, sparse_value, complex_value, extra)
  maximum_bytes = 134217728;
  width = mathlab_edit_width (type, complex_value);
  if (sparse_value)
    capacity = builtin('nzmax', value);
    maximum_slots = 1048576;
    maximum_columns = 1048575;
    if (dimensions(2) > maximum_columns || capacity > maximum_slots - extra)
      builtin('error', 'MathLab:ValueEditLimit', 'Sparse edit construction exceeds capacity/column admission.');
    endif
    ## Eight-byte index/reference units are an explicit conservative accounting
    ## convention for this proposal, not a report of native allocator bytes.
    bytes = (capacity + extra) * (width + 8) + (dimensions(2) + 1) * 8;
    if (bytes > maximum_bytes)
      builtin('error', 'MathLab:ValueEditLimit', 'Sparse edit construction exceeds its logical byte budget.');
    endif
  else
    elements = mathlab_edit_product (dimensions, builtin('floor', maximum_bytes / width));
    bytes = elements * width;
  endif
endfunction

function tokens = mathlab_edit_tokens (encoded, count)
  if (builtin('iscell', encoded))
    if (builtin('numel', encoded) != count)
      builtin('error', 'MathLab:ValueEditPayload', 'Typed payload component counts do not match the entire range.');
    endif
    tokens = encoded;
  elseif (count == 0 && builtin('isnumeric', encoded) && builtin('isempty', encoded))
    tokens = {};
  else
    builtin('error', 'MathLab:ValueEditPayload', 'Expected an array of typed component strings.');
  endif
  for index = 1:count
    token = tokens{index};
    if (! builtin('ischar', token) || builtin('isempty', token) || ...
        builtin('size', token, 1) != 1 || builtin('numel', token) > 32 || ...
        builtin('any', builtin('uint8', token) > 127))
      builtin('error', 'MathLab:ValueEditPayload', 'Typed component tokens require 1 to 32 ASCII bytes.');
    endif
  endfor
endfunction

function values = mathlab_edit_components (tokens, type, floating)
  count = builtin('numel', tokens);
  if (builtin('strcmp', type, 'logical'))
    values = builtin('false', count, 1);
  else
    values = builtin('zeros', count, 1, type);
  endif
  for index = 1:count
    if (floating)
      values(index) = mathlab_edit_float_bits (tokens{index}, type);
    else
      values(index) = mathlab_edit_integer (tokens{index}, type);
    endif
  endfor
endfunction

function value = mathlab_edit_float_bits (text, type)
  if (builtin('strcmp', type, 'single')); digits = 8; else; digits = 16; endif
  if (builtin('numel', text) != digits || ...
      builtin('any', ! ((text >= '0' & text <= '9') | (text >= 'a' & text <= 'f'))))
    builtin('error', 'MathLab:ValueEditPayload', 'Floating edits require exact lowercase IEEE-754 hexadecimal bits.');
  endif
  bits = builtin('uint64', 0);
  for index = 1:digits
    digit = builtin('double', text(index)) - builtin('double', '0');
    if (text(index) >= 'a')
      digit = builtin('double', text(index)) - builtin('double', 'a') + 10;
    endif
    ## At most sixteen hex digits; each prefix fits before the next four-bit
    ## shift. Bit operations do not route uint64 through floating point.
    bits = builtin('bitor', builtin('bitshift', bits, 4), builtin('uint64', digit));
  endfor
  if (digits == 8); bits = builtin('uint32', bits); endif
  value = builtin('typecast', bits, type);
endfunction

function value = mathlab_edit_integer (text, type)
  if (builtin('strcmp', type, 'logical'))
    if (builtin('strcmp', text, '0')); value = builtin('false'); return; endif
    if (builtin('strcmp', text, '1')); value = builtin('true'); return; endif
    builtin('error', 'MathLab:ValueEditPayload', 'Logical edit components must be exactly 0 or 1.');
  endif
  unsigned = builtin('any', builtin('strcmp', type, {'uint8','uint16','uint32','uint64'}));
  negative = text(1) == '-';
  if (negative)
    if (unsigned || builtin('numel', text) == 1)
      builtin('error', 'MathLab:ValueEditPayload', 'Expected a canonical integer of the declared class.');
    endif
    digits = text(2:end);
  else
    digits = text;
  endif
  if (builtin('any', digits < '0' | digits > '9') || ...
      (builtin('numel', digits) > 1 && digits(1) == '0') || ...
      (negative && builtin('strcmp', digits, '0')))
    builtin('error', 'MathLab:ValueEditPayload', 'Expected a canonical integer of the declared class.');
  endif
  switch (type)
    case 'uint8'; maximum = '255';
    case 'uint16'; maximum = '65535';
    case 'uint32'; maximum = '4294967295';
    case 'uint64'; maximum = '18446744073709551615';
    case 'int8'; if (negative); maximum = '128'; else; maximum = '127'; endif
    case 'int16'; if (negative); maximum = '32768'; else; maximum = '32767'; endif
    case 'int32'; if (negative); maximum = '2147483648'; else; maximum = '2147483647'; endif
    case 'int64'; if (negative); maximum = '9223372036854775808'; else; maximum = '9223372036854775807'; endif
    otherwise
      builtin('error', 'MathLab:ValueEditUnsupported', 'Unsupported integer edit class.');
  endswitch
  if (mathlab_edit_decimal_greater (digits, maximum))
    builtin('error', 'MathLab:ValueEditPayload', 'An integer edit component overflows its declared class.');
  endif
  magnitude = builtin('uint64', 0);
  for index = 1:builtin('numel', digits)
    digit = builtin('uint64', builtin('double', digits(index)) - builtin('double', '0'));
    magnitude = magnitude * builtin('uint64', 10) + digit;
  endfor
  if (negative)
    ## magnitude-1 fits int64 even for the signed minimum's 2^63 magnitude.
    signed_value = -builtin('int64', magnitude - builtin('uint64', 1)) - builtin('int64', 1);
    value = builtin(type, signed_value);
  else
    value = builtin(type, magnitude);
  endif
endfunction

function greater = mathlab_edit_decimal_greater (digits, maximum)
  if (builtin('numel', digits) != builtin('numel', maximum))
    greater = builtin('numel', digits) > builtin('numel', maximum);
    return;
  endif
  greater = builtin('false');
  for index = 1:builtin('numel', digits)
    if (digits(index) < maximum(index)); return; endif
    if (digits(index) > maximum(index)); greater = builtin('true'); return; endif
  endfor
endfunction

function value = mathlab_edit_complex_policy (value, complex_value)
  if (complex_value && builtin('isreal', value))
    ## The selected backend must qualify that genuine complex(sparse) remains
    ## sparse. A signature failure rejects the private candidate, never base.
    value = builtin('complex', value);
  endif
endfunction

function mathlab_edit_signature (value, type, sparse_value, complex_value, dimensions)
  if (! builtin('strcmp', builtin('class', value), type) || ...
      builtin('issparse', value) != sparse_value || (! builtin('isreal', value)) != complex_value || ...
      ! mathlab_edit_same_dimensions (value, dimensions))
    builtin('error', 'MathLab:ValueEditInvariant', 'Edit construction changed class, shape, storage or complex metadata.');
  endif
endfunction

function same = mathlab_edit_same_dimensions (value, dimensions)
  actual = builtin('size', value);
  same = builtin('numel', actual) == builtin('numel', dimensions) && ...
         ! builtin('any', actual != dimensions);
endfunction

function patch = mathlab_edit_patch (value, type, sparse_value, complex_value, rows, columns)
  if (sparse_value)
    ## Reconstruct a compact bounded patch: a full-window sparse index can share
    ## the original's oversized nzmax buffer even when it has <=2000 cells.
    [r, c, components] = builtin('find', value);
    if (builtin('numel', components) > rows * columns)
      builtin('error', 'MathLab:ValueEditInvariant', 'Sparse patch has too many selected components.');
    endif
    patch = builtin('sparse', r, c, components, rows, columns);
  else
    ## A contiguous index/reshape can keep the complete source ArrayRep alive.
    ## That view is only a temporary input here: bounded exact text is rebuilt
    ## into fresh <=2000-element buffers by scalar assignments. Do not replace
    ## this with a full-range/vector assignment, which can share the old backing.
    selected = builtin('reshape', value, rows, columns);
    selected = mathlab_edit_complex_policy (selected, complex_value);
    encoded = mathlab_edit_patch_data (selected, type, complex_value);
    floating = builtin('any', builtin('strcmp', type, {'double','single'}));
    real_values = mathlab_edit_components (encoded.real, type, floating);
    if (complex_value)
      imag_values = mathlab_edit_components (encoded.imag, type, floating);
      patch = builtin('complex', real_values, imag_values);
    else
      patch = real_values;
    endif
    patch = builtin('reshape', patch, rows, columns);
  endif
  patch = mathlab_edit_complex_policy (patch, complex_value);
  mathlab_edit_signature (patch, type, sparse_value, complex_value, [rows columns]);
  if (sparse_value && builtin('nzmax', patch) > builtin('max', 1, rows * columns))
    builtin('error', 'MathLab:ValueEditLimit', 'Sparse selected patch exceeds its compact capacity budget.');
  endif
endfunction

function bytes = mathlab_edit_patch_bytes (value, type, complex_value, storage, rows, columns)
  width = mathlab_edit_width (type, complex_value);
  if (complex_value && builtin('strcmp', storage, 'dense'))
    count = mathlab_value_pair_validate (value, type, rows, columns, builtin('false'), 'MathLab:ValueEditInvariant');
    bytes = count * width; ## two genuine REAL buffers, not struct numel==1
    return;
  endif
  if (builtin('issparse', value))
    bytes = builtin('nzmax', value) * (width + 8) + (builtin('size', value, 2) + 1) * 8;
  else
    bytes = builtin('numel', value) * width;
  endif
endfunction

## Internal-only encoder for bounded native ledger patches. The owner creates
## path/window/expected_leaf/current-token separately, then calls the normal
## preparation path for undo/redo. This function never accesses a base binding.
function data = mathlab_edit_patch_data (patch, type, complex_value, storage = '', rows = [], columns = [])
  if (! builtin('islogical', complex_value) || builtin('numel', complex_value) != 1)
    builtin('error', 'MathLab:ValueEditPayload', 'Expected scalar logical complex metadata.');
  endif
  if (complex_value && builtin('ischar', storage) && builtin('strcmp', storage, 'dense'))
    data = mathlab_edit_pair_data (patch, type, rows, columns);
    return;
  endif
  ## Dense complex history must never fall back to a selected complex array.
  ## Sparse complex history retains its existing compact numerical route.
  if (complex_value && (! builtin('ischar', storage) || ! builtin('strcmp', storage, 'sparse') || ! builtin('issparse', patch)))
    builtin('error', 'MathLab:ValueEditInvariant', 'Dense complex history requires native REAL component pairs.');
  endif
  classes = {'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64','logical'};
  if (! builtin('ischar', type) || ! builtin('any', builtin('strcmp', type, classes)) || ...
      ! builtin('strcmp', builtin('class', patch), type) || ...
      ! (builtin('isnumeric', patch) || builtin('islogical', patch)) || ...
      ! builtin('islogical', complex_value) || builtin('numel', complex_value) != 1)
    builtin('error', 'MathLab:ValueEditPayload', 'Expected a genuine typed native history patch.');
  endif
  if (builtin('ndims', patch) != 2 || builtin('numel', patch) < 1 || builtin('numel', patch) > 2000 || ...
      (! builtin('isreal', patch)) != complex_value || ...
      (complex_value && ! builtin('any', builtin('strcmp', type, {'double','single'}))))
    builtin('error', 'MathLab:ValueEditLimit', 'Native history patch shape/complex metadata is unsupported.');
  endif
  count = builtin('numel', patch);
  floating = builtin('any', builtin('strcmp', type, {'double','single'}));
  if (floating)
    encoding = 'ieee754_hex';
    if (builtin('strcmp', type, 'single')); zero = '00000000'; else; zero = '0000000000000000'; endif
  else
    encoding = 'decimal';
    zero = '0';
  endif
  real_tokens = builtin('cell', 1, count);
  if (complex_value); imag_tokens = builtin('cell', 1, count); else; imag_tokens = {}; endif
  if (builtin('issparse', patch))
    if (! builtin('any', builtin('strcmp', type, {'double','logical'})) || ...
        builtin('nzmax', patch) > count)
      builtin('error', 'MathLab:ValueEditLimit', 'Native sparse history patch exceeds compact capacity/class limits.');
    endif
    ## Encode the <=2000-cell logical page, leaving implicit zeros explicit in
    ## the dense edit wire. Do not densify an arbitrary source/root value.
    for index = 1:count
      real_tokens{index} = zero;
      if (complex_value); imag_tokens{index} = zero; endif
    endfor
    [rows, columns, values] = builtin('find', patch);
    real_values = builtin('real', values);
    if (complex_value); imag_values = builtin('imag', values); endif
    for entry = 1:builtin('numel', values)
      index = rows(entry) + (columns(entry) - 1) * builtin('size', patch, 1);
      real_tokens{index} = mathlab_edit_component_text (real_values(entry), type, floating);
      if (complex_value)
        imag_tokens{index} = mathlab_edit_component_text (imag_values(entry), type, floating);
      endif
    endfor
  else
    ## Extract components before scalar indexing: Octave can narrow a complex
    ## scalar with -0 imaginary part to a real scalar, losing that sign bit.
    real_values = builtin('real', patch);
    if (complex_value); imag_values = builtin('imag', patch); endif
    for index = 1:count
      real_tokens{index} = mathlab_edit_component_text (real_values(index), type, floating);
      if (complex_value)
        imag_tokens{index} = mathlab_edit_component_text (imag_values(index), type, floating);
      endif
    endfor
  endif
  data = builtin('struct', 'kind', 'dense', 'encoding', encoding, ...
                 'real', {real_tokens}, 'imag', {imag_tokens});
endfunction

function text = mathlab_edit_component_text (value, type, floating)
  if (floating)
    if (builtin('strcmp', type, 'single'))
      bits = builtin('typecast', value, 'uint32');
      text = builtin('sprintf', '%08x', bits);
      digits = 8;
    else
      bits = builtin('typecast', value, 'uint64');
      text = builtin('sprintf', '%016x', bits);
      digits = 16;
    endif
    if (builtin('numel', text) != digits || ...
        builtin('any', ! ((text >= '0' & text <= '9') | (text >= 'a' & text <= 'f'))))
      builtin('error', 'MathLab:ValueEditInvariant', 'Native bit formatting did not produce canonical hex.');
    endif
  elseif (builtin('strcmp', type, 'logical'))
    if (value); text = '1'; else; text = '0'; endif
  elseif (builtin('any', builtin('strcmp', type, {'uint8','uint16','uint32','uint64'})))
    text = builtin('sprintf', '%u', value);
  else
    text = builtin('sprintf', '%d', value);
  endif
endfunction

## SPDX-License-Identifier: GPL-3.0-or-later
## Private history preparation. The ordinary owner
## prepares a candidate and receipt BEFORE revocation/root assignment, then
## installs the candidate only after the one assignment. A lost acknowledgment
## is uncertain; no command here authorizes retries or performs base assignment.
function state = mathlab_edit_history_new (invocation = 1)
  ## Give each same-process recovery its own disjoint epoch range. A local
  ## counter must never roll into the next invocation's range.
  if (! builtin('isnumeric', invocation) || ! builtin('isreal', invocation) || ...
      builtin('numel', invocation) != 1 || invocation < 1 || invocation > 4294967295 || ...
      builtin('fix', invocation) != invocation)
    builtin('error', 'MathLab:ValueEditHistoryExhausted', 'History invocation identity exhausted; restart the session.');
  endif
  epoch = builtin('bitshift', builtin('uint64', invocation), 32) + builtin('uint64', 1);
  state = builtin('struct', 'epoch', epoch, ...
                  'next_id', builtin('uint64', 1), 'context', [], ...
                  'undo', {{}}, 'redo', {{}}, 'retained_patch_bytes', 0);
endfunction

function state = mathlab_edit_history_invalidate (state)
  if (builtin('bitand', state.epoch, builtin('uint64', 4294967295)) == builtin('uint64', 4294967295))
    builtin('error', 'MathLab:ValueEditHistoryExhausted', 'History epoch identity exhausted; restart the session.');
  endif
  epoch = mathlab_next_revision (state.epoch);
  next_id = state.next_id;
  state = mathlab_edit_history_new ();
  state.epoch = epoch;
  state.next_id = next_id;
endfunction

function [candidate, receipt] = mathlab_edit_history_accept_write (state, patch, token)
  before = mathlab_page_token (patch.token);
  token = mathlab_edit_history_next_token (before, token);
  mathlab_edit_history_context (state, before.context);
  ## Reserve the next identity and complete bounded record construction before
  ## any root replacement. Exhaustion fails without changing state or values.
  next_id = mathlab_next_revision (state.next_id);
  record = builtin('struct', 'id', state.next_id, 'name', before.name, ...
                   'path', {patch.path}, 'window', patch.window, ...
                   'expected_leaf', patch.expected_leaf, ...
                   'old_value', patch.old_value, 'new_value', patch.new_value);
  wire_record = builtin('struct', 'id', builtin('sprintf', '%u', record.id), ...
                        'name', record.name, 'path', {record.path}, ...
                        'window', record.window, 'expected_leaf', record.expected_leaf, ...
                        'old_data', mathlab_edit_patch_data(record.old_value, record.expected_leaf.class, record.expected_leaf.complex, record.expected_leaf.source_storage, record.window.rows, record.window.columns), ...
                        'new_data', mathlab_edit_patch_data(record.new_value, record.expected_leaf.class, record.expected_leaf.complex, record.expected_leaf.source_storage, record.window.rows, record.window.columns));
  encoded = builtin('__unicode2native__', builtin('jsonencode', wire_record), 'UTF-8');
  charge = builtin('numel', encoded);
  if (charge > 512 * 1024)
    builtin('error', 'MathLab:ValueEditHistoryLimit', 'One undo record exceeds the 512 KiB encoded-record charge.');
  endif
  record.retained_patch_bytes = charge;
  candidate = state;
  candidate.redo = {};
  candidate.retained_patch_bytes = mathlab_edit_history_charge (candidate.undo);
  evicted = 0;
  while (builtin('numel', candidate.undo) >= 32 || candidate.retained_patch_bytes + charge > 2 * 1024 * 1024)
    if (builtin('isempty', candidate.undo))
      builtin('error', 'MathLab:ValueEditHistoryLimit', 'The transaction cannot fit the combined history quota.');
    endif
    candidate.retained_patch_bytes -= candidate.undo{1}.retained_patch_bytes;
    candidate.undo(1) = [];
    evicted += 1;
  endwhile
  candidate.undo{end + 1} = record;
  candidate.retained_patch_bytes += charge;
  candidate.next_id = next_id;
  candidate.context = token.context;
  receipt = mathlab_edit_history_receipt (candidate, 'write', record.id, token, evicted);
endfunction

function edit = mathlab_edit_history_restore_request (state, request, operation, context)
  record = mathlab_edit_history_requested_record (state, request, operation, context);
  if (builtin('strcmp', operation, 'undo'))
    value = record.old_value;
  else
    value = record.new_value;
  endif
  path = builtin('struct', 'root', mathlab_page_token(request.expected), 'steps', {record.path});
  edit = builtin('struct', 'schema_version', 1, 'path', path, ...
                 'window', record.window, 'expected_leaf', record.expected_leaf, ...
                 'data', mathlab_edit_patch_data(value, record.expected_leaf.class, record.expected_leaf.complex, record.expected_leaf.source_storage, record.window.rows, record.window.columns));
endfunction

function [candidate, receipt] = mathlab_edit_history_accept_restore (state, request, operation, token)
  before = mathlab_page_token (request.expected);
  record = mathlab_edit_history_requested_record (state, request, operation, before.context);
  token = mathlab_edit_history_next_token (before, token);
  candidate = state;
  if (builtin('strcmp', operation, 'undo'))
    candidate.undo(end) = [];
    candidate.redo{end + 1} = record;
  else
    candidate.redo(end) = [];
    candidate.undo{end + 1} = record;
  endif
  candidate.context = token.context;
  ## Movement retains the same bounded record, not another copy of its payload.
  receipt = mathlab_edit_history_receipt (candidate, operation, record.id, token, 0);
endfunction

function record = mathlab_edit_history_requested_record (state, request, operation, context)
  mathlab_edit_fields (request, {'schema_version','expected','history_epoch','transaction_id'}, {'command','id'});
  if (! builtin('isnumeric', request.schema_version) || ! builtin('isreal', request.schema_version) || ...
      builtin('numel', request.schema_version) != 1 || request.schema_version != 1 || ...
      ! builtin('ischar', operation) || ! builtin('any', builtin('strcmp', operation, {'undo','redo'})))
    builtin('error', 'MathLab:ValueEditSchema', 'Expected a version-1 undo or redo request.');
  endif
  if (builtin('isfield', request, 'command') && ...
      (! builtin('ischar', request.command) || ! builtin('strcmp', request.command, [operation '_value_edit_checked'])))
    builtin('error', 'MathLab:ValueEditSchema', 'Unexpected history command.');
  endif
  mathlab_edit_fields (request.expected, {'context','name'}, {});
  mathlab_edit_fields (request.expected.context, {'worker_id','generation','invocation','revision'}, {});
  token = mathlab_page_token (request.expected);
  if (! mathlab_same_context (token.context, context))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'The history request belongs to a different binding context.');
  endif
  mathlab_edit_history_context (state, context);
  if (! builtin('ischar', request.history_epoch) || ...
      ! builtin('strcmp', request.history_epoch, builtin('sprintf', '%u', state.epoch)))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'The undo history epoch changed.');
  endif
  if (builtin('strcmp', operation, 'undo')); stack = state.undo; else; stack = state.redo; endif
  if (builtin('isempty', stack))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'No accepted transaction is available for this operation.');
  endif
  record = stack{end};
  if (! builtin('ischar', request.transaction_id) || ...
      ! builtin('strcmp', request.transaction_id, builtin('sprintf', '%u', record.id)) || ...
      ! builtin('strcmp', token.name, record.name))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'The expected transaction or root is not the history head.');
  endif
endfunction

function mathlab_edit_history_context (state, context)
  if (! builtin('isempty', state.context) && ! mathlab_same_context (state.context, context))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'An intervening operation invalidated this history.');
  endif
endfunction

function token = mathlab_edit_history_next_token (before, token)
  token = mathlab_page_token (token);
  expected_revision = mathlab_next_revision (mathlab_edit_integer(before.context.revision, 'uint64'));
  expected = before;
  expected.context.revision = builtin('sprintf', '%u', expected_revision);
  if (! builtin('strcmp', token.name, expected.name) || ! mathlab_same_context (token.context, expected.context))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'An accepted transaction must advance the exact binding revision once.');
  endif
endfunction

function charge = mathlab_edit_history_charge (records)
  charge = 0;
  if (builtin('numel', records) > 32)
    builtin('error', 'MathLab:ValueEditHistoryLimit', 'History exceeds the transaction quota.');
  endif
  for index = 1:builtin('numel', records)
    charge += records{index}.retained_patch_bytes;
  endfor
  if (charge > 2 * 1024 * 1024)
    builtin('error', 'MathLab:ValueEditHistoryLimit', 'History exceeds the combined encoded-record charge.');
  endif
endfunction

function head = mathlab_edit_history_head (records)
  if (builtin('isempty', records))
    ## Octave jsonencode represents NaN as JSON null (ConvertInfAndNaN=true).
    ## [] would be an array and is not an optional HistoryHead on the wire.
    head = builtin('nan');
  else
    record = records{end};
    head = builtin('struct', 'transaction_id', builtin('sprintf', '%u', record.id), 'root_name', record.name);
  endif
endfunction

function receipt = mathlab_edit_history_receipt (state, operation, id, token, evicted)
  count = builtin('numel', state.undo) + builtin('numel', state.redo);
  charge = mathlab_edit_history_charge (state.undo) + mathlab_edit_history_charge (state.redo);
  if (count < 1 || count > 32 || charge < 1 || charge > 2 * 1024 * 1024 || charge != state.retained_patch_bytes)
    builtin('error', 'MathLab:ValueEditHistoryLimit', 'History count or charge is inconsistent.');
  endif
  history = builtin('struct', 'epoch', builtin('sprintf', '%u', state.epoch), ...
                    'undo', mathlab_edit_history_head(state.undo), ...
                    'redo', mathlab_edit_history_head(state.redo));
  receipt = builtin('struct', 'schema_version', 1, 'operation', operation, ...
                    'transaction_id', builtin('sprintf', '%u', id), 'token', token, ...
                    'history', history, 'retained_patch_bytes', charge, ...
                    'retained_transactions', count, 'evicted_undo_entries', evicted);
  encoded = builtin('__unicode2native__', builtin('jsonencode', receipt), 'UTF-8');
  if (builtin('numel', encoded) > 16 * 1024)
    builtin('error', 'MathLab:ValueEditHistoryLimit', 'An edit receipt exceeds 16 KiB.');
  endif
endfunction

## Dense complex history stays as two bounded native REAL arrays. Only this
## encoder converts it to existing canonical IEEE hex request/wire data.
function data = mathlab_edit_pair_data (pair, type, rows, columns)
  count = mathlab_value_pair_validate (pair, type, rows, columns, builtin('false'), 'MathLab:ValueEditInvariant');
  if (builtin('strcmp', type, 'single'))
    integer_type = 'uint32'; format = '%08x'; digits = 8;
  else
    integer_type = 'uint64'; format = '%016x'; digits = 16;
  endif
  real_bits = builtin('typecast', pair.real(:), integer_type);
  imag_bits = builtin('typecast', pair.imag(:), integer_type);
  real_tokens = builtin('cell', 1, count);
  imag_tokens = builtin('cell', 1, count);
  for index = 1:count
    real_text = builtin('sprintf', format, real_bits(index));
    imag_text = builtin('sprintf', format, imag_bits(index));
    if (builtin('numel', real_text) != digits || builtin('numel', imag_text) != digits || ...
        builtin('any', ! ((real_text >= '0' & real_text <= '9') | (real_text >= 'a' & real_text <= 'f'))) || ...
        builtin('any', ! ((imag_text >= '0' & imag_text <= '9') | (imag_text >= 'a' & imag_text <= 'f'))))
      builtin('error', 'MathLab:ValueEditInvariant', 'Native component-pair encoding must be canonical fixed-width hex.');
    endif
    real_tokens{index} = real_text;
    imag_tokens{index} = imag_text;
  endfor
  data = builtin('struct', 'kind', 'dense', 'encoding', 'ieee754_hex', ...
                 'real', {real_tokens}, 'imag', {imag_tokens});
endfunction
