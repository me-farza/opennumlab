## SPDX-License-Identifier: GPL-3.0-or-later
## Private worker subfunction text; appended by Engine, never load-path resolved.
## The owner MUST revoke workspace tokens before invoking this retained closure.
## This qualified Octave profile is deliberately narrower than MATLAB graphics.
function dispatch = mathlab_graphics_service (worker_id, generation, invocation, native_bridge)
  session = builtin('struct', 'worker_id', worker_id, ...
                    'generation', builtin('sprintf', '%.0f', generation), ...
                    'invocation', builtin('sprintf', '%.0f', invocation));
  records = {};
  queue = {};
  queue_callbacks = {};
  serial = builtin('uint64', 0);
  sequence = builtin('uint64', 0);
  scene_revision = builtin('uint64', 0);
  epoch = [worker_id ':' session.generation ':' session.invocation];
  builtin('feval', native_bridge, 'open', epoch);
  last_effect_revision = builtin('uint64', 0);
  snapshot_nodes = {};
  unsupported = {};
  dispatch = @handle_request;

  function result = handle_request (request, context)
    current_effect_revision = mathlab_graphics_parse_counter(context.revision);
    ## Every graphics request revokes workspace tokens exactly once in the
    ## ordinary owner. A gap proves an intervening effectful operation. Retain
    ## live identity but conservatively revoke every prior property revision,
    ## even after change-and-restore. No replaceable property observer is needed
    ## for authority; outside-owner asynchronous event delivery is unqualified.
    if (last_effect_revision != 0 && current_effect_revision != mathlab_graphics_next_counter(last_effect_revision))
      touch_all ();
    endif
    last_effect_revision = current_effect_revision;
    switch (request.command)
      case 'graphics_snapshot'
        snapshot_nodes = {};
        unsupported = {};
        for sweep_index = 1:builtin('numel', records)
          if (records{sweep_index}.alive)
            try
              ensure_live(sweep_index);
            catch caught
              if (! builtin('any', builtin('strcmp', caught.identifier, {'MathLab:StaleGraphics', 'MathLab:GraphicsUnsupported'})))
                builtin('rethrow', caught);
              endif
            end_try_catch
          endif
        endfor
        ## Genuine native enumeration creates no figure, axes or current selection.
        ## Native enumeration allocates its complete handle vector first. The
        ## 128-node transport budget is not a native heap-allocation guarantee.
        handles = builtin('__go_figure_handles__', true);
        if (builtin('numel', handles) > 128)
          builtin('error', 'MathLab:GraphicsLimit', 'Graphics registry supports at most 128 native figure roots.');
        endif
        for root_index = 1:builtin('numel', handles)
          walk (handles(root_index), []);
        endfor
        result = builtin('struct', 'context', context, ...
                         'scene_revision', mathlab_graphics_decimal(scene_revision), ...
                         'nodes', {snapshot_nodes}, 'unsupported', {unsupported});
      case 'graphics_query'
        index = resolve (request.expected);
        [start, count] = mathlab_graphics_page_selection (request.start, request.count);
        result = query (index, context, start, count);
      case 'graphics_mutate'
        index = resolve (request.expected);
        mutation = request.mutation;
        if (! builtin('isstruct', mutation) || builtin('numel', mutation) != 1 || builtin('numfields', mutation) != 2 || ...
            ! builtin('isfield', mutation, 'kind') || ! builtin('isfield', mutation, 'value') || ...
            ! builtin('ischar', mutation.kind))
          builtin('error', 'MathLab:GraphicsValue', 'Expected one typed graphics mutation.');
        endif
        if (builtin('strcmp', mutation.kind, 'line_width'))
          require_kind (index, 'line');
          value = mutation.value;
          if (! mathlab_graphics_real_double(value) || builtin('numel', value) != 1 || ...
              ! builtin('isfinite', value) || value <= 0)
            builtin('error', 'MathLab:GraphicsValue', 'Line width must be one finite positive real double in points.');
          endif
          property = 'linewidth';
        elseif (builtin('any', builtin('strcmp', mutation.kind, {'x_lim', 'y_lim'})))
          require_kind (index, 'axes');
          value = mutation.value;
          if (! mathlab_graphics_real_double(value) || builtin('numel', value) != 2 || ...
              ! builtin('all', builtin('isfinite', value(:))) || value(1) >= value(2))
            builtin('error', 'MathLab:GraphicsValue', 'Axes limits require exactly two finite increasing real doubles.');
          endif
          value = value(:).';
          if (builtin('strcmp', mutation.kind, 'x_lim')); property = 'xlim'; else; property = 'ylim'; endif
        else
          builtin('error', 'MathLab:GraphicsUnsupported', 'This graphics mutation is unsupported.');
        endif
        ## The owner already revoked workspace tokens, including invalid writes.
        ## Advance before set: user listeners may mutate then fail or interrupt.
        touch_all ();
        builtin('set', records{index}.handle, property, value);
        ensure_live (index);
        result = query (index, context, 1, 512);
      case 'graphics_enqueue_callback'
        index = resolve (request.expected);
        require_kind (index, 'line');
        callback = named_callback(index);
        if (builtin('numel', queue) >= 32)
          builtin('error', 'MathLab:GraphicsLimit', 'Programmatic callback queue is limited to 32 entries.');
        endif
        sequence = mathlab_graphics_next_counter (sequence);
        result = builtin('struct', 'sequence', mathlab_graphics_decimal(sequence), ...
                         'expected', token(index), 'stimulus', 'programmatic');
        queue{end + 1} = result;
        queue_callbacks{end + 1} = callback;
      case 'graphics_cancel_callback'
        position = queued_position (request.ticket);
        result = queue{position};
        queue(position) = [];
        queue_callbacks(position) = [];
      case 'graphics_dispatch_callback'
        position = queued_position (request.ticket);
        if (position != 1)
          builtin('error', 'MathLab:GraphicsCallbackOrder', 'Programmatic callbacks must dispatch in FIFO order.');
        endif
        ticket = queue{1};
        callback = queue_callbacks{1};
        ## Remove before user dispatch. Failure and Stop cannot replay it.
        queue(1) = [];
        queue_callbacks(1) = [];
        index = resolve (ticket.expected);
        event = builtin('struct', 'schema_version', 1, 'stimulus', 'programmatic', ...
                        'sequence', ticket.sequence, 'workspace_revision', context.revision);
        ## A callback can alter arbitrary peer graphics, including objects whose
        ## replaceable property observers were removed. Revoke before dispatch.
        touch_all ();
        builtin('fprintf', builtin('stdout'), 'NUMJINN_GRAPHICS_PROGRAMMATIC:%s\n', ticket.sequence);
        builtin('fflush', builtin('stdout'));
        caught_error = [];
        started = builtin('tic');
        try
          ## Direct owned dispatch does not establish toolkit delivery or gcbo.
          builtin('feval', callback, records{index}.handle, event);
        catch caught
          caught_error = mathlab_error (caught);
        end_try_catch
        ## Execution-shaped results use the existing original-source remapper.
        ## No print/export/event pumping follows a callback; preserve PNG preview.
        result = builtin('struct', 'output', '', 'error', caught_error, ...
                         'workspace', {mathlab_safe_workspace()}, 'figures', {{}}, ...
                         'working_directory', builtin('pwd'), ...
                         'duration_ms', builtin('toc', started) * 1000, 'figure_warnings', {{}});
      otherwise
        builtin('error', 'MathLab:GraphicsUnsupported', 'Unknown graphics operation.');
    endswitch
    encoded = builtin('__unicode2native__', builtin('jsonencode', result), 'UTF-8');
    if (builtin('numel', encoded) > 1024 * 1024)
      builtin('error', 'MathLab:GraphicsLimit', 'Graphics result exceeds its 1 MiB payload budget.');
    endif
  endfunction

  function object_id = walk (handle, parent_id)
    object_id = [];
    ## Root/child lists contain native scalar handles, never caller objects.
    native_type = builtin('get', handle, 'type');
    visibility = builtin('get', handle, 'handlevisibility');
    if (! builtin('strcmp', visibility, 'on')); return; endif
    if (! builtin('any', builtin('strcmp', native_type, {'figure', 'axes', 'line'})))
      reject_node (parent_id, native_type, 'Only ordinary figure, Cartesian axes and real-double 2-D line objects are supported.');
      return;
    endif
    if (builtin('isempty', parent_id))
      supported_parent = builtin('strcmp', native_type, 'figure');
    else
      parent_index = record_id_index(parent_id);
      supported_parent = parent_index > 0 && ...
        ((builtin('strcmp', records{parent_index}.kind, 'figure') && builtin('strcmp', native_type, 'axes')) || ...
         (builtin('strcmp', records{parent_index}.kind, 'axes') && builtin('strcmp', native_type, 'line')));
    endif
    if (! supported_parent)
      reject_node (parent_id, native_type, 'Only a direct figure -> axes -> line hierarchy is supported.');
      return;
    endif
    try
      properties = read_properties (handle, native_type);
    catch caught
      if (! builtin('strcmp', caught.identifier, 'MathLab:GraphicsUnsupported')); builtin('rethrow', caught); endif
      retire_handle (handle);
      reject_node (parent_id, native_type, caught.message);
      return;
    end_try_catch
    index = register_object (handle, native_type, parent_id);
    if (builtin('numel', snapshot_nodes) >= 128)
      builtin('error', 'MathLab:GraphicsLimit', 'Graphics snapshot exceeds 128 supported nodes.');
    endif
    object_id = records{index}.id;
    slot = builtin('numel', snapshot_nodes) + 1;
    snapshot_nodes{slot} = builtin('struct', 'token', token(index), 'parent_id', mathlab_graphics_wire_parent(parent_id), ...
                                 'children', {{}}, 'properties', properties);
    children = {};
    if (! builtin('strcmp', native_type, 'line'))
      handles = builtin('get', handle, 'children');
      if (builtin('numel', handles) > 256)
        builtin('error', 'MathLab:GraphicsLimit', 'A supported graphics node exceeds 256 native children.');
      endif
      for child_index = 1:builtin('numel', handles)
        child_id = walk (handles(child_index), object_id);
        if (! builtin('isempty', child_id)); children{end + 1} = child_id; endif
      endfor
    endif
    snapshot_nodes{slot}.children = children;
    ## Publish the final token after child discovery, retaining the same live
    ## object identity across ordinary property refreshes.
    snapshot_nodes{slot}.token = token(index);
  endfunction

  function reject_node (parent_id, native_type, reason)
    if (builtin('numel', unsupported) >= 128)
      builtin('error', 'MathLab:GraphicsLimit', 'Graphics snapshot exceeds 128 unsupported-object diagnostics.');
    endif
    if (builtin('numel', reason) > 256); reason = reason(1:256); endif
    unsupported{end + 1} = builtin('struct', 'parent_id', mathlab_graphics_wire_parent(parent_id), ...
                                 'native_type', native_type, 'reason', reason);
  endfunction

  function index = register_object (handle, kind, parent_id)
    for record_index = 1:builtin('numel', records)
      if (records{record_index}.alive && records{record_index}.handle == handle)
        try
          ensure_live (record_index);
          if (! mathlab_graphics_same_parent(records{record_index}.parent_id, parent_id))
            retire (record_index);
          else
            index = record_index;
            return;
          endif
        catch caught
          if (! builtin('strcmp', caught.identifier, 'MathLab:StaleGraphics')); builtin('rethrow', caught); endif
        end_try_catch
      endif
    endfor
    if (builtin('numel', records) >= 4096)
      builtin('error', 'MathLab:GraphicsLimit', 'Graphics lifetime ledger exhausted; restart the worker.');
    endif
    serial = mathlab_graphics_next_counter (serial);
    scene_revision = mathlab_graphics_next_counter (scene_revision);
    object_id = ['g' mathlab_graphics_decimal(serial)];
    index = builtin('numel', records) + 1;
    native_key = builtin('feval', native_bridge, 'capture', epoch, handle);
    record = builtin('struct', 'handle', handle, 'kind', kind, 'id', object_id, ...
                     'parent_id', parent_id, 'lifetime', mathlab_graphics_decimal(serial), ...
                     'revision', builtin('uint64', 1), 'alive', true, 'native_key', native_key);
    records{index} = record;
  endfunction

  function touch_all ()
    for touch_index = 1:builtin('numel', records)
      if (records{touch_index}.alive); observe(touch_index, 'change'); endif
    endfor
  endfunction

  function observe (index, action)
    if (index > builtin('numel', records) || ! records{index}.alive); return; endif
    if (builtin('strcmp', action, 'delete'))
      retire (index);
    else
      records{index}.revision = mathlab_graphics_next_counter(records{index}.revision);
      scene_revision = mathlab_graphics_next_counter(scene_revision);
    endif
  endfunction

  function retire (index)
    if (! records{index}.alive); return; endif
    records{index}.alive = false;
    scene_revision = mathlab_graphics_next_counter(scene_revision);
    ## Descendant authority ends with its registered parent, even if a native
    ## deletion callback reparents a descendant before its own destruction.
    for descendant = 1:builtin('numel', records)
      if (records{descendant}.alive && mathlab_graphics_same_parent(records{descendant}.parent_id, records{index}.id))
        retire (descendant);
      endif
    endfor
  endfunction

  function retire_handle (handle)
    for record_index = 1:builtin('numel', records)
      if (records{record_index}.alive && records{record_index}.handle == handle); retire(record_index); endif
    endfor
  endfunction

  function ensure_live (index)
    record = records{index};
    if (! record.alive || ! builtin('feval', native_bridge, 'check', epoch, record.native_key, record.handle))
      retire(index);
      mathlab_graphics_stale ();
    endif
    try
      valid = builtin('strcmp', builtin('get', record.handle, 'type'), record.kind) && ...
              ! builtin('strcmp', builtin('get', record.handle, 'beingdeleted'), 'on');
      parent = builtin('get', record.handle, 'parent');
      if (builtin('isempty', record.parent_id))
        valid = valid && builtin('strcmp', record.kind, 'figure') && parent == 0;
      else
        parent_index = record_id_index (record.parent_id);
        valid = valid && parent_index > 0 && records{parent_index}.alive && records{parent_index}.handle == parent;
        if (valid); ensure_live(parent_index); endif
      endif
    catch
      valid = false;
    end_try_catch
    if (! valid); retire(index); mathlab_graphics_stale(); endif
    ## Validate the entire supported property profile before effects. No generic
    ## serialization, object traversal, custom subsref or project helper calls.
    try
      read_properties (record.handle, record.kind);
    catch caught
      retire(index);
      builtin('rethrow', caught);
    end_try_catch
  endfunction

  function index = resolve (expected)
    validate_token (expected);
    if (! builtin('strcmp', expected.context.worker_id, session.worker_id) || ...
        ! builtin('strcmp', expected.context.generation, session.generation) || ...
        ! builtin('strcmp', expected.context.invocation, session.invocation))
      mathlab_graphics_stale ();
    endif
    index = record_id_index(expected.object_id);
    if (index == 0 || ! builtin('strcmp', records{index}.lifetime, expected.lifetime)); mathlab_graphics_stale(); endif
    ensure_live(index);
    if (! builtin('strcmp', mathlab_graphics_decimal(records{index}.revision), expected.revision)); mathlab_graphics_stale(); endif
  endfunction

  function index = record_id_index (object_id)
    index = 0;
    for record_index = 1:builtin('numel', records)
      if (builtin('strcmp', records{record_index}.id, object_id)); index = record_index; return; endif
    endfor
  endfunction

  function expected = token (index)
    record = records{index};
    expected = builtin('struct', 'context', session, 'object_id', record.id, ...
                       'lifetime', record.lifetime, 'revision', mathlab_graphics_decimal(record.revision));
  endfunction

  function result = query (index, context, start, count)
    record = records{index};
    properties = read_properties(record.handle, record.kind);
    children = {};
    for child_index = 1:builtin('numel', records)
      if (records{child_index}.alive && mathlab_graphics_same_parent(records{child_index}.parent_id, record.id))
        ensure_live(child_index);
        children{end + 1} = records{child_index}.id;
      endif
    endfor
    node = builtin('struct', 'token', token(index), 'parent_id', mathlab_graphics_wire_parent(record.parent_id), ...
                   'children', {children}, 'properties', properties);
    samples = builtin('NaN');
    if (builtin('strcmp', record.kind, 'line'))
      [x, y] = line_data(record.handle);
      total = builtin('numel', x);
      if (start > builtin('max', 1, total))
        builtin('error', 'MathLab:GraphicsValue', 'Sample start is outside the live line.');
      endif
      last = builtin('min', total, start + count - 1);
      indices = builtin('cell', 1, builtin('max', 0, last - start + 1));
      x_page = indices;
      y_page = indices;
      for sample = start:last
        offset = sample - start + 1;
        indices{offset} = builtin('sprintf', '%.0f', sample);
        x_page{offset} = x(sample);
        y_page{offset} = y(sample);
      endfor
      samples = builtin('struct', 'class_name', 'double', 'source_shape', {{builtin('uint32', builtin('size', x, 1)), builtin('uint32', builtin('size', x, 2))}}, ...
                        'total_count', builtin('sprintf', '%.0f', total), ...
                        'start', builtin('sprintf', '%.0f', start), ...
                        'x', {x_page}, 'y', {y_page}, 'original_indices', {indices}, 'truncated', last < total);
    endif
    result = builtin('struct', 'context', context, 'node', node, 'samples', samples);
  endfunction

  function properties = read_properties (handle, kind)
    if (builtin('strcmp', kind, 'figure'))
      name = builtin('get', handle, 'name');
      toolkit = builtin('get', handle, '__graphics_toolkit__');
      if (! mathlab_graphics_bounded_text(name, 256, true) || ! mathlab_graphics_bounded_text(toolkit, 64, false)); mathlab_graphics_unsupported_profile(); endif
      properties = builtin('struct', 'kind', 'figure', 'name', name, 'toolkit', toolkit);
    elseif (builtin('strcmp', kind, 'axes'))
      if (! builtin('strcmp', builtin('get', handle, 'xscale'), 'linear') || ...
          ! builtin('strcmp', builtin('get', handle, 'yscale'), 'linear'))
        builtin('error', 'MathLab:GraphicsUnsupported', 'Only numeric linear Cartesian axes are supported.');
      endif
      view = builtin('get', handle, 'view');
      if (! mathlab_graphics_real_double(view) || builtin('numel', view) != 2 || view(1) != 0 || view(2) != 90)
        builtin('error', 'MathLab:GraphicsUnsupported', 'Only the ordinary 2-D Cartesian axes view is supported.');
      endif
      xlim = builtin('get', handle, 'xlim');
      ylim = builtin('get', handle, 'ylim');
      xmode = builtin('get', handle, 'xlimmode');
      ymode = builtin('get', handle, 'ylimmode');
      if (! mathlab_graphics_limits(xlim) || ! mathlab_graphics_limits(ylim) || ...
          ! builtin('any', builtin('strcmp', xmode, {'auto', 'manual'})) || ...
          ! builtin('any', builtin('strcmp', ymode, {'auto', 'manual'})))
        mathlab_graphics_unsupported_profile ();
      endif
      properties = builtin('struct', 'kind', 'axes', 'xlim', {{xlim(1), xlim(2)}}, ...
                           'ylim', {{ylim(1), ylim(2)}}, 'xlim_mode', xmode, 'ylim_mode', ymode);
    else
      width = builtin('get', handle, 'linewidth');
      if (! mathlab_graphics_real_double(width) || builtin('numel', width) != 1 || ! builtin('isfinite', width) || width <= 0)
        mathlab_graphics_unsupported_profile ();
      endif
      [x, y] = line_data(handle);
      properties = builtin('struct', 'kind', 'line', 'line_width', width, ...
                           'source_shape', {{builtin('uint32', builtin('size', x, 1)), builtin('uint32', builtin('size', x, 2))}}, ...
                           'total_count', builtin('sprintf', '%.0f', builtin('numel', x)));
    endif
  endfunction

  function [x, y] = line_data (handle)
    x = builtin('get', handle, 'xdata');
    y = builtin('get', handle, 'ydata');
    z = builtin('get', handle, 'zdata');
    if (! mathlab_graphics_real_double(x) || ! mathlab_graphics_real_double(y) || ! builtin('isempty', z) || ...
        builtin('ndims', x) != 2 || builtin('ndims', y) != 2 || ...
        (builtin('size', x, 1) != 1 && builtin('size', x, 2) != 1) || ...
        (builtin('size', y, 1) != 1 && builtin('size', y, 2) != 1) || ...
        builtin('numel', x) != builtin('numel', y) || builtin('numel', x) > 65536 || ...
        ! builtin('all', builtin('isfinite', x(:))) || ! builtin('all', builtin('isfinite', y(:))))
      builtin('error', 'MathLab:GraphicsUnsupported', 'Only finite real-double 2-D line vectors of at most 65536 samples are supported.');
    endif
  endfunction

  function callback = named_callback (index)
    callback = builtin('get', records{index}.handle, 'buttondownfcn');
    if (! builtin('strcmp', builtin('class', callback), 'function_handle') || builtin('numel', callback) != 1)
      builtin('error', 'MathLab:GraphicsUnsupported', 'Programmatic dispatch requires one original named function-handle ButtonDownFcn.');
    endif
    description = builtin('functions', callback);
    if (! builtin('isfield', description, 'type') || ...
        ! builtin('any', builtin('strcmp', description.type, {'simple', 'scopedfunction'})) || ...
        ! builtin('isfield', description, 'file') || ! mathlab_graphics_bounded_text(description.file, 4096, false))
      builtin('error', 'MathLab:GraphicsUnsupported', 'Anonymous, builtin, string and cell callbacks are unsupported.');
    endif
  endfunction

  function position = queued_position (ticket)
    if (! builtin('isstruct', ticket) || builtin('numel', ticket) != 1 || builtin('numfields', ticket) != 3 || ...
        ! builtin('isfield', ticket, 'sequence') || ! mathlab_graphics_canonical(ticket.sequence) || ...
        ! builtin('isfield', ticket, 'stimulus') || ! builtin('ischar', ticket.stimulus) || ...
        ! builtin('strcmp', ticket.stimulus, 'programmatic') || ! builtin('isfield', ticket, 'expected'))
      builtin('error', 'MathLab:GraphicsValue', 'Expected a bounded programmatic callback ticket.');
    endif
    validate_token(ticket.expected);
    ## Reject a ticket from a retired session even when the new queue is empty.
    if (! builtin('strcmp', ticket.expected.context.worker_id, session.worker_id) || ...
        ! builtin('strcmp', ticket.expected.context.generation, session.generation) || ...
        ! builtin('strcmp', ticket.expected.context.invocation, session.invocation)); mathlab_graphics_stale(); endif
    for position = 1:builtin('numel', queue)
      candidate = queue{position};
      if (builtin('strcmp', ticket.sequence, candidate.sequence))
        if (! mathlab_graphics_same_token(ticket.expected, candidate.expected))
          builtin('error', 'MathLab:GraphicsValue', 'Callback ticket does not match the queued operation.');
        endif
        return;
      endif
    endfor
    builtin('error', 'MathLab:GraphicsCallbackMissing', 'This programmatic callback was canceled, consumed or never queued.');
  endfunction

  function validate_token (expected)
    if (! builtin('isstruct', expected) || builtin('numel', expected) != 1 || builtin('numfields', expected) != 4 || ...
        ! builtin('isfield', expected, 'context') || ! builtin('isstruct', expected.context) || ...
        builtin('numel', expected.context) != 1 || builtin('numfields', expected.context) != 3 || ...
        ! builtin('isfield', expected, 'object_id') || ! mathlab_graphics_bounded_text(expected.object_id, 128, false) || ...
        ! builtin('isfield', expected, 'lifetime') || ! mathlab_graphics_canonical(expected.lifetime) || ...
        ! builtin('isfield', expected, 'revision') || ! mathlab_graphics_canonical(expected.revision))
      mathlab_graphics_stale ();
    endif
    context = expected.context;
    if (! builtin('isfield', context, 'worker_id') || ! mathlab_graphics_bounded_text(context.worker_id, 256, false) || ...
        ! builtin('isfield', context, 'generation') || ! mathlab_graphics_canonical(context.generation) || ...
        ! builtin('isfield', context, 'invocation') || ! mathlab_graphics_canonical(context.invocation))
      mathlab_graphics_stale ();
    endif
  endfunction

  function require_kind (index, kind)
    if (! builtin('strcmp', records{index}.kind, kind))
      builtin('error', 'MathLab:GraphicsUnsupported', 'This property does not belong to the selected supported object kind.');
    endif
  endfunction
