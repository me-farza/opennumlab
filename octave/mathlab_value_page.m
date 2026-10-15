## SPDX-License-Identifier: GPL-3.0-or-later
## Private helper text appended to the owned worker source by Engine startup.
## It is not installed as a separately shadowable helper on the load path.
## The dispatch owner must check request.path.root against its current binding
## context immediately before calling this helper in the same ordinary operation.
function result = mathlab_value_page (request)
  token = mathlab_page_token (request.path.root);
  value = mathlab_named_value (token.name);
  steps = mathlab_page_steps (request.path.steps);
  for step_index = 1:builtin('numel', steps)
    step = steps{step_index};
    if (! builtin('isstruct', step) || builtin('numel', step) != 1 || ...
        ! builtin('isfield', step, 'kind') || ! builtin('ischar', step.kind))
      builtin('error', 'MathLab:ValuePagePath', 'Expected a typed container path segment.');
    endif
    if (builtin('strcmp', step.kind, 'cell'))
      if (! builtin('iscell', value) || ! builtin('strcmp', builtin('class', value), 'cell'))
        builtin('error', 'MathLab:ValuePageUnsupported', 'Cell navigation requires a primitive cell container.');
      endif
      indices = mathlab_page_container_indices (step, value);
      value = value{indices{:}};
      steps{step_index} = builtin('struct', 'kind', 'cell', ...
                                  'indices', {mathlab_page_index_strings(step.indices)});
    elseif (builtin('strcmp', step.kind, 'field'))
      if (! builtin('isstruct', value) || ! builtin('strcmp', builtin('class', value), 'struct'))
        builtin('error', 'MathLab:ValuePageUnsupported', 'Field navigation requires a primitive struct container.');
      endif
      if (! builtin('isfield', step, 'name') || ! mathlab_page_identifier (step.name))
        builtin('error', 'MathLab:ValuePagePath', 'Expected an ASCII struct field identifier.');
      endif
      ## Scalar struct selection can copy one reference per field. Bound that
      ## intermediate independently of the final numeric page cell count.
      if (builtin('numfields', value) > 256)
        builtin('error', 'MathLab:ValuePageUnsupported', 'Typed field navigation is limited to 256 fields per struct.');
      endif
      indices = mathlab_page_container_indices (step, value);
      element = value(indices{:});
      if (! builtin('isfield', element, step.name))
        builtin('error', 'MathLab:ValuePagePath', 'The requested struct field does not exist.');
      endif
      value = element.(step.name);
      steps{step_index} = builtin('struct', 'kind', 'field', ...
                                  'indices', {mathlab_page_index_strings(step.indices)}, 'name', step.name);
    else
      builtin('error', 'MathLab:ValuePageUnsupported', 'This container path segment is unsupported.');
    endif
  endfor

  numeric_classes = {'double', 'single', 'int8', 'uint8', 'int16', 'uint16', ...
                     'int32', 'uint32', 'int64', 'uint64', 'logical'};
  type = builtin('class', value);
  if (! builtin('any', builtin('strcmp', type, numeric_classes)) || ...
      ! (builtin('isnumeric', value) || builtin('islogical', value)))
    ## Reject before size, indexing, conversion or display can enter an object.
    builtin('error', 'MathLab:ValuePageUnsupported', 'Typed pages currently support primitive numeric/logical values only.');
  endif
  dimensions = mathlab_page_dimensions (value);
  higher = mathlab_page_index_strings (request.window.higher_indices);
  if (builtin('numel', higher) != builtin('numel', dimensions) - 2)
    builtin('error', 'MathLab:ValuePageSelection', 'Provide one explicit index for every higher dimension.');
  endif
  row_start = mathlab_page_index (request.window.row_start, false);
  column_start = mathlab_page_index (request.window.column_start, false);
  requested_rows = mathlab_page_count (request.window.rows);
  requested_columns = mathlab_page_count (request.window.columns);
  if (requested_rows > 2000 / requested_columns)
    builtin('error', 'MathLab:ValuePageLimit', 'Typed pages are limited to 2000 requested cells.');
  endif
  if (row_start > builtin('max', 1, dimensions(1)) || ...
      column_start > builtin('max', 1, dimensions(2)))
    builtin('error', 'MathLab:ValuePageSelection', 'The page starts outside the value dimensions.');
  endif
  subscripts = builtin('cell', 1, builtin('numel', dimensions));
  for axis = 3:builtin('numel', dimensions)
    if (dimensions(axis) == 0)
      builtin('error', 'MathLab:ValuePageUnsupported', 'A zero-length higher axis has no selectable plane in typed-page version 1.');
    endif
    index = mathlab_page_index (higher{axis - 2}, false);
    if (index > dimensions(axis))
      builtin('error', 'MathLab:ValuePageSelection', 'A higher-axis index is outside the value dimensions.');
    endif
    subscripts{axis} = index;
  endfor
  rows = builtin('min', requested_rows, builtin('max', 0, dimensions(1) - row_start + 1));
  columns = builtin('min', requested_columns, builtin('max', 0, dimensions(2) - column_start + 1));
  subscripts{1} = row_start:row_start + rows - 1;
  subscripts{2} = column_start:column_start + columns - 1;
  sparse_value = builtin('issparse', value);
  complex_value = ! builtin('isreal', value);
  if (sparse_value && ! builtin('any', builtin('strcmp', type, {'double', 'logical'})))
    builtin('error', 'MathLab:ValuePageUnsupported', 'Sparse typed pages currently support double and logical values only.');
  endif
  if (complex_value && ! builtin('any', builtin('strcmp', type, {'double', 'single'})))
    builtin('error', 'MathLab:ValuePageUnsupported', 'Complex typed pages require single or double values.');
  endif

  ## Index only after proving primitive type and every bound. No full-source
  ## flattening or sparse densification is performed. A selected sparse page has
  ## at most 2000 logical cells, and find() operates on that page only.
  page = value(subscripts{:});
  if (sparse_value)
    [local_rows, local_columns, components] = builtin('find', page);
    encoded_rows = builtin('cell', 1, builtin('numel', local_rows));
    encoded_columns = builtin('cell', 1, builtin('numel', local_columns));
    for index = 1:builtin('numel', local_rows)
      encoded_rows{index} = local_rows(index);
      encoded_columns{index} = local_columns(index);
    endfor
    storage = 'sparse';
    data = builtin('struct', 'kind', 'sparse_coo', 'rows', {encoded_rows}, ...
                   'columns', {encoded_columns});
  else
    components = page(:);
    storage = 'dense';
    data = builtin('struct', 'kind', 'dense');
  endif
  data.real = mathlab_page_components (builtin('real', components), type);
  if (complex_value)
    data.imag = mathlab_page_components (builtin('imag', components), type);
  else
    data.imag = {};
  endif
  shape = builtin('cell', 1, builtin('numel', dimensions));
  for axis = 1:builtin('numel', dimensions)
    shape{axis} = builtin('sprintf', '%.0f', dimensions(axis));
  endfor
  selection = builtin('struct', 'row_start', request.window.row_start, ...
                        'column_start', request.window.column_start, ...
                        'rows', rows, 'columns', columns, 'higher_indices', {higher});
  result = builtin('struct', 'schema_version', 1, 'token', token, ...
                   'path', {steps}, 'class', type, 'source_shape', {shape}, ...
                   'source_storage', storage, 'complex', complex_value, ...
                   'selection', selection, 'data', data);
  ## The pre-materialization cell/rank/path/token budgets bound construction;
  ## this independent final cap also catches a future schema expansion.
  encoded = builtin('__unicode2native__', builtin('jsonencode', result), 'UTF-8');
  if (builtin('numel', encoded) > 1024 * 1024)
    builtin('error', 'MathLab:ValuePageLimit', 'Typed page payload exceeds 1 MiB.');
  endif
