## SPDX-License-Identifier: GPL-3.0-or-later
function varargout = mathlab_scope_register (key, handles, configuration = [], original_source = [])
  persistent keys = {};
  persistent scopes = {};
  persistent bridge_enabled = false;
  persistent debugger_enabled = false;
  varargout = {};
  if (builtin ('nargin') == 3)
    if (builtin ('ischar', configuration) && builtin ('strcmp', configuration, 'lookup'))
      retained = {};
      index = builtin ('find', builtin ('strcmp', keys, key), 1);
      if (! builtin ('isempty', index)); retained = scopes{index}; endif
      if (builtin ('nargout') > 0); varargout = {retained}; endif
    elseif (builtin ('strcmp', key, '__debug_configuration__'))
      debugger_enabled = builtin ('logical', configuration);
    else
      bridge_enabled = builtin ('logical', configuration);
    endif
    return;
  endif
  index = builtin ('find', builtin ('strcmp', keys, key), 1);
  if (builtin ('isempty', index))
    keys{end + 1} = key;
    scopes{end + 1} = handles;
    index = builtin ('numel', keys);
  endif
  if (bridge_enabled || debugger_enabled)
    registry_bindings = __ml_registry_provider__ ('intrinsic_bindings');
  endif
  if (bridge_enabled)
    if (! builtin ('isfield', registry_bindings.adapters, 'scope'))
      builtin ('error', 'MathLab:IntrinsicUnavailable', 'The scope adapter is absent from the admitted native profile.');
    endif
    if (builtin ('nargin') == 4)
      mathlab_scope_bridge (key, scopes{index}, configuration, original_source);
    else
      mathlab_scope_bridge (key, scopes{index});
    endif
  endif
  if (debugger_enabled)
    if (! builtin ('isfield', registry_bindings.adapters, 'debug'))
      builtin ('error', 'MathLab:IntrinsicUnavailable', 'The debugger adapter is absent from the admitted native profile.');
    endif
    mathlab_debug_bridge ('refresh_source', key);
  endif
  if (builtin ('nargout') > 0); varargout = {scopes{index}}; endif
endfunction