endfunction

## Enable only the exact graphics callback already captured by trusted startup.
## These remain private subfunctions of the worker; never resolve a project file
## named mathlab_graphics_service or a separately installed helper.
function [dispatch, expected] = mathlab_graphics_enable (session_directory, bridge_path, worker_id, generation, invocation)
  if (! mathlab_graphics_bounded_text(bridge_path, 4096, false))
    builtin('error', 'MathLab:GraphicsUnsupported', 'Expected the exact graphics bridge file path.');
  endif
  expected = builtin('canonicalize_file_name', bridge_path);
  if (builtin('isempty', expected) || ! builtin('exist', expected, 'file'))
    builtin('error', 'MathLab:GraphicsUnsupported', 'The selected native graphics lifetime bridge is unavailable.');
  endif
  [bridge, expected] = mathlab_native_adapter ('graphics', expected);
  protocol = builtin('feval', bridge, 'protocol');
  if (! builtin('isstruct', protocol) || builtin('numel', protocol) != 1 || ...
      ! builtin('isfield', protocol, 'protocol') || ! builtin('strcmp', protocol.protocol, 'mathlab-graphics') || ...
      ! builtin('isfield', protocol, 'version') || protocol.version != 1 || ...
      ! builtin('isfield', protocol, 'native_object_lifetime') || ! builtin('islogical', protocol.native_object_lifetime) || ...
      ! protocol.native_object_lifetime || ! builtin('isfield', protocol, 'bound_primitive_dispatcher') || ...
      ! builtin('islogical', protocol.bound_primitive_dispatcher) || ! protocol.bound_primitive_dispatcher || ...
      ! builtin('isfield', protocol, 'max_entries') || protocol.max_entries != 4096 || ...
      ! builtin('isfield', protocol, 'octave_version') || ! builtin('strcmp', protocol.octave_version, builtin('OCTAVE_VERSION')))
    builtin('error', 'MathLab:GraphicsUnsupported', 'The native graphics lifetime bridge protocol/runtime does not match.');
  endif
  dispatch = mathlab_graphics_service(worker_id, generation, invocation, bridge);
  ## Recovery retains the module only within this generation. Restart
  ## reuses the session directory and must not revive an older marker.
  target = [session_directory '/graphics-bridge-' builtin('sprintf', '%.0f', generation) '.path'];
  stream = builtin('fopen', target, 'wb');
  if (stream < 0); builtin('error', 'MathLab:GraphicsUnsupported', 'Cannot retain the selected graphics bridge path.'); endif
  unwind_protect
    written = builtin('fwrite', stream, expected, 'char');
    if (written != builtin('numel', expected)); builtin('error', 'MathLab:GraphicsUnsupported', 'Cannot retain the complete graphics bridge path.'); endif
  unwind_protect_cleanup
    builtin('fclose', stream);
  end_unwind_protect