endfunction

function token = mathlab_page_token (root)
  ## Reconstruct only bounded schema fields. Never echo unchecked extra metadata
  ## from a token/step into a response before checking its serialized size.
  if (! builtin('isstruct', root) || builtin('numel', root) != 1 || ...
      ! builtin('isfield', root, 'name') || ! mathlab_page_identifier (root.name) || ...
      ! builtin('isfield', root, 'context') || ! builtin('isstruct', root.context) || ...
      builtin('numel', root.context) != 1)
    builtin('error', 'MathLab:ValuePageIdentity', 'Expected a bounded binding token.');
  endif
  context = root.context;
  if (! builtin('isfield', context, 'worker_id') || ! builtin('ischar', context.worker_id) || ...
      builtin('isempty', context.worker_id) || builtin('numel', context.worker_id) > 256)
    builtin('error', 'MathLab:ValuePageIdentity', 'The worker identity exceeds its token budget.');
  endif
  if (builtin('numel', builtin('__unicode2native__', context.worker_id, 'UTF-8')) > 256)
    builtin('error', 'MathLab:ValuePageIdentity', 'The worker identity exceeds 256 UTF-8 bytes.');
  endif
  names = {'generation', 'invocation', 'revision'};
  maximum = '18446744073709551615';
  for index = 1:3
    name = names{index};
    if (! builtin('isfield', context, name) || ! builtin('ischar', context.(name)) || ...
        builtin('numel', context.(name)) > 20 || ...
        builtin('isempty', builtin('regexp', context.(name), '^(0|[1-9][0-9]*)$', 'once')))
      builtin('error', 'MathLab:ValuePageIdentity', 'Expected a canonical unsigned 64-bit identity counter.');
    endif
    text = context.(name);
    if (builtin('any', text(:) < '0' | text(:) > '9'))
      builtin('error', 'MathLab:ValuePageIdentity', 'Identity counters contain decimal digits only.');
    endif
    if (builtin('numel', text) == 20)
      for digit = 1:20
        if (text(digit) < maximum(digit)); break; endif
        if (text(digit) > maximum(digit))
          builtin('error', 'MathLab:ValuePageIdentity', 'Identity counter overflows unsigned 64-bit.');
        endif
      endfor
    endif
  endfor
  bounded_context = builtin('struct', 'worker_id', context.worker_id, ...
                             'generation', context.generation, 'invocation', context.invocation, ...
                             'revision', context.revision);
  token = builtin('struct', 'context', bounded_context, 'name', root.name);
