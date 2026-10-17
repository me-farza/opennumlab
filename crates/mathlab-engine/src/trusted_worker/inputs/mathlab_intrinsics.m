## SPDX-License-Identifier: GPL-3.0-or-later
## Private V2 source: append once to the exact owned worker closure.
## Not an on-path helper, not a numerical function registry.
function [dispatcher, bridge, adapters] = mathlab_intrinsics (mode = 0, bindings = [])
  persistent retained_dispatcher = [];
  persistent retained_bridge = [];
  persistent retained_adapters = [];
  if (mode == 1)
    ## The ONLY seeding caller is the worker's first line, invoked through the
    ## exact worker function object retained by the startup native provider.
    ## No @builtin fallback and no shadowable helper is used to initialize.
    dispatcher = bindings.dispatcher;
    bridge = bindings.bridge;
    ## Fmlock must see this helper as its caller, without a builtin frame.
    caller_lock = bindings.caller_lock;
    caller_lock ();
    retained_dispatcher = dispatcher;
    retained_bridge = bridge;
    retained_adapters = bindings.adapters;
  endif
  dispatcher = retained_dispatcher;
  bridge = retained_bridge;
  adapters = retained_adapters;
endfunction

## Carrier values are produced by the private native provider, never requests.
## These checks establish consistency; native acquisition establishes identity.
function [callback, module_path] = mathlab_native_adapter (role, expected_path)
  [builtin, bridge, adapters] = mathlab_intrinsics ();
  if (! builtin('ischar', role) || builtin('rows', role) != 1)
    builtin('error', 'MathLab:IntrinsicAdapter', 'Expected a fixed adapter role.');
  endif
  switch (role)
    case 'input'
      protocol = 'mathlab-input'; version = '1';
    case 'scope'
      protocol = 'mathlab-scope'; version = '2';
    case 'value'
      protocol = 'mathlab-value'; version = '1';
    case 'debug'
      protocol = 'mathlab-debugger'; version = '4';
    case 'graphics'
      protocol = 'mathlab-graphics'; version = '1';
    otherwise
      builtin('error', 'MathLab:IntrinsicAdapter', 'Unknown private adapter role.');
  endswitch
  if (builtin('isobject', adapters) || ! builtin('isstruct', adapters) ...
      || builtin('numel', adapters) != 1 || ! builtin('isfield', adapters, role))
    builtin('error', 'MathLab:IntrinsicAdapterMissing', 'Adapter was not admitted before project entry.');
  endif
  selected = adapters.(role);
  if (builtin('isobject', selected) || ! builtin('isstruct', selected) ...
      || builtin('numel', selected) != 1 ...
      || builtin('numel', builtin('fieldnames', selected)) != 4 ...
      || ! builtin('isfield', selected, 'callback') ...
      || ! builtin('isfield', selected, 'path') ...
      || ! builtin('isfield', selected, 'protocol') ...
      || ! builtin('isfield', selected, 'version'))
    builtin('error', 'MathLab:IntrinsicAdapter', 'Invalid private adapter binding.');
  endif
  if (! builtin('isa', selected.callback, 'function_handle') ...
      || ! builtin('ischar', expected_path) || builtin('rows', expected_path) != 1 ...
      || builtin('isempty', expected_path) ...
      || ! builtin('ischar', selected.path) || builtin('rows', selected.path) != 1 ...
      || ! builtin('strcmp', selected.path, expected_path) ...
      || ! builtin('ischar', selected.protocol) || builtin('rows', selected.protocol) != 1 ...
      || ! builtin('strcmp', selected.protocol, protocol) ...
      || ! builtin('ischar', selected.version) || builtin('rows', selected.version) != 1 ...
      || ! builtin('strcmp', selected.version, version))
    builtin('error', 'MathLab:IntrinsicAdapter', 'Restart to change the admitted adapter binding.');
  endif
  ## Never stringify/re-resolve this handle, install a numerical registry here,
  ## or enable a feature simply because its code was admitted at startup.
  callback = selected.callback;
  module_path = selected.path;
endfunction

function names = mathlab_intrinsic_base_names ()
  [builtin, bridge] = mathlab_intrinsics ();
  names = bridge('intrinsic_base_names');
endfunction

function entry = mathlab_intrinsic_base_metadata (name)
  [builtin, bridge] = mathlab_intrinsics ();
  entry = bridge('intrinsic_base_metadata', name);
endfunction

function varargout = mathlab_scope_call (varargin)
  [builtin, bridge] = mathlab_intrinsics ();
  registry = bridge('intrinsic_bind_registry');
  ## Keep this temporary function object in THIS short-lived helper frame only.
  ## Never let the long-lived worker retain the numerical registry in ans/local.
  count = builtin('nargout');
  if (count == 0)
    builtin('feval', registry, varargin{:});
  else
    [varargout{1:count}] = builtin('feval', registry, varargin{:});
  endif
endfunction

function value = mathlab_intrinsic_base_value (name)
  [builtin, bridge] = mathlab_intrinsics ();
  value = bridge('intrinsic_base_value', name);
endfunction
