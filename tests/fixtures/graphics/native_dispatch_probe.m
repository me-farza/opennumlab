## SPDX-License-Identifier: GPL-3.0-or-later
## Headless proof that simple @builtin is not a retained intrinsic identity.
bridge_path = getenv ('MATHLAB_GRAPHICS_BRIDGE');
assert (! isempty(bridge_path));
autoload ('mathlab_graphics_bridge', canonicalize_file_name(bridge_path));
bindings = mathlab_graphics_bridge ('bindings');
trusted = bindings.dispatcher;
bridge = bindings.bridge;
simple = @builtin;
dispatcher_probe_value = [1 2 3];
directory = tempname ();
mkdir (directory);
stream = fopen ([directory '/builtin.m'], 'w');
fprintf (stream, ['function varargout=builtin(varargin)\n' ...
                 ' assignin(''base'',''dispatcher_shadow_called'',true);\n' ...
                 ' error(''OpenNumLab:DispatcherShadow'',''Project dispatcher invoked.'');\n' ...
                 'endfunction\n']);
fclose (stream);
stream = fopen ([directory '/mathlab_graphics_bridge.m'], 'w');
fprintf (stream, ['function varargout=mathlab_graphics_bridge(varargin)\n' ...
                 ' error(''OpenNumLab:BridgeShadow'',''Project bridge alias invoked.'');\n' ...
                 'endfunction\n']);
fclose (stream);
addpath (directory, '-begin');
unwind_protect
  bare_id = '';
  simple_id = '';
  try
    builtin ('disp', 'BARE_DISPATCH_MUST_NOT_SUCCEED');
  catch caught
    bare_id = caught.identifier;
  end_try_catch
  try
    simple ('disp', 'SIMPLE_CAPTURE_MUST_NOT_SUCCEED');
  catch caught
    simple_id = caught.identifier;
  end_try_catch
  if (! trusted('strcmp', bare_id, 'OpenNumLab:DispatcherShadow') || ...
      ! trusted('strcmp', simple_id, 'OpenNumLab:DispatcherShadow') || ! dispatcher_shadow_called)
    trusted('error', 'OpenNumLab:DispatcherProbe', 'The project-dispatch positive controls did not execute.');
  endif
  trusted ('disp', 'PASS native_dispatch_bare_and_simple_handle_positive_controls');
  if (trusted('class', dispatcher_probe_value) != 'double')
    trusted('error', 'OpenNumLab:DispatcherProbe', 'The bound intrinsic returned the wrong class.');
  endif
  trusted ('disp', 'PASS native_dispatch_bound_builtin_ignores_project_shadow');
  names = bridge ('base_names');
  if (! trusted('any', trusted('strcmp', names, 'dispatcher_probe_value')))
    trusted('error', 'OpenNumLab:DispatcherProbe', 'The native base query did not use the original base frame.');
  endif
  metadata = bridge ('base_metadata', 'dispatcher_probe_value');
  if (! trusted('strcmp', metadata.name, 'dispatcher_probe_value') || metadata.bytes != 24)
    trusted('error', 'OpenNumLab:DispatcherProbe', 'Native primitive byte metadata is incorrect.');
  endif
  trusted ('disp', 'PASS native_dispatch_native_base_queries_ignore_project_shadow');
unwind_protect_cleanup
  trusted ('rmpath', directory);
  rmdir (directory, 's');
end_unwind_protect
disp ('PASS native_dispatch_probe_complete');