endfunction

function steps = mathlab_page_steps (encoded)
  if (builtin('numel', encoded) > 16)
    builtin('error', 'MathLab:ValuePageLimit', 'Container paths are limited to 16 segments.');
  endif
  if (builtin('isempty', encoded))
    steps = {};
  elseif (builtin('iscell', encoded))
    steps = encoded;
  elseif (builtin('isstruct', encoded))
    steps = builtin('cell', 1, builtin('numel', encoded));
    for index = 1:builtin('numel', encoded)
      steps{index} = encoded(index);
    endfor
  else
    builtin('error', 'MathLab:ValuePagePath', 'Expected a typed container path.');
  endif
endfunction

function indices = mathlab_page_container_indices (step, value)
  if (! builtin('isfield', step, 'indices'))
    builtin('error', 'MathLab:ValuePagePath', 'Container navigation requires explicit indices.');
  endif
  dimensions = mathlab_page_dimensions (value);
  encoded = mathlab_page_index_strings (step.indices);
  if (builtin('numel', encoded) != builtin('numel', dimensions))
    builtin('error', 'MathLab:ValuePagePath', 'Container navigation requires one index per dimension.');
  endif
  indices = builtin('cell', 1, builtin('numel', encoded));
  for axis = 1:builtin('numel', encoded)
    index = mathlab_page_index (encoded{axis}, false);
    if (index > dimensions(axis))
      builtin('error', 'MathLab:ValuePagePath', 'A container index is outside its dimensions.');
    endif
    indices{axis} = index;
  endfor
endfunction