endfunction

function [dispatch, bridge_path] = mathlab_graphics_restore (session_directory, worker_id, generation, invocation)
  dispatch = [];
  bridge_path = '';
  ## A fresh/restarted process requires explicit enable. Only recovery of the
  ## same process restores configuration, with a new invocation/registry.
  if (invocation <= 1); return; endif
  target = [session_directory '/graphics-bridge-' builtin('sprintf', '%.0f', generation) '.path'];
  if (! builtin('exist', target, 'file')); return; endif
  stream = builtin('fopen', target, 'rb');
  if (stream < 0); return; endif
  try
    selected_path = builtin('fread', stream, 4097, '*char').';
    builtin('fclose', stream);
    stream = -1;
    if (builtin('numel', selected_path) > 4096); return; endif
    [dispatch, bridge_path] = mathlab_graphics_enable(session_directory, selected_path, worker_id, generation, invocation);
  catch
    ## Recovery must leave the ordinary numerical session usable. A missing or
    ## incompatible module removes the optional capability; never weakly fall
    ## back to numeric-handle identity. Old invocation tokens still fail closed.
    if (stream >= 0); builtin('fclose', stream); endif
    dispatch = [];
    bridge_path = '';
  end_try_catch
endfunction

function result = mathlab_graphics_real_double (value)
  result = builtin('strcmp', builtin('class', value), 'double') && ...
           builtin('isnumeric', value) && builtin('isreal', value) && ! builtin('issparse', value);
