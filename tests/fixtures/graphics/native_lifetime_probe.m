## SPDX-License-Identifier: GPL-3.0-or-later
## Original headless native-reference qualification. This is not an Engine or
## native mouse/UI test. Run only after the serial build/runtime handoff.
bridge_path = getenv ('MATHLAB_GRAPHICS_BRIDGE');
assert (! isempty (bridge_path), 'MATHLAB_GRAPHICS_BRIDGE is required.');
autoload ('mathlab_graphics_bridge', canonicalize_file_name(bridge_path));
bridge = @mathlab_graphics_bridge;
protocol = bridge ('protocol');
assert (strcmp(protocol.protocol, 'mathlab-graphics') && protocol.version == 1);
assert (strcmp(protocol.octave_version, OCTAVE_VERSION));
assert (protocol.native_object_lifetime && protocol.max_entries == 4096);
disp ('PASS native_graphics_protocol_runtime_identity');

## Explicitly invisible gnuplot objects avoid creating toolkit GUI windows.
assert (any(strcmp(available_graphics_toolkits(), 'gnuplot')), ...
        'This headless qualification requires the gnuplot toolkit.');
native_figure = figure ('visible', 'off', 'integerhandle', 'on', '__graphics_toolkit__', 'gnuplot');
unwind_protect
  native_axes = axes ('parent', native_figure);
  native_line = line ('parent', native_axes, 'xdata', [0 1], ...
                      'ydata', [0 1], 'linewidth', 1.5);
  bridge ('open', 'original-probe-epoch-one');
  figure_key = bridge ('capture', 'original-probe-epoch-one', native_figure);
  axes_key = bridge ('capture', 'original-probe-epoch-one', native_axes);
  line_key = bridge ('capture', 'original-probe-epoch-one', native_line);
  assert (ischar(figure_key) && ischar(axes_key) && ischar(line_key));
  assert (! strcmp(figure_key, axes_key) && ! strcmp(axes_key, line_key));
  assert (bridge('check', 'original-probe-epoch-one', figure_key, native_figure));
  assert (bridge('check', 'original-probe-epoch-one', axes_key, native_axes));
  assert (bridge('check', 'original-probe-epoch-one', line_key, native_line));
  set (native_line, 'linewidth', 2.5);
  assert (get(native_line, 'linewidth') == 2.5);
  assert (bridge('check', 'original-probe-epoch-one', line_key, native_line));
  disp ('PASS native_graphics_script_mutation_preserves_reference');

  ## Defeat every replaceable beingdeleted listener and overwrite DeleteFcn.
  ## The exact same integer figure handle will name a different native object.
  old_figure_handle = native_figure;
  old_axes_handle = native_axes;
  old_line_handle = native_line;
  number = get (native_figure, 'number');
  for object = [native_line native_axes native_figure]
    dellistener (object, 'beingdeleted', 'persistent');
    dellistener (object, 'beingdeleted');
    set (object, 'deletefcn', @(source,event) 0);
  endfor
  delete (native_figure);
  native_figure = figure (number, 'visible', 'off', '__graphics_toolkit__', 'gnuplot');
  native_axes = axes ('parent', native_figure);
  native_line = line ('parent', native_axes, 'xdata', [0 1], ...
                      'ydata', [0 1], 'linewidth', 1.5);
  assert (native_figure == old_figure_handle);
  assert (! bridge('check', 'original-probe-epoch-one', figure_key, native_figure));
  assert (! bridge('check', 'original-probe-epoch-one', axes_key, native_axes));
  assert (! bridge('check', 'original-probe-epoch-one', line_key, native_line));
  assert (! bridge('check', 'original-probe-epoch-one', axes_key, old_axes_handle));
  assert (! bridge('check', 'original-probe-epoch-one', line_key, old_line_handle));
  assert (get(native_line, 'linewidth') == 1.5);
  disp ('PASS native_graphics_recycled_integer_handle_rejects_without_listeners');

  replacement_key = bridge ('capture', 'original-probe-epoch-one', native_line);
  assert (bridge('check', 'original-probe-epoch-one', replacement_key, native_line));
  bridge ('open', 'original-probe-epoch-two');
  assert (! bridge('check', 'original-probe-epoch-one', replacement_key, native_line));
  assert (! bridge('check', 'original-probe-epoch-two', replacement_key, native_line));
  fresh_key = bridge ('capture', 'original-probe-epoch-two', native_line);
  assert (! strcmp(replacement_key, fresh_key));
  assert (bridge('check', 'original-probe-epoch-two', fresh_key, native_line));
  disp ('PASS native_graphics_epoch_retirement_preserves_live_object');

  for invalid = {'not a numeric handle', [native_line native_line], ...
                 complex(native_line,1), NaN, Inf, single(native_line)}
    rejected = false;
    try
      bridge ('capture', 'original-probe-epoch-two', invalid{1});
    catch caught
      rejected = strcmp(caught.identifier, 'MathLab:GraphicsBridgeArguments');
    end_try_catch
    assert (rejected);
  endfor
  assert (get(native_line, 'linewidth') == 1.5);
  disp ('PASS native_graphics_primitive_gate_rejects_without_effects');

  ## One reference already occupies this epoch. All retained references count,
  ## including captures of the same object; the cap is not a live-object count.
  for capture_index = 2:4096
    bridge ('capture', 'original-probe-epoch-two', native_line);
  endfor
  rejected = false;
  try
    bridge ('capture', 'original-probe-epoch-two', native_line);
  catch caught
    rejected = strcmp(caught.identifier, 'MathLab:GraphicsLimit');
  end_try_catch
  assert (rejected);
  assert (bridge('check', 'original-probe-epoch-two', fresh_key, native_line));
  bridge ('open', 'original-probe-epoch-three');
  assert (! bridge('check', 'original-probe-epoch-three', fresh_key, native_line));
  assert (get(native_line, 'linewidth') == 1.5);
  disp ('PASS native_graphics_retention_budget_and_release');
unwind_protect_cleanup
  if (ishghandle(native_figure)); delete(native_figure); endif
  bridge ('open', 'original-probe-cleanup');
end_unwind_protect
disp ('PASS native_graphics_probe_complete');