function dimensions = mathlab_page_dimensions (value)
  rank = builtin('ndims', value);
  if (rank < 2 || rank > 32)
    builtin('error', 'MathLab:ValuePageUnsupported', 'Typed-page rank must be between 2 and 32.');
  endif
  dimensions = builtin('size', value);
  ## size() exposes doubles. Refuse the threshold itself as well as larger
  ## extents: a backend dimension of 2^53+1 could already round down to 2^53.
  if (builtin('any', dimensions < 0) || builtin('any', dimensions >= builtin('flintmax')) || ...
      builtin('any', ! builtin('isfinite', dimensions)) || ...
      builtin('any', builtin('fix', dimensions) != dimensions))
    builtin('error', 'MathLab:ValuePageUnsupported', 'A source dimension is not exactly representable by this backend.');
  endif
endfunction

function encoded = mathlab_page_index_strings (value)
  if (builtin('isempty', value))
    encoded = {};
  elseif (builtin('iscell', value) && builtin('numel', value) <= 32)
    encoded = value;
  else
    builtin('error', 'MathLab:ValuePageSelection', 'Expected at most 32 decimal-string indices.');
  endif
endfunction

function index = mathlab_page_index (text, allow_zero)
  if (! builtin('ischar', text) || builtin('numel', text) > 20 || ...
      builtin('isempty', builtin('regexp', text, '^(0|[1-9][0-9]*)$', 'once')))
    builtin('error', 'MathLab:ValuePageSelection', 'Expected a canonical unsigned decimal index.');
  endif
  index = builtin('str2double', text);
  if (! builtin('isfinite', index) || index >= builtin('flintmax') || ...
      (! allow_zero && index == 0) || ...
      ! builtin('strcmp', builtin('sprintf', '%.0f', index), text))
    builtin('error', 'MathLab:ValuePageSelection', 'The index is not exactly supported by this backend.');
  endif
endfunction

function count = mathlab_page_count (value)
  if (! builtin('isnumeric', value) || builtin('numel', value) != 1 || ...
      ! builtin('isreal', value) || ! builtin('isfinite', value) || ...
      value < 1 || value > 2000 || builtin('fix', value) != value)
    builtin('error', 'MathLab:ValuePageLimit', 'Page dimensions must be integers between 1 and 2000.');
  endif
  count = builtin('double', value);
endfunction

function valid = mathlab_page_identifier (name)
  valid = false;
  if (! builtin('ischar', name) || builtin('isempty', name) || ...
      builtin('numel', name) > 63 || builtin('size', name, 1) != 1)
    return;
  endif
  letters = (name >= 'A' & name <= 'Z') | (name >= 'a' & name <= 'z') | name == '_';
  digits = name >= '0' & name <= '9';
  valid = letters(1) && ! builtin('any', ! (letters | digits));
endfunction

function strings = mathlab_page_components (values, type)
  if (builtin('numel', values) > 2000)
    builtin('error', 'MathLab:ValuePageLimit', 'Too many typed components.');
  endif
  strings = builtin('cell', 1, builtin('numel', values));
  for index = 1:builtin('numel', values)
    component = values(index);
    if (builtin('strcmp', type, 'logical'))
      if (component); text = '1'; else; text = '0'; endif
    elseif (builtin('any', builtin('strcmp', type, {'uint8', 'uint16', 'uint32', 'uint64'})))
      text = builtin('sprintf', '%u', component);
    elseif (builtin('any', builtin('strcmp', type, {'int8', 'int16', 'int32', 'int64'})))
      text = builtin('sprintf', '%d', component);
    elseif (builtin('isnan', component))
      text = 'NaN';
    elseif (builtin('isinf', component))
      if (component < 0); text = '-Inf'; else; text = 'Inf'; endif
    elseif (builtin('strcmp', type, 'single'))
      text = builtin('sprintf', '%.9g', component);
    else
      ## The C-format conversion retains -0; qualification includes sign checks.
      text = builtin('sprintf', '%.17g', component);
    endif
    if (builtin('numel', text) > 32)
      builtin('error', 'MathLab:ValuePageLimit', 'A typed component exceeds its token budget.');
    endif
    strings{index} = text;
  endfor
endfunction
