## SPDX-License-Identifier: GPL-3.0-or-later
## An ordinary named user callback, exercised by the programmatic service route.
## This source does not simulate a native toolkit mouse event.
function numjinn_graphics_callback (source, event)
  builtin('fprintf', builtin('stdout'), 'OPENNUMLAB_GRAPHICS_ENTRY:%s:%s\n', ...
          event.sequence, event.workspace_revision);
  builtin('fflush', builtin('stdout'));
  if (! builtin('strcmp', event.stimulus, 'programmatic'))
    builtin('error', 'NumJinn:GraphicsStimulus', 'Expected a programmatic stimulus.');
  endif
  if (event.schema_version != 1)
    builtin('error', 'NumJinn:GraphicsEventSchema', 'Unexpected event payload schema.');
  endif
  mode = builtin('evalin', 'base', 'graphics_callback_mode');
  log = builtin('evalin', 'base', 'graphics_callback_log');
  log{end + 1} = event.sequence;
  builtin('assignin', 'base', 'graphics_callback_log', log);
  effect = builtin('evalin', 'base', 'graphics_callback_effect');
  builtin('assignin', 'base', 'graphics_callback_effect', effect + 1);
  builtin('assignin', 'base', 'graphics_callback_source_width', ...
          builtin('get', source, 'linewidth'));
  if (builtin('strcmp', mode, 'error'))
    builtin('error', 'NumJinn:GraphicsCallbackError', ...
            'Deliberate original-source graphics callback error.');
  elseif (builtin('strcmp', mode, 'loop'))
    builtin('fprintf', builtin('stdout'), 'NUMJINN_GRAPHICS_CALLBACK_STOP_READY');
    builtin('fflush', builtin('stdout'));
    while (true)
    endwhile
  elseif (! builtin('strcmp', mode, 'record'))
    builtin('error', 'NumJinn:GraphicsCallbackMode', 'Unsupported callback fixture mode.');
  endif
endfunction
