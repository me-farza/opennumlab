## SPDX-License-Identifier: GPL-3.0-or-later
## Private helper appended to the owned worker, not installed on its load path.
## Octave 8.4 can reject native integer values in jsonencode, or spell a double
## limit with a fractional suffix. Only readiness's three unsigned wire fields
## are encoded here after an explicit owner flag. Ordinary values, including
## user data shaped exactly like Ready, retain the builtin encoder's behavior.
function text = mathlab_ready_json (response, is_ready = (1 == 0))
  if (! builtin('islogical', is_ready) || builtin('isobject', is_ready) || ...
      ! builtin('strcmp', builtin('class', is_ready), 'logical') || ...
      builtin('issparse', is_ready) || builtin('numel', is_ready) != 1)
    builtin('error', 'MathLab:ReadyJsonMode', 'The readiness owner flag must be a genuine dense logical scalar.');
  endif
  if (! is_ready)
    text = builtin('jsonencode', response);
    return;
  endif
  if (! mathlab_ready_json_envelope(response))
    builtin('error', 'MathLab:ReadyJsonShape', 'Explicit readiness requires a valid scalar success envelope and readiness metadata.');
  endif
  protocol = response.result.protocol;
  if (! mathlab_ready_json_struct(protocol) || ! builtin('isfield', protocol, 'version') || ...
      ! builtin('isfield', protocol, 'limits') || ! mathlab_ready_json_struct(protocol.limits) || ...
      ! builtin('isfield', protocol.limits, 'variable_cells') || ...
      ! builtin('isfield', protocol.limits, 'response_bytes'))
    builtin('error', 'MathLab:ReadyJsonShape', 'Readiness requires scalar protocol/limits structs and all three integer fields.');
  endif

  version = mathlab_ready_json_unsigned (protocol.version, (1 == 1));
  cells = mathlab_ready_json_unsigned (protocol.limits.variable_cells, (1 == 0));
  bytes = mathlab_ready_json_unsigned (protocol.limits.response_bytes, (1 == 0));
  limits = mathlab_ready_json_object (protocol.limits, {'variable_cells','response_bytes'}, {cells,bytes});
  protocol_text = mathlab_ready_json_object (protocol, {'version','limits'}, {version,limits});
  result = mathlab_ready_json_object (response.result, {'protocol'}, {protocol_text});
  text = mathlab_ready_json_object (response, {'result'}, {result});
endfunction

## This validates an explicitly owner-tagged Ready; it never classifies an
## ordinary payload. Numeric protocol fields are checked separately below.
function valid = mathlab_ready_json_envelope (response)
  valid = (1 == 0);
  if (! mathlab_ready_json_struct(response) || ! builtin('isfield', response, 'id') || ...
      ! builtin('isfield', response, 'ok') || ! builtin('isfield', response, 'result') || ...
      ! builtin('islogical', response.ok) || builtin('isobject', response.ok) || ...
      builtin('issparse', response.ok) || builtin('numel', response.ok) != 1 || ! response.ok || ...
      ! builtin('isnumeric', response.id) || builtin('isobject', response.id) || ...
      ! builtin('isreal', response.id) || builtin('issparse', response.id) || builtin('numel', response.id) != 1 || ...
      ! mathlab_ready_json_struct(response.result))
    return;
  endif
  result = response.result;
  valid = builtin('isfield', result, 'version') && builtin('ischar', result.version) && ...
          builtin('isfield', result, 'working_directory') && builtin('ischar', result.working_directory) && ...
          builtin('isfield', result, 'context') && mathlab_ready_json_struct(result.context) && ...
          builtin('isfield', result, 'protocol');
endfunction

function valid = mathlab_ready_json_struct (value)
  valid = builtin('isstruct', value) && builtin('strcmp', builtin('class', value), 'struct') && ...
          ! builtin('isobject', value) && builtin('numel', value) == 1;
endfunction

## This is only scalar-object assembly at the four owned readiness levels.
## Field names, capabilities, context, extra fields and arbitrary string values
## are escaped/encoded by the genuine builtin. No JSON text is searched/replaced.
function text = mathlab_ready_json_object (value, names, encoded)
  fields = builtin('__fieldnames__', value);
  text = '{';
  for index = 1:builtin('numel', fields)
    name = fields{index};
    replacement = 0;
    for candidate = 1:builtin('numel', names)
      if (builtin('strcmp', name, names{candidate})); replacement = candidate; break; endif
    endfor
    if (replacement)
      item = encoded{replacement};
    else
      item = builtin('jsonencode', value.(name));
    endif
    if (index > 1); text = [text ',']; endif
    text = [text builtin('jsonencode', name) ':' item];
  endfor
  text = [text '}'];
endfunction

function text = mathlab_ready_json_unsigned (value, version)
  type = builtin('class', value);
  classes = {'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64'};
  if (! builtin('any', builtin('strcmp', type, classes)) || builtin('isobject', value) || ...
      ! builtin('isnumeric', value) || ! builtin('isreal', value) || builtin('issparse', value) || ...
      builtin('numel', value) != 1)
    builtin('error', 'MathLab:ReadyJsonInteger', 'Readiness integers require genuine dense real primitive numeric scalars.');
  endif
  if (builtin('strcmp', type, 'double') || builtin('strcmp', type, 'single'))
    ## Check before conversion: integer casts must never repair fractions,
    ## saturate overflow, or turn negative/nonfinite input into accepted data.
    ## 2^64 is exactly representable; every admitted integral floating value
    ## below it converts to uint64 without changing its represented value.
    if (! builtin('isfinite', value) || value < 0 || builtin('fix', value) != value || ...
        value >= 18446744073709551616 || (value == 0 && builtin('signbit', value)))
      builtin('error', 'MathLab:ReadyJsonInteger', 'Readiness integers must be finite unsigned integral values below 2^64, without negative zero.');
    endif
  elseif (value < 0)
    builtin('error', 'MathLab:ReadyJsonInteger', 'Readiness integers cannot be negative.');
  endif
  bits = builtin('uint64', value);
  if (version && bits > builtin('uint64', 4294967295))
    builtin('error', 'MathLab:ReadyJsonInteger', 'The protocol version must fit uint32.');
  endif
  ## Native unsigned formatting avoids a double intermediate, including values
  ## above 2^53. The probe requires exact uint64-max output on each target ABI.
  text = builtin('sprintf', '%u', bits);
  if (builtin('isempty', text) || builtin('any', text < '0' | text > '9') || ...
      (builtin('numel', text) > 1 && text(1) == '0'))
    builtin('error', 'MathLab:ReadyJsonInteger', 'Native unsigned formatting did not produce a canonical integer token.');
  endif
endfunction