endfunction

function result = mathlab_graphics_limits (value)
  result = mathlab_graphics_real_double(value) && builtin('numel', value) == 2 && ...
           builtin('all', builtin('isfinite', value(:))) && value(1) < value(2);
endfunction

function [start, count] = mathlab_graphics_page_selection (start, count)
  if (! mathlab_graphics_real_double(start) || ! mathlab_graphics_real_double(count) || builtin('numel', start) != 1 || ...
      builtin('numel', count) != 1 || ! builtin('isfinite', start) || ! builtin('isfinite', count) || ...
      start < 1 || start > 65536 || count < 1 || count > 512 || ...
      builtin('fix', start) != start || builtin('fix', count) != count)
    builtin('error', 'MathLab:GraphicsValue', 'A graphics sample page requires start 1..65536 and count 1..512.');
  endif
endfunction

function result = mathlab_graphics_bounded_text (value, maximum, allow_empty)
  result = builtin('ischar', value) && (builtin('isempty', value) || builtin('size', value, 1) == 1) && ...
           builtin('numel', value) <= maximum && (allow_empty || ! builtin('isempty', value));
  if (result)
    result = builtin('numel', builtin('__unicode2native__', value, 'UTF-8')) <= maximum && ...
             ! builtin('any', value < 32) && ! builtin('any', value == 127);
  endif
