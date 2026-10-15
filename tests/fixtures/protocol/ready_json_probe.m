## SPDX-License-Identifier: GPL-3.0-or-later
## Run with octave/ and this directory on the path. No native module required.
## This probe must execute on the actual Octave 8.4 and 11.3 target runtimes.
function ready_json_probe ()
  escaped = ['quote" slash\ line' builtin('char', 10) 'tab' builtin('char', 9) '雪'];
  protocol = builtin('struct', 'name', 'mathlab-worker', 'version', builtin('uint32', 1), ...
    'capabilities', {{'binding_snapshot.v1','checked_scalar_write.v1','display_page.v1','future_dynamic.v2',escaped}}, ...
    'limits', builtin('struct', 'variable_cells', builtin('uint32', 2000), ...
                             'response_bytes', builtin('uint32', 33554432), 'limit_note', escaped), ...
    'producer_note', escaped);
  result = builtin('struct', 'version', 'ready-json-probe', 'working_directory', escaped, ...
    'protocol', protocol, 'context', builtin('struct', 'worker_id', escaped, ...
      'generation', '1', 'invocation', '2', 'revision', '9007199254740993'), ...
    'extra', builtin('struct', 'note', escaped, 'values', {{(1 == 1),[],3.5}}));
  response = builtin('struct', 'id', 0, 'ok', (1 == 1), 'result', result, 'extra_envelope', escaped);
  ## Field enumeration and escaping must preserve extensions at every owned
  ## object level, including field names that are not source identifiers.
  limits_key = ['limit"' builtin('char', 92) builtin('char', 10) '雪'];
  protocol_key = ['protocol"' builtin('char', 92) builtin('char', 9) '雪'];
  result_key = ['result"' builtin('char', 92) builtin('char', 10) '雪'];
  envelope_key = ['envelope"' builtin('char', 92) builtin('char', 9) '雪'];
  response.result.protocol.limits.(limits_key) = escaped;
  response.result.protocol.(protocol_key) = escaped;
  response.result.(result_key) = escaped;
  response.(envelope_key) = escaped;
  wire = mathlab_ready_json (response, (1 == 1));
  for field = {limits_key,protocol_key,result_key,envelope_key}
    ready_json_probe_token (wire, [builtin('jsonencode', field{1}) ':' builtin('jsonencode', escaped)]);
  endfor
  ready_json_probe_token (wire, '"version":1,');
  ready_json_probe_token (wire, '"variable_cells":2000,');
  ready_json_probe_token (wire, '"response_bytes":33554432,');
  decoded = builtin('jsondecode', wire);
  assert(builtin('strcmp', decoded.result.protocol.name, protocol.name));
  assert(isequal(decoded.result.protocol.capabilities(:), protocol.capabilities(:)));
  assert(builtin('strcmp', decoded.result.protocol.producer_note, escaped));
  assert(builtin('strcmp', decoded.result.protocol.limits.limit_note, escaped));
  assert(builtin('strcmp', decoded.result.working_directory, escaped));
  assert(builtin('strcmp', decoded.result.context.worker_id, escaped));
  assert(builtin('strcmp', decoded.result.context.revision, '9007199254740993'));
  assert(builtin('strcmp', decoded.result.extra.note, escaped));
  assert(builtin('strcmp', decoded.extra_envelope, escaped));
  assert(decoded.result.extra.values{1} && builtin('isempty', decoded.result.extra.values{2}) && decoded.result.extra.values{3} == 3.5);
  builtin('fprintf', 'READY_JSON_BASE %s\n', wire);

  ## Different VALID wire integers must be emitted unchanged. The strict Rust
  ## contract must reject these values rather than this producer normalizing.
  alternate = response;
  alternate.result.protocol.version = 2;
  alternate.result.protocol.limits.variable_cells = builtin('uint16', 1999);
  alternate.result.protocol.limits.response_bytes = 33554431;
  different = mathlab_ready_json (alternate, (1 == 1));
  ready_json_probe_token (different, '"version":2,');
  ready_json_probe_token (different, '"variable_cells":1999,');
  ready_json_probe_token (different, '"response_bytes":33554431,');
  builtin('fprintf', 'READY_JSON_DIFFERENT %s\n', different);

  ## Exact native extrema and a value just beyond consecutive binary64 integers.
  large = response;
  large.result.protocol.version = builtin('intmax', 'uint32');
  large.result.protocol.limits.variable_cells = builtin('bitor', builtin('bitshift', builtin('uint64', 1), 53), builtin('uint64', 1));
  large.result.protocol.limits.response_bytes = builtin('intmax', 'uint64');
  extrema = mathlab_ready_json (large, (1 == 1));
  ready_json_probe_token (extrema, '"version":4294967295,');
  ready_json_probe_token (extrema, '"variable_cells":9007199254740993,');
  ready_json_probe_token (extrema, '"response_bytes":18446744073709551615,');
  builtin('fprintf', 'READY_JSON_EXTREMA %s\n', extrema);
  zero = response;
  zero.result.protocol.version = builtin('uint8', 0);
  zero.result.protocol.limits.variable_cells = builtin('int64', 0);
  zero.result.protocol.limits.response_bytes = builtin('single', 0);
  encoded_zero = mathlab_ready_json (zero, (1 == 1));
  ready_json_probe_token (encoded_zero, '"version":0,');
  ready_json_probe_token (encoded_zero, '"variable_cells":0,');
  ready_json_probe_token (encoded_zero, '"response_bytes":0,');

  negative_zero = builtin('typecast', builtin('bitshift', builtin('uint64', 1), 63), 'double');
  malformed = {1.5, builtin('single', 1.5), -1, builtin('int64', -1), negative_zero, ...
               NaN, Inf, -Inf, (1 == 1), (1 == 0), '1', [], [1 2], {1}, builtin('struct'), ...
               builtin('sparse', 1), builtin('complex', 1, 1), 18446744073709551616};
  for field = {'version','variable_cells','response_bytes'}
    name = field{1};
    for index = 1:builtin('numel', malformed)
      rejected = response;
      if (builtin('strcmp', name, 'version'))
        rejected.result.protocol.version = malformed{index};
      else
        rejected.result.protocol.limits.(name) = malformed{index};
      endif
      ready_json_probe_reject (rejected, 'MathLab:ReadyJsonInteger');
    endfor
  endfor
  for overflow = {4294967296, builtin('uint64', 4294967296), builtin('intmax', 'uint64')}
    rejected = response;
    rejected.result.protocol.version = overflow{1};
    ready_json_probe_reject (rejected, 'MathLab:ReadyJsonInteger');
  endfor
  missing = response;
  missing.result.protocol.limits = builtin('rmfield', missing.result.protocol.limits, 'response_bytes');
  ready_json_probe_reject (missing, 'MathLab:ReadyJsonShape');
  malformed_protocol = response;
  malformed_protocol.result.protocol = {'not','a','scalar struct'};
  ready_json_probe_reject (malformed_protocol, 'MathLab:ReadyJsonShape');

  ## No shape inference is permitted. Ordinary data can contain every Ready
  ## discriminator, even a malformed protocol, and must pass through unchanged.
  ordinary_ready = response;
  ordinary_ready.result.protocol.version = 1;
  ordinary_ready.result.protocol.limits.variable_cells = 2000;
  ordinary_ready.result.protocol.limits.response_bytes = 33554432;
  ordinary_bad_protocol = ordinary_ready;
  ordinary_bad_protocol.result.protocol = 'fully Ready-shaped ordinary protocol data';
  ordinary_bad_limits = ordinary_ready;
  ordinary_bad_limits.result.protocol.limits = {'ordinary','limits'};
  ordinary_bad_integer = ordinary_ready;
  ordinary_bad_integer.result.protocol.version = 'ordinary version';
  ordinary = {builtin('struct', 'id', 7, 'ok', (1 == 1), 'result', builtin('struct', 'output', escaped)), ...
    builtin('struct', 'id', 8, 'ok', (1 == 0), 'error', builtin('struct', 'message', escaped)), ...
    escaped, [], builtin('struct', 'protocol', 'an ordinary top-level field'), ...
    builtin('struct', 'id', 7, 'ok', (1 == 1), 'result', builtin('struct', 'protocol', 'user data')), ...
    builtin('struct', 'id', 7, 'ok', (1 == 1), 'result', builtin('struct', 'protocol', builtin('struct', 'version', 1))), ...
    builtin('struct', 'id', 7, 'ok', (1 == 0), 'result', builtin('struct', 'protocol', 'ordinary failure data')), ...
    ordinary_ready, ordinary_bad_protocol, ordinary_bad_limits, ordinary_bad_integer};
  for index = 1:builtin('numel', ordinary)
    ready_json_probe_passthrough (ordinary{index});
  endfor
  for field = {'version','working_directory','context','protocol'}
    candidate = ordinary_bad_protocol;
    candidate.result = builtin('rmfield', candidate.result, field{1});
    ready_json_probe_passthrough (candidate);
    ready_json_probe_reject (candidate, 'MathLab:ReadyJsonShape');
  endfor
  for field = {'id','ok'}
    candidate = ordinary_bad_protocol;
    candidate = builtin('rmfield', candidate, field{1});
    ready_json_probe_passthrough (candidate);
    ready_json_probe_reject (candidate, 'MathLab:ReadyJsonShape');
  endfor
  candidate = ordinary_bad_protocol;
  candidate.ok = (1 == 0);
  ready_json_probe_passthrough (candidate);
  ready_json_probe_reject (candidate, 'MathLab:ReadyJsonShape');
  candidate = ordinary_bad_protocol;
  candidate.result.context = {'ordinary','context'};
  ready_json_probe_passthrough (candidate);
  ready_json_probe_reject (candidate, 'MathLab:ReadyJsonShape');
  ready_json_probe_reject (ordinary_bad_protocol, 'MathLab:ReadyJsonShape');
  ready_json_probe_reject (ordinary_bad_limits, 'MathLab:ReadyJsonShape');
  ready_json_probe_reject (ordinary_bad_integer, 'MathLab:ReadyJsonInteger');

  ## The mode cannot be a numeric truthy value, container, array or sparse value.
  invalid_modes = {0, 1, -1, NaN, Inf, [], [0 1], 'true', {(1 == 1)}, ...
                   builtin('struct'), [(1 == 1) (1 == 0)], builtin('sparse', (1 == 1))};
  for index = 1:builtin('numel', invalid_modes)
    ready_json_probe_reject (ordinary_bad_protocol, 'MathLab:ReadyJsonMode', invalid_modes{index});
  endfor
  builtin('disp', 'PASS ready_json_probe explicit owner, canonical integers, escaping, dynamic fields, exact extrema, rejection and ordinary passthrough');
endfunction

function ready_json_probe_token (wire, expected)
  assert(! builtin('isempty', builtin('strfind', wire, expected)), ['Missing exact JSON token: ' expected]);
endfunction

function ready_json_probe_reject (response, identifier, is_ready = (1 == 1))
  caught_id = '';
  try
    mathlab_ready_json (response, is_ready);
  catch caught
    caught_id = caught.identifier;
  end_try_catch
  assert(builtin('strcmp', caught_id, identifier), ['Expected rejection: ' identifier ', received: ' caught_id]);
endfunction

function ready_json_probe_passthrough (value)
  expected = builtin('jsonencode', value);
  assert(builtin('strcmp', mathlab_ready_json(value), expected));
  assert(builtin('strcmp', mathlab_ready_json(value, (1 == 0)), expected));
endfunction