endfunction

function result = mathlab_graphics_canonical (value)
  result = mathlab_graphics_bounded_text(value, 20, false) && builtin('all', value >= '0' & value <= '9') && ...
           (builtin('numel', value) == 1 || value(1) != '0');
  maximum = '18446744073709551615';
  if (result && builtin('numel', value) == 20)
    difference = builtin('find', value != maximum, 1);
    if (! builtin('isempty', difference)); result = value(difference) < maximum(difference); endif
  endif
endfunction

function value = mathlab_graphics_decimal (counter)
  value = builtin('sprintf', '%u', counter);
endfunction

function counter = mathlab_graphics_parse_counter (value)
  if (! mathlab_graphics_canonical(value)); mathlab_graphics_stale(); endif
  counter = builtin('uint64', 0);
  for digit = 1:builtin('numel', value)
    counter = counter * builtin('uint64', 10) + builtin('uint64', value(digit) - '0');
  endfor
endfunction

function next = mathlab_graphics_next_counter (counter)
  if (counter == builtin('intmax', 'uint64'))
    builtin('error', 'MathLab:GraphicsLimit', 'Graphics identity counter exhausted; restart the worker.');
  endif
  next = counter + builtin('uint64', 1);
endfunction

function result = mathlab_graphics_same_parent (left, right)
  result = (builtin('isempty', left) && builtin('isempty', right)) || ...
           (builtin('ischar', left) && builtin('ischar', right) && builtin('strcmp', left, right));
endfunction

function result = mathlab_graphics_same_token (left, right)
  result = builtin('strcmp', left.object_id, right.object_id) && ...
           builtin('strcmp', left.lifetime, right.lifetime) && ...
           builtin('strcmp', left.revision, right.revision) && ...
           builtin('strcmp', left.context.worker_id, right.context.worker_id) && ...
           builtin('strcmp', left.context.generation, right.context.generation) && ...
           builtin('strcmp', left.context.invocation, right.context.invocation);
endfunction

## jsonencode represents a structural NaN as JSON null. Samples themselves are
## proven finite before serialization; [] instead would be the JSON array [].
function parent = mathlab_graphics_wire_parent (parent)
  if (builtin('isempty', parent)); parent = builtin('NaN'); endif
endfunction

function mathlab_graphics_stale ()
  builtin('error', 'MathLab:StaleGraphics', 'The graphics session, lifetime or revision changed. Refresh the supported object tree.');
endfunction

function mathlab_graphics_unsupported_profile ()
  builtin('error', 'MathLab:GraphicsUnsupported', 'This object is outside the bounded graphics property profile.');
endfunction
