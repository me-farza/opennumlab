## SPDX-License-Identifier: GPL-3.0-or-later
## An isolated protocol function keeps transport variables out of the base workspace.
function mathlab_worker (session_directory, announce_ready = (1 == 1), debugger_enabled = (1 == 0), scope_bridge_path = '', response_prefix = '', generation = 1, console_helper_path = '', invocation = 1, input_bridge_path = '', value_bridge_path = '', intrinsic_bindings = [], initial_directory = '')
  ## Private V2 only: the provider passes actual transport objects before entry.
  mathlab_intrinsics (1, intrinsic_bindings);
  [builtin, unused_provider, admitted_adapters] = mathlab_intrinsics ();
  input_bridge = [];
  scope_bridge = [];
  debug_bridge = [];
  if (! builtin('isempty', scope_bridge_path))
    scope_bridge = mathlab_native_adapter ('scope', scope_bridge_path);
  endif
  if (debugger_enabled)
    if (! builtin('isstruct', admitted_adapters) ...
        || ! builtin('isfield', admitted_adapters, 'debug'))
      builtin('error', 'MathLab:IntrinsicAdapterMissing', 'Debugger was not admitted before project entry.');
    endif
    debug_bridge = mathlab_native_adapter ('debug', admitted_adapters.debug.path);
  endif
  builtin('clear', '-variables', 'intrinsic_bindings', 'unused_provider', 'admitted_adapters');
  registry_directory = [session_directory '/registry'];
  registry_path = [registry_directory '/mathlab_scope_register.m'];
  if (builtin('isempty', console_helper_path) || ! builtin('exist', console_helper_path, 'file'))
    builtin('error', 'MathLab:ConsoleHelper', 'The session console fence helper is unavailable.');
  endif
  if (announce_ready)
    builtin('page_screen_output', (1 == 0));
    builtin('page_output_immediately', (1 == 1));
    toolkits = builtin('available_graphics_toolkits');
    if (builtin('any', builtin('strcmp', toolkits, 'gnuplot')))
      ## On native macOS Octave, the first figure initializes the graphics manager
      ## and can reset an earlier default-toolkit choice to FLTK. Warm that manager
      ## with an explicitly selected invisible figure before choosing our default.
      builtin('evalc', "graphics_toolkit ('gnuplot'); bootstrap_figure = figure ('visible', 'off', '__graphics_toolkit__', 'gnuplot'); close (bootstrap_figure); graphics_toolkit ('gnuplot');");
    endif
    builtin('set', 0, 'defaultfigurevisible', 'off');
  endif
  ## Retained code is ready before project entry; no path-based adapter load.
  if (announce_ready && ! builtin('isempty', initial_directory))
    builtin('cd', initial_directory);
  endif
  request_path = [session_directory '/request.json'];
  worker_active = (1 == 1);
  response_sent = (1 == 1);
  request_id = 0;
  execution_request = (1 == 0);
  captured = '';
  started = builtin('tic');
  binding_revision = builtin('uint64', 0);
  edit_history = mathlab_edit_history_new (invocation);
  ## Optional startup-only adapter. Its canonical path must be supplied by the
  ## Engine from an explicitly trusted matching binary, including recovery.
  ## Retain the configured path in worker scope. Checked dispatch selects
  ## its already admitted actual callback; a changed path requires restart.
  value_bridge = [];
  unwind_protect
    ## Recovery re-entry has its own readiness fence. Rust waits for it before
    ## publishing the interrupted request's completion or accepting another command.
    mathlab_begin (0, response_prefix, generation, console_helper_path, invocation);
    try
      ## MATHLAB_VALUE_BRIDGE_READY_VALIDATION
      value_bridge = mathlab_value_bridge_open (value_bridge_path);
      startup_response = builtin('struct', 'id', 0, 'ok', (1 == 1), 'result', ...
                                 mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge));
      [startup_response, startup_encoded] = mathlab_encode_response (startup_response, (1 == 1));
    catch caught
      ## Encoding is part of readiness validation. Do not replace a producer
      ## failure with Interrupted/R0 and recursively re-enter the same failure.
      ## Disable recovery even if encoding the diagnostic itself then fails.
      worker_active = (1 == 0);
      startup_response = builtin('struct', 'id', 0, 'ok', (1 == 0), 'error', mathlab_error(caught));
      [startup_response, startup_encoded] = mathlab_encode_response (startup_response, (1 == 0));
    end_try_catch
    if (! startup_response.ok); worker_active = (1 == 0); endif
    ## Publish once, outside the validation catch: an I/O/fence failure must
    ## never overwrite metadata that may already have been committed.
    mathlab_publish (startup_response, startup_encoded, session_directory, response_prefix, ...
                     generation, console_helper_path, invocation);
    response_sent = (1 == 1);
    builtin('clear', '-variables', 'startup_response', 'startup_encoded', 'caught');
    if (! worker_active); return; endif
    if (debugger_enabled && ! builtin('isempty', input_bridge_path))
      input_bridge = mathlab_native_adapter ('input', input_bridge_path);
      debug_bridge ('input_adapter', input_bridge);
    endif
    while (1 == 1)
      if (! builtin('isempty', input_bridge_path))
        input_bridge = mathlab_native_adapter ('input', input_bridge_path);
        input_bridge ('context', generation, invocation, 0);
      endif
      if (debugger_enabled)
        debug_bridge ('poll');
      endif
      ## Octave stdin can read ahead until EOF under --eval. An atomic file mailbox
      ## keeps request input independent of user input() and works on all platforms.
      if (! builtin('exist', request_path, 'file'))
        builtin('pause', 0.01);
        continue;
      endif
      request_file = builtin('fopen', request_path, 'r');
      if (request_file < 0)
        builtin('pause', 0.01);
        continue;
      endif
      request_line = builtin('fgetl', request_file);
      builtin('fclose', request_file);
      range_edit_started = (1 == 0);
      result_is_ready = (1 == 0);
      ## Request metadata and error structs are not durable session state.
      ## This outer guard also runs if publication or SIGINT leaves the try.
      unwind_protect
      try
        request = builtin('jsondecode', request_line);
        request_id = request.id;
        response_sent = (1 == 0);
        execution_request = builtin('any', builtin('strcmp', request.command, {'execute', 'run_file'}));
        captured = '';
        started = builtin('tic');
        ## Arbitrary execution, legacy previews and package/path operations may
        ## run user hooks or mutate before an error/Stop. Revoke snapshots before
        ## dispatch, never only after success. Checked operations own their commit.
        if (! builtin('any', builtin('strcmp', request.command, {'ping', 'workspace_snapshot', ...
                                            'read_variable_checked', 'set_variable_element_checked', ...
                                            'read_value_page_checked', 'write_value_range_checked', ...
                                            'undo_value_edit_checked', 'redo_value_edit_checked'})))
          edit_history = mathlab_edit_history_invalidate (edit_history);
          binding_revision = mathlab_next_revision (binding_revision);
        endif
        builtin('unlink', request_path);
        mathlab_begin (request_id, response_prefix, generation, console_helper_path, invocation);
        if (! builtin('isempty', input_bridge_path))
          input_bridge = mathlab_native_adapter ('input', input_bridge_path);
          input_bridge ('context', generation, invocation, request_id);
        endif
        cancel_path = [session_directory '/' builtin('sprintf', 'cancel-%.0f-%.0f', generation, request_id)];
        if (builtin('exist', cancel_path, 'file'))
          interrupted = mathlab_interrupted ();
          if (execution_request)
            result = mathlab_interrupted_result (interrupted, started);
          else
            builtin('error', 'MathLab:Interrupted', 'Request canceled before execution. Workspace preserved.');
          endif
        else
        ## All dispatch-local candidates must die while the accepted base root
        ## still owns every opaque sibling. Never retain a root between requests.
        unwind_protect
        switch (request.command)
          case {'execute', 'run_file'}
            execution_request = (1 == 1);
            execution_error = [];
            captured = '';
            retained_handles = {};
            started = builtin('tic');
            if (! builtin('isempty', scope_bridge_path))
              ## User clear functions/all may reset the unlocked numerical
              ## registry. Rebind it without retaining its user-function roots.
              scope_bridge = mathlab_native_adapter ('scope', scope_bridge_path);
              scope_bridge ('registry', registry_path);
              mathlab_scope_call ('__configuration__', {}, (1 == 1));
            endif
            if (debugger_enabled)
              debug_bridge ('registry', registry_path);
              ## The debugger marks the exact retained local-function objects
              ## after each script's scope registration and optional native bind.
              mathlab_scope_call ('__debug_configuration__', {}, (1 == 1));
              ## Reconcile user dbclear commands from prior requests before a
              ## direct retained-handle call can encounter stale AST marks.
              debug_bridge ('synchronize', 0);
            endif
            if (builtin('strcmp', request.command, 'run_file'))
              source_expression = "builtin('source', request.path, 'base');";
              if (debugger_enabled)
                if (builtin('isfield', request, 'original_path'))
                  source_expression = "if (! debug_bridge ('source', request.path, retained_handles, request.original_path, request.original_source)); execution_error = mathlab_debug_quit (); endif;";
                else
                  source_expression = "if (! debug_bridge ('source', request.path)); execution_error = mathlab_debug_quit (); endif;";
                endif
              elseif (! builtin('isempty', scope_bridge_path) && builtin('isfield', request, 'source_kind') && builtin('strcmp', request.source_kind, 'script'))
                source_expression = "scope_bridge ('source', request.path, retained_handles, request.original_path, request.original_source);";
              endif
              if (builtin('isfield', request, 'source_directory'))
                injected_path = request.source_directory;
                path_was_present = mathlab_path_contains (injected_path);
                if (! path_was_present)
                  builtin('addpath', injected_path, '-end');
                endif
                unwind_protect
                  ## Restore configuration after adding the capsule, and retain
                  ## exact handles in transport scope before any user statement.
                  ## Native command-line registration keeps the helper stable
                  ## across Octave's path-cache refreshes.
                  try
                    if (! builtin('isempty', scope_bridge_path))
                      mathlab_scope_call ('__configuration__', {}, (1 == 1));
                    endif
                    if (debugger_enabled)
                      mathlab_scope_call ('__debug_configuration__', {}, (1 == 1));
                    endif
                    if (builtin('isfield', request, 'local_names') && ! builtin('isempty', request.local_names))
                      ## Registration is performed only in this transport scope.
                      ## No helper identifier or temporary variable enters user code.
                      retained_handles = mathlab_scope_call (request.path, {}, 'lookup');
                      if (builtin('isempty', retained_handles))
                        if (! builtin('isempty', scope_bridge_path))
                          names = request.local_names;
                          if (builtin('ischar', names)); names = {names}; endif
                          retained_handles = scope_bridge ('prepare', request.path, names, request.original_path, request.original_source);
                        else
                          factory_name = mathlab_intrinsic_factory_name (request.factory_path);
                          mathlab_require_function (factory_name, request.factory_path);
                          retained_handles = builtin('feval', factory_name);
                        endif
                      endif
                      mathlab_scope_call (request.path, retained_handles, request.original_path, request.original_source);
                    endif
                  catch caught
                    execution_error = mathlab_error (caught);
                  end_try_catch
                  if (builtin('isempty', execution_error))
                    try
                      builtin('eval', source_expression);
                    catch caught
                      execution_error = mathlab_error (caught);
                    end_try_catch
                  endif
                unwind_protect_cleanup
                  if (request.keep_source_directory)
                    ## Stable .mlsource files cannot be autoloaded by script name.
                    ## Keep their parent scope and contents unchanged so private
                    ## function persistent variables survive subsequent requests.
                  elseif (! path_was_present && mathlab_path_contains (injected_path))
                    ## Remove only our entry; preserve intentional user path changes.
                    builtin('rmpath', injected_path);
                  endif
                end_unwind_protect
              else
                try
                  builtin('eval', source_expression);
                catch caught
                  execution_error = mathlab_error (caught);
                end_try_catch
              endif
            else
              code = request.code;
              try
                if (debugger_enabled)
                  if (! debug_bridge ('evaluate', code))
                    execution_error = mathlab_debug_quit ();
                  endif
                else
                  builtin('evalin', 'base', code);
                endif
              catch caught
                execution_error = mathlab_error (caught);
              end_try_catch
            endif
            ## A real Stop inside a debugger watch first unwinds the watch, then
            ## exits the outer debugger through its guarded Quit boundary. The
            ## durable request marker distinguishes it from an explicit Quit.
            if (builtin('isstruct', execution_error) && builtin('isfield', execution_error, 'identifier') ...
                && builtin('strcmp', execution_error.identifier, 'MathLab:DebugQuit') ...
                && builtin('exist', cancel_path, 'file'))
              execution_error = mathlab_interrupted ();
            endif
            if (builtin('isstruct', execution_error) && builtin('isfield', execution_error, 'identifier') ...
                && builtin('strcmp', execution_error.identifier, 'MathLab:Interrupted'))
              figures = {};
              figure_warnings = {};
            else
              [figures, figure_warnings] = mathlab_figures (session_directory, request_id);
            endif
            result = builtin('struct', 'output', captured, 'error', execution_error, ...
                             'workspace', {mathlab_workspace()}, 'figures', {figures}, ...
                             'working_directory', builtin('pwd'), 'duration_ms', builtin('toc', started) * 1000, ...
                             'figure_warnings', {figure_warnings});
          case 'ping'
            ## Service pending interpreter interrupts before confirming readiness.
            builtin('pause', 0);
            if (! builtin('isempty', value_bridge_path))
              value_bridge = mathlab_validate_value_bridge (value_bridge_path);
            endif
            result = mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge);
            result_is_ready = (1 == 1);
          case {'workspace_snapshot', 'reset_value_edit_history'}
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            metadata = mathlab_safe_workspace ();
            bound = builtin('cell', 1, builtin('numel', metadata));
            for variable_index = 1:builtin('numel', metadata)
              bound{variable_index} = builtin('struct', 'token', builtin('struct', 'context', context, 'name', metadata{variable_index}.name), ...
                                             'variable', metadata{variable_index});
            endfor
            result = builtin('struct', 'context', context, 'variables', {bound});
          case 'read_variable_checked'
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            mathlab_check_binding (request, context);
            result = builtin('struct', 'token', request.expected, 'slice', mathlab_read_variable (request, (1 == 1)));
          case 'read_value_page_checked'
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            binding_request = builtin('struct', 'name', request.path.root.name, 'expected', request.path.root);
            mathlab_check_binding (binding_request, context);
            ## The private appended helper traverses only genuine containers and
            ## primitive leaves, under the same owner and accepted root token.
            if (! builtin('isempty', value_bridge_path))
              value_bridge = mathlab_validate_value_bridge (value_bridge_path);
            endif
            result = mathlab_value_page (request, value_bridge);
          case 'set_variable_element_checked'
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            mathlab_check_binding (request, context);
            replacement = mathlab_set_variable_element (request, (1 == 0));
            ## Validation constructs an owned replacement. Revision revocation
            ## and its single assignment are in this same interpreter operation.
            edit_history = mathlab_edit_history_invalidate (edit_history);
            binding_revision = mathlab_next_revision (binding_revision);
            builtin('assignin', 'base', request.name, replacement);
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            result = builtin('struct', 'token', builtin('struct', 'context', context, 'name', request.name), ...
                             'workspace', {mathlab_safe_workspace()});
          case {'write_value_range_checked', 'undo_value_edit_checked', 'redo_value_edit_checked'}
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            if (builtin('strcmp', request.command, 'write_value_range_checked'))
              edit_token = mathlab_page_token (request.path.root);
              operation = 'write';
              edit_request = request;
            else
              edit_token = mathlab_page_token (request.expected);
              if (builtin('strcmp', request.command, 'undo_value_edit_checked')); operation = 'undo'; else; operation = 'redo'; endif
            endif
            binding_request = builtin('struct', 'name', edit_token.name, 'expected', edit_token);
            mathlab_check_binding (binding_request, context);
            if (! builtin('isempty', value_bridge_path))
              value_bridge = mathlab_validate_value_bridge (value_bridge_path);
            endif
            if (! builtin('strcmp', operation, 'write'))
              edit_request = mathlab_edit_history_restore_request (edit_history, request, operation, context);
            endif
            [replacement, patch] = mathlab_prepare_value_edit (edit_request, value_bridge);
            next_revision = mathlab_next_revision (binding_revision);
            next_context = mathlab_binding_context (response_prefix, generation, invocation, next_revision);
            next_token = builtin('struct', 'context', next_context, 'name', edit_token.name);
            if (builtin('strcmp', operation, 'write'))
              [candidate_history, edit_receipt] = mathlab_edit_history_accept_write (edit_history, patch, next_token);
            else
              [candidate_history, edit_receipt] = mathlab_edit_history_accept_restore (edit_history, request, operation, next_token);
            endif
            ## All candidate values, bounded history and receipt are ready.
            ## MATHLAB_VALUE_EDIT_PRECOMMIT_BOUNDARY
            range_edit_started = (1 == 1);
            binding_revision = next_revision;
            builtin('assignin', 'base', edit_token.name, replacement);
            ## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY
            edit_history = candidate_history;
            result = edit_receipt;
          case 'workspace'
            result = mathlab_workspace ();
          case 'read_variable'
            result = mathlab_read_variable (request);
          case 'set_variable_element'
            mathlab_set_variable_element (request);
            result = mathlab_workspace ();
          case 'set_directory'
            builtin('cd', request.path);
            result = builtin('struct', 'working_directory', builtin('pwd'));
          case 'clear'
            builtin('evalin', 'base', 'clear -variables');
            result = [];
          case 'list_packages'
            installed = pkg ('list');
            result = builtin('cell', 1, builtin('numel', installed));
            for package_index = 1:builtin('numel', installed)
              package = installed{package_index};
              result{package_index} = builtin('struct', 'name', package.name, 'version', package.version, ...
                                             'description', package.description, 'loaded', builtin('logical', package.loaded));
            endfor
          case 'load_package'
            pkg ('load', request.name);
            result = '';
          case 'enable_debugger'
            ## Select admitted code; the existing protocol and install below
            ## remain responsible for enabling debugger behavior.
            debug_bridge = mathlab_native_adapter ('debug', request.bridge_path);
            try
              bridge_protocol = debug_bridge ('protocol', 0);
            catch caught
              builtin('error', 'MathLab:DebuggerProtocol', ...
                     'Debugger adapter is outdated or incompatible; rebuild it against the selected Octave runtime. %s', caught.message);
            end_try_catch
            if (! builtin('isstruct', bridge_protocol) || ! builtin('isfield', bridge_protocol, 'protocol') || ...
                ! builtin('isfield', bridge_protocol, 'version') || ! builtin('ischar', bridge_protocol.protocol) || ...
                ! builtin('strcmp', bridge_protocol.protocol, 'mathlab-debugger') || ...
                ! builtin('isnumeric', bridge_protocol.version) || ! builtin('isscalar', bridge_protocol.version) || ...
                bridge_protocol.version != 4 || ...
                ! builtin('isfield', bridge_protocol, 'streamed_evaluation') || ! builtin('islogical', bridge_protocol.streamed_evaluation) || ...
                ! builtin('isscalar', bridge_protocol.streamed_evaluation) || ! bridge_protocol.streamed_evaluation || ...
                ! builtin('isfield', bridge_protocol, 'input_wait_cancel') || ! builtin('islogical', bridge_protocol.input_wait_cancel) || ...
                ! builtin('isscalar', bridge_protocol.input_wait_cancel) || ! bridge_protocol.input_wait_cancel || ...
                ! builtin('isfield', bridge_protocol, 'stable_registry') || ! builtin('islogical', bridge_protocol.stable_registry) || ...
                ! builtin('isscalar', bridge_protocol.stable_registry) || ! bridge_protocol.stable_registry || ...
                ! builtin('isfield', bridge_protocol, 'off_path_registry') || ! builtin('islogical', bridge_protocol.off_path_registry) || ...
                ! builtin('isscalar', bridge_protocol.off_path_registry) || ! bridge_protocol.off_path_registry || ...
                ! builtin('isfield', bridge_protocol, 'source_entry') || ! builtin('islogical', bridge_protocol.source_entry) || ...
                ! builtin('isscalar', bridge_protocol.source_entry) || ! bridge_protocol.source_entry || ...
                ! builtin('isfield', bridge_protocol, 'breakpoint_source_metadata') || ...
                ! builtin('islogical', bridge_protocol.breakpoint_source_metadata) || ...
                ! builtin('isscalar', bridge_protocol.breakpoint_source_metadata) || ! bridge_protocol.breakpoint_source_metadata)
              builtin('error', 'MathLab:DebuggerProtocol', 'Debugger adapter protocol is unsupported; rebuild the adapter.');
            endif
            debug_bridge (request.debug_directory);
            debug_bridge ('registry', registry_path);
            if (mathlab_path_contains (registry_directory)); builtin('rmpath', registry_directory); endif
            debugger_enabled = (1 == 1);
            if (! builtin('isempty', input_bridge_path))
              input_bridge = mathlab_native_adapter ('input', input_bridge_path);
              debug_bridge ('input_adapter', input_bridge);
            endif
            mathlab_scope_call ('__debug_configuration__', {}, (1 == 1));
            result = [];
          case 'enable_value_bridge'
            ## Effectful installation is intentionally absent from the pure
            ## dispatch whitelist: bindings/history were revoked before loading.
            if (! builtin('ischar', request.bridge_path) || builtin('isempty', request.bridge_path))
              builtin('error', 'MathLab:ValuePagePrecisionProtocol', 'Adapter installation requires a nonempty canonical .oct path.');
            endif
            installed_value_bridge = mathlab_value_bridge_open (request.bridge_path);
            value_bridge = installed_value_bridge;
            value_bridge_path = request.bridge_path;
            result = mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge);
            result_is_ready = (1 == 1);
          case 'enable_input_bridge'
            input_bridge = mathlab_native_adapter ('input', request.bridge_path);
            try
              input_protocol = input_bridge ('protocol');
            catch caught
              builtin('error', 'MathLab:InputProtocol', 'Input adapter is outdated or incompatible; rebuild it. %s', caught.message);
            end_try_catch
            if (! builtin('isstruct', input_protocol) || ! builtin('isfield', input_protocol, 'protocol') || ...
                ! builtin('ischar', input_protocol.protocol) || ! builtin('strcmp', input_protocol.protocol, 'mathlab-input') || ...
                ! builtin('isfield', input_protocol, 'version') || ! builtin('isnumeric', input_protocol.version) || ...
                ! builtin('isscalar', input_protocol.version) || input_protocol.version != 1)
              builtin('error', 'MathLab:InputProtocol', 'Input adapter protocol is unsupported; rebuild the adapter.');
            endif
            input_capabilities = {'original_builtin', 'caller_scope', 'separate_reply', 'genuine_reads'};
            for capability_index = 1:builtin('numel', input_capabilities)
              capability = input_capabilities{capability_index};
              if (! builtin('isfield', input_protocol, capability) || ! builtin('islogical', input_protocol.(capability)) || ...
                  ! builtin('isscalar', input_protocol.(capability)) || ! input_protocol.(capability))
                builtin('error', 'MathLab:InputProtocol', 'Input adapter lacks a required capability; rebuild the adapter.');
              endif
            endfor
            input_bridge ('install', [session_directory '/input'], response_prefix, generation, invocation, request_id);
            input_bridge_path = request.bridge_path;
            if (debugger_enabled)
              debug_bridge ('input_adapter', input_bridge);
            endif
            result = [];
          case 'enable_scope_bridge'
            scope_bridge = mathlab_native_adapter ('scope', request.bridge_path);
            try
              scope_protocol = scope_bridge ('protocol', 0);
            catch caught
              builtin('error', 'MathLab:ScopeProtocol', 'Scope adapter is outdated or incompatible; rebuild it. %s', caught.message);
            end_try_catch
            if (! builtin('isstruct', scope_protocol) || ! builtin('isfield', scope_protocol, 'protocol') || ...
                ! builtin('isfield', scope_protocol, 'version') || ! builtin('ischar', scope_protocol.protocol) || ...
                ! builtin('strcmp', scope_protocol.protocol, 'mathlab-scope') || ...
                ! builtin('isnumeric', scope_protocol.version) || ! builtin('isscalar', scope_protocol.version) || ...
                scope_protocol.version != 2 || ...
                ! builtin('isfield', scope_protocol, 'stable_registry') || ! builtin('islogical', scope_protocol.stable_registry) || ...
                ! builtin('isscalar', scope_protocol.stable_registry) || ! scope_protocol.stable_registry || ...
                ! builtin('isfield', scope_protocol, 'off_path_registry') || ! builtin('islogical', scope_protocol.off_path_registry) || ...
                ! builtin('isscalar', scope_protocol.off_path_registry) || ! scope_protocol.off_path_registry || ...
                ! builtin('isfield', scope_protocol, 'source_entry') || ! builtin('islogical', scope_protocol.source_entry) || ...
                ! builtin('isscalar', scope_protocol.source_entry) || ! scope_protocol.source_entry || ...
                ! builtin('isfield', scope_protocol, 'pre_entry') || ! builtin('islogical', scope_protocol.pre_entry) || ...
                ! builtin('isscalar', scope_protocol.pre_entry) || ! scope_protocol.pre_entry || ...
                ! builtin('isfield', scope_protocol, 'original_source_reflection') || ...
                ! builtin('islogical', scope_protocol.original_source_reflection) || ...
                ! builtin('isscalar', scope_protocol.original_source_reflection) || ! scope_protocol.original_source_reflection)
              builtin('error', 'MathLab:ScopeProtocol', 'Scope adapter protocol is unsupported; rebuild it.');
            endif
            scope_bridge ('registry', registry_path);
            if (mathlab_path_contains (registry_directory)); builtin('rmpath', registry_directory); endif
            scope_bridge_path = request.bridge_path;
            mathlab_scope_call ('__configuration__', {}, (1 == 1));
            result = [];
          case 'shutdown'
            mathlab_emit (builtin('struct', 'id', request_id, 'ok', (1 == 1), 'result', []), session_directory, response_prefix, generation, console_helper_path, invocation);
            response_sent = (1 == 1);
            worker_active = (1 == 0);
            break;
          otherwise
            builtin('error', 'MathLab:UnknownCommand', 'Unknown command: %s', request.command);
        endswitch
        unwind_protect_cleanup
          ## Exact local names only: never clear base values, committed history,
          ## revision, or range_edit_started (the outer catch still needs it).
          builtin('clear', '-variables', 'replacement', 'patch', 'candidate_history', ...
                  'edit_receipt', 'edit_request', 'edit_token', 'operation', ...
                  'binding_request', 'next_revision', 'next_context', 'next_token', 'context', 'ans', 'installed_value_bridge');
        end_unwind_protect
        endif
        mathlab_emit (builtin('struct', 'id', request_id, 'ok', (1 == 1), 'result', {result}), session_directory, response_prefix, generation, console_helper_path, invocation, 'E', result_is_ready);
        response_sent = (1 == 1);
      catch caught
        if (range_edit_started)
          ## Revocation/assignment or publication may already have happened.
          ## Retire history and force explicit client reconciliation; no replay.
          edit_history = mathlab_edit_history_invalidate (edit_history);
          caught.identifier = 'MathLab:ValueEditUncertain';
          caught.message = ['Edit outcome is uncertain; refresh values and explicitly reset history. ' caught.message];
        endif
        mathlab_emit (builtin('struct', 'id', request_id, 'ok', (1 == 0), 'error', mathlab_error (caught)), session_directory, response_prefix, generation, console_helper_path, invocation);
        response_sent = (1 == 1);
      end_try_catch
      unwind_protect_cleanup
        ## These contain only request/response/error data. The outer recovery
        ## path constructs its own response from preserved control state.
        builtin('clear', '-variables', 'request', 'request_line', 'result', 'caught', 'execution_error', 'result_is_ready');
      end_unwind_protect
    endwhile
  unwind_protect_cleanup
    ## Recovery is a separate commit record. A late normal End may already be
    ## in the common pipe; never overwrite its immutable metadata or emit End twice.
    if (worker_active)
      interrupted = mathlab_interrupted ();
      if (! response_sent && request_id != 0)
        if (execution_request)
          result = mathlab_interrupted_result (interrupted, started);
          response = builtin('struct', 'id', request_id, 'ok', (1 == 1), 'result', result);
        else
          response = builtin('struct', 'id', request_id, 'ok', (1 == 0), 'error', interrupted);
        endif
      else
        response = builtin('struct', 'id', 0, 'ok', (1 == 0), 'error', interrupted);
      endif
      mathlab_emit (response, session_directory, response_prefix, generation, console_helper_path, invocation, 'R');
    endif
  end_unwind_protect
endfunction

function result = mathlab_ready (worker_id, generation, invocation, revision, value_bridge = [])
  builtin = mathlab_intrinsics ();
  capabilities = {'binding_snapshot.v1', 'checked_scalar_write.v1', 'display_page.v1', 'typed_nd_page.v1', 'atomic_range_edit.v1'};
  if (builtin('is_function_handle', value_bridge))
    capabilities{end + 1} = 'dense_complex_native_components.v1';
  endif
  ## Retain exact native integers here. The explicit Ready producer encodes
  ## these three fields without passing them to older builtin JSON encoders.
  protocol = builtin('struct', 'name', 'mathlab-worker', 'version', builtin('uint32', 1), ...
                     'capabilities', {capabilities}, ...
                     'limits', builtin('struct', 'variable_cells', builtin('uint32', 2000), ...
                                       'response_bytes', builtin('uint32', 33554432)));
  result = builtin('struct', 'version', builtin('OCTAVE_VERSION'), 'working_directory', builtin('pwd'), 'protocol', protocol, ...
                   'context', mathlab_binding_context (worker_id, generation, invocation, revision));
  result.intrinsics = mathlab_intrinsic_status ();
endfunction

function context = mathlab_binding_context (worker_id, generation, invocation, revision)
  builtin = mathlab_intrinsics ();
  if (generation > builtin('flintmax') || invocation > builtin('flintmax'))
    builtin('error', 'MathLab:RevisionLimit', 'Worker generation/invocation cannot be represented exactly. Restart the session.');
  endif
  context = builtin('struct', 'worker_id', worker_id, 'generation', builtin('sprintf', '%.0f', generation), ...
                    'invocation', builtin('sprintf', '%.0f', invocation), 'revision', builtin('sprintf', '%u', revision));
endfunction

function next = mathlab_next_revision (revision)
  builtin = mathlab_intrinsics ();
  if (revision == builtin('intmax', 'uint64'))
    builtin('error', 'MathLab:RevisionLimit', 'Binding revision exhausted; restart the session.');
  endif
  next = revision + builtin('uint64', 1);
endfunction

function mathlab_check_binding (request, context)
  builtin = mathlab_intrinsics ();
  if (! builtin('isfield', request, 'expected') || ! builtin('isstruct', request.expected) || ...
      builtin('numel', request.expected) != 1 || ...
      ! builtin('isfield', request.expected, 'context') || ! mathlab_same_context (request.expected.context, context) || ...
      ! builtin('isfield', request.expected, 'name') || ! builtin('ischar', request.expected.name) || ...
      ! builtin('strcmp', request.expected.name, request.name))
    builtin('error', 'MathLab:StaleBinding', 'The workspace binding changed. Refresh this variable before reading or editing.');
  endif
  ## Existence and token checks happen in the same owned operation as lookup.
  mathlab_named_value (request.name);
endfunction

function same = mathlab_same_context (candidate, expected)
  builtin = mathlab_intrinsics ();
  same = (1 == 0);
  if (! builtin('isstruct', candidate) || builtin('numel', candidate) != 1 || ...
      builtin('numel', builtin('__fieldnames__', candidate)) != 4)
    return;
  endif
  names = {'worker_id', 'generation', 'invocation', 'revision'};
  for index = 1:4
    name = names{index};
    if (! builtin('isfield', candidate, name) || ! builtin('ischar', candidate.(name)) || ...
        ! builtin('strcmp', candidate.(name), expected.(name)))
      return;
    endif
  endfor
  same = (1 == 1);
endfunction

function result = mathlab_safe_workspace ()
  builtin = mathlab_intrinsics ();
  ## Metadata only: never evaluate properties, overloaded indexing or display.
  info = mathlab_metadata ();
  result = builtin('cell', 1, builtin('numel', info));
  for index = 1:builtin('numel', info)
    entry = info(index);
    result{index} = builtin('struct', 'name', entry.name, 'class_name', entry.class, ...
                           'dimensions', entry.size, 'bytes', entry.bytes, 'preview', '', ...
                           'dimensions_known', entry.dimensions_known, 'bytes_known', entry.bytes_known);
  endfor
endfunction

function info = mathlab_metadata ()
  builtin = mathlab_intrinsics ();
  ## A base variable named builtin/feval/whos must not become the dispatcher.
  ## A literal function handle ignores variable-call precedence in that scope.
  ## whos itself calls overloaded size on objects. Enumerate names first, and
  ## only request byte accounting after proving a primitive runtime class.
  names = mathlab_intrinsic_base_names ();
  info = builtin('struct', 'name', {}, 'class', {}, 'size', {}, 'bytes', {}, ...
                  'dimensions_known', {}, 'bytes_known', {});
  primitive = {'double','single','int8','uint8','int16','uint16','int32', ...
               'uint32','int64','uint64','logical','char'};
  containers = {'cell','struct','function_handle'};
  for index = 1:builtin('numel', names)
    name = names{index};
    value = mathlab_intrinsic_base_value (name);
    type = builtin('class', value);
    bytes_known = builtin('any', builtin('strcmp', type, primitive)) && ...
                  (builtin('isnumeric', value) || builtin('islogical', value) || builtin('ischar', value));
    dimensions_known = bytes_known || ...
                       (builtin('any', builtin('strcmp', type, containers)) && ...
                        (builtin('iscell', value) || builtin('isstruct', value) || builtin('is_function_handle', value)));
    dimensions = [];
    bytes = 0;
    if (dimensions_known)
      dimensions = builtin('size', value);
    endif
    if (bytes_known)
      selected = mathlab_intrinsic_base_metadata (name);
      bytes = selected.bytes;
    endif
    info(index) = builtin('struct', 'name', name, 'class', type, 'size', dimensions, 'bytes', bytes, ...
                           'dimensions_known', dimensions_known, 'bytes_known', bytes_known);
  endfor
endfunction

function result = mathlab_interrupted ()
  builtin = mathlab_intrinsics ();
  result = builtin('struct', 'message', 'Execution interrupted. Workspace preserved.', ...
                   'identifier', 'MathLab:Interrupted', 'stack', {{}});
endfunction

function result = mathlab_interrupted_result (interrupted, started)
  builtin = mathlab_intrinsics ();
  result = builtin('struct', 'output', '', 'error', interrupted, ...
                   'workspace', {mathlab_workspace()}, 'figures', {{}}, ...
                   'working_directory', builtin('pwd'), 'duration_ms', builtin('toc', started) * 1000, ...
                   'figure_warnings', {{}});
endfunction

function mathlab_require_function (name, expected_path)
  builtin = mathlab_intrinsics ();
  definition = builtin('__which__', name);
  if (! builtin('isstruct', definition) || builtin('numel', definition) != 1 ...
      || ! builtin('isfield', definition, 'file') || ! builtin('ischar', definition.file) ...
      || builtin('isempty', definition.file) ...
      || ! builtin('strcmp', builtin('canonicalize_file_name', definition.file), builtin('canonicalize_file_name', expected_path)))
    builtin('error', 'MathLab:RuntimeIdentity', 'Internal transport function %s is shadowed; expected %s.', name, expected_path);
  endif
endfunction

function present = mathlab_path_contains (entry)
  builtin = mathlab_intrinsics ();
  present = mathlab_intrinsic_path_contains (entry);
endfunction

function result = mathlab_debug_quit ()
  builtin = mathlab_intrinsics ();
  result = builtin('struct', 'message', 'Debug execution stopped. Workspace preserved.', ...
                   'identifier', 'MathLab:DebugQuit', 'stack', {{}});
endfunction

function mathlab_begin (request_id, nonce, generation, console_helper_path, invocation)
  builtin = mathlab_intrinsics ();
  mathlab_fence ('B', request_id, nonce, generation, console_helper_path, invocation);
endfunction

function [response, encoded] = mathlab_encode_response (response, is_ready = (1 == 0))
  builtin = mathlab_intrinsics ();
  ## Preflight is separate from physical publication so a startup producer
  ## failure can become one ordinary E0 diagnostic before any file is written.
  encoded = builtin('__unicode2native__', mathlab_ready_json(response, is_ready), 'UTF-8');
  if (builtin('numel', encoded) > 32 * 1024 * 1024)
    limit_error = builtin('struct', 'message', 'Worker metadata exceeds the 32 MiB response limit.', ...
                         'identifier', 'MathLab:ResponseLimit', 'stack', {{}});
    response = builtin('struct', 'id', response.id, 'ok', (1 == 0), 'error', limit_error);
    encoded = builtin('__unicode2native__', builtin('jsonencode', response), 'UTF-8');
  endif
endfunction

function mathlab_emit (response, session_directory, nonce, generation, console_helper_path, invocation, kind = 'E', is_ready = (1 == 0))
  builtin = mathlab_intrinsics ();
  [response, encoded] = mathlab_encode_response (response, is_ready);
  mathlab_publish (response, encoded, session_directory, nonce, generation, console_helper_path, invocation, kind);
endfunction

function mathlab_publish (response, encoded, session_directory, nonce, generation, console_helper_path, invocation, kind = 'E')
  builtin = mathlab_intrinsics ();
  ## Metadata is never multiplexed with console bytes: write a bounded temporary
  ## file, close it, rename atomically, then place End in the common output pipe.
  if (builtin('strcmp', kind, 'R')); basename = 'interruption'; else; basename = 'response'; endif
  target = [session_directory '/' builtin('sprintf', '%s-%.0f-%.0f.json', basename, invocation, response.id)];
  temporary = [target '.tmp'];
  stream = builtin('fopen', temporary, 'wb');
  if (stream < 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot create response metadata.'); endif
  closed = (1 == 0);
  unwind_protect
    count = builtin('fwrite', stream, encoded, 'uint8');
    if (count != builtin('numel', encoded)); builtin('error', 'MathLab:ResponseWrite', 'Incomplete response metadata write.'); endif
    status = builtin('fclose', stream);
    closed = (1 == 1);
    if (status != 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot close response metadata.'); endif
    [status, message] = builtin('rename', temporary, target);
    if (status != 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot publish response metadata: %s', message); endif
    mathlab_fence (kind, response.id, nonce, generation, console_helper_path, invocation);
  unwind_protect_cleanup
    if (! closed); builtin('fclose', stream); endif
  end_unwind_protect
endfunction

function mathlab_fence (kind, request_id, nonce, generation, console_helper_path, invocation)
  builtin = mathlab_intrinsics ();
  fence = builtin('sprintf', '%cMATHLAB:%s:%.0f:%s:%.0f:%.0f%c', 30, nonce, generation, kind, invocation, request_id, 31);
  ## Flush the language streams before the helper writes the entire short
  ## control record with a single OS-pipe write. popen keeps SIGINT enabled;
  ## system() temporarily ignores it and can silently lose an early Stop.
  ## Redirect child stdout to inherited stderr, which is the same common pipe.
  builtin('fflush', builtin('stdout'));
  builtin('fflush', builtin('stderr'));
  command = [mathlab_shell_quote(console_helper_path), ' ', mathlab_shell_quote(fence)];
  stream = builtin('popen', [command ' 1>&2'], 'r');
  status = builtin('pclose', stream);
  if (status != 0)
    builtin('error', 'MathLab:ConsoleFence', 'Console fence publication failed (status %d).', status);
  endif
endfunction

function quoted = mathlab_shell_quote (value)
  builtin = mathlab_intrinsics ();
  ## POSIX shell literal quoting, including session paths containing apostrophes.
  quoted = [builtin('char', 39), builtin('strrep', value, builtin('char', 39), builtin('char', [39 92 39 39])), builtin('char', 39)];
endfunction

function value = mathlab_named_value (name)
  builtin = mathlab_intrinsics ();
  if (! builtin('ischar', name) || ! builtin('isvarname', name) || builtin('numel', name) > 63)
    builtin('error', 'MathLab:VariableName', 'Expected a valid variable identifier.');
  endif
  names = mathlab_intrinsic_base_names ();
  if (! builtin('any', builtin('strcmp', names, name)))
    builtin('error', 'MathLab:VariableNotFound', 'Variable does not exist in the workspace: %s', name);
  endif
  ## Only a validated identifier is evaluated, never caller-supplied expressions.
  value = mathlab_intrinsic_base_value (name);
endfunction

function result = mathlab_read_variable (request, primitive_only = (1 == 0))
  builtin = mathlab_intrinsics ();
  value = mathlab_named_value (request.name);
  numeric_classes = {'double', 'single', 'int8', 'uint8', 'int16', 'uint16', ...
                     'int32', 'uint32', 'int64', 'uint64', 'logical'};
  numeric = builtin('any', builtin('strcmp', builtin('class', value), numeric_classes)) && ...
            (builtin('isnumeric', value) || builtin('islogical', value));
  if (primitive_only && ! numeric)
    builtin('error', 'MathLab:VariableType', 'Checked pages currently support primitive numeric/logical values only.');
  endif
  if (! numeric && ! builtin('ischar', value) && ! builtin('iscell', value) && ! builtin('isstruct', value))
    builtin('error', 'MathLab:VariableType', 'Variable Editor does not yet support %s values.', builtin('class', value));
  endif
  row_start = request.row_start;
  column_start = request.column_start;
  if (builtin('any', [row_start column_start request.rows request.columns] < 1) || ...
      builtin('any', builtin('fix', [row_start column_start request.rows request.columns]) != ...
           [row_start column_start request.rows request.columns]))
    builtin('error', 'MathLab:VariableIndex', 'Variable page starts and sizes must be positive integers.');
  endif
  if (request.rows * request.columns > 2000)
    builtin('error', 'MathLab:VariablePageLimit', 'Variable pages are limited to 2000 cells.');
  endif
  dimensions = builtin('size', value);
  if (row_start > builtin('max', 1, dimensions(1)) || column_start > builtin('max', 1, dimensions(2)))
    builtin('error', 'MathLab:VariableIndex', 'Variable page starts outside its dimensions.');
  endif
  rows = builtin('min', request.rows, builtin('max', 0, dimensions(1) - row_start + 1));
  columns = builtin('min', request.columns, builtin('max', 0, dimensions(2) - column_start + 1));
  cells = builtin('cell', 1, rows);
  subscripts = builtin('cell', 1, builtin('max', 2, builtin('ndims', value)));
  for axis = 1:builtin('numel', subscripts)
    subscripts{axis} = 1;
  endfor
  for row = 1:rows
    line = builtin('cell', 1, columns);
    subscripts{1} = row_start + row - 1;
    for column = 1:columns
      subscripts{2} = column_start + column - 1;
      element = value(subscripts{:});
      if (builtin('iscell', value))
        element = element{1};
      endif
      if (primitive_only)
        line{column} = mathlab_primitive_preview (element);
      else
        line{column} = mathlab_cell_preview (element);
      endif
    endfor
    cells{row} = line;
  endfor
  result = builtin('struct', 'name', request.name, 'class_name', builtin('class', value), ...
                   'dimensions', dimensions, 'row_start', row_start, ...
                   'column_start', column_start, 'rows', rows, 'columns', columns, ...
                   'cells', {cells}, 'editable', numeric && builtin('ndims', value) == 2);
endfunction

function text = mathlab_primitive_preview (value)
  builtin = mathlab_intrinsics ();
  type = builtin('class', value);
  if (builtin('strcmp', type, 'logical'))
    if (value); text = 'true'; else; text = 'false'; endif
  elseif (builtin('any', builtin('strcmp', type, {'uint8','uint16','uint32','uint64'})))
    text = builtin('sprintf', '%u', value);
  elseif (builtin('any', builtin('strcmp', type, {'int8','int16','int32','int64'})))
    text = builtin('sprintf', '%d', value);
  elseif (builtin('isreal', value))
    text = builtin('sprintf', '%.17g', value);
  else
    text = builtin('sprintf', '%.17g%+.17gi', builtin('real', value), builtin('imag', value));
  endif
endfunction

function text = mathlab_cell_preview (value)
  builtin = mathlab_intrinsics ();
  if (builtin('islogical', value) && builtin('isscalar', value))
    if (value)
      text = 'true';
    else
      text = 'false';
    endif
  elseif (builtin('isnumeric', value))
    if (builtin('isinteger', value) && builtin('isscalar', value))
      if (builtin('strncmp', builtin('class', value), 'uint', 4))
        text = builtin('sprintf', '%u', value);
      else
        text = builtin('sprintf', '%d', value);
      endif
      return;
    endif
    sample = value(1:builtin('min', builtin('numel', value), 6));
    text = mat2str (builtin('full', sample), 16);
    if (builtin('numel', value) > 6)
      text = [text ' ...'];
    endif
  elseif (builtin('ischar', value))
    text = ['''' value(1:builtin('min', builtin('numel', value), 120))(:).' ''''];
  elseif (builtin('isstruct', value))
    fields = fieldnames (value);
    text = ['struct: ' strjoin(fields(1:builtin('min', builtin('numel', fields), 8)).', ', ')];
  elseif (builtin('iscell', value))
    text = [mat2str(builtin('size', value)) ' cell'];
  else
    text = [mat2str(builtin('size', value)) ' ' builtin('class', value)];
  endif
  text = builtin('strrep', text, builtin('sprintf', '\n'), ' ');
  if (builtin('numel', text) > 240)
    text = [text(1:237) '...'];
  endif
endfunction

function value = mathlab_set_variable_element (request, commit = (1 == 1))
  builtin = mathlab_intrinsics ();
  value = mathlab_named_value (request.name);
  numeric_classes = {'double', 'single', 'int8', 'uint8', 'int16', 'uint16', ...
                     'int32', 'uint32', 'int64', 'uint64', 'logical'};
  original_class = builtin('class', value);
  if (! builtin('any', builtin('strcmp', original_class, numeric_classes)) || ...
      ! (builtin('isnumeric', value) || builtin('islogical', value)))
    builtin('error', 'MathLab:VariableReadOnly', 'Only two-dimensional numeric/logical matrices can be edited.');
  endif
  original_dimensions = builtin('size', value);
  if (builtin('ndims', value) != 2)
    builtin('error', 'MathLab:VariableReadOnly', 'Only two-dimensional numeric/logical matrices can be edited.');
  endif
  if (request.row < 1 || request.column < 1 || builtin('fix', request.row) != request.row || ...
      builtin('fix', request.column) != request.column || request.row > builtin('size', value,1) || ...
      request.column > builtin('size', value,2))
    builtin('error', 'MathLab:VariableIndex', 'Element index is outside the existing matrix dimensions.');
  endif
  ## Typed scalar components are independently checked. No user scalar is eval'd.
  pattern = '^[+-]?(([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?|NaN|Inf)$';
  parts = {request.scalar.real, request.scalar.imaginary};
  for index = 1:2
    if (! builtin('ischar', parts{index}) || builtin('numel', parts{index}) > 512 || ...
        builtin('isempty', builtin('regexp', parts{index}, pattern, 'once')))
      builtin('error', 'MathLab:VariableLiteral', 'Expected a scalar numeric literal.');
    endif
  endfor
  real_part = builtin('str2double', parts{1});
  imaginary_part = builtin('str2double', parts{2});
  if ((builtin('islogical', value) || builtin('isinteger', value)) && imaginary_part != 0)
    builtin('error', 'MathLab:VariableLiteral', 'Complex values cannot be assigned to an integer/logical matrix.');
  endif
  if (imaginary_part == 0)
    scalar = real_part;
  else
    scalar = builtin('complex', real_part, imaginary_part);
  endif
  if (builtin('isinteger', value) && builtin('ischar', request.scalar.integer))
    ## Exact decimal accumulation avoids rounding int64/uint64 values through
    ## double. Octave's saturating integer arithmetic matches element assignment.
    integer_text = request.scalar.integer;
    if (builtin('isempty', builtin('regexp', integer_text, '^[+-]?[0-9]+$', 'once')) || builtin('numel', integer_text) > 512)
      builtin('error', 'MathLab:VariableLiteral', 'Expected an integer literal.');
    endif
    negative = integer_text(1) == '-';
    if (integer_text(1) == '-' || integer_text(1) == '+')
      integer_text = integer_text(2:end);
    endif
    magnitude = builtin('uint64', 0);
    for digit = integer_text
      magnitude = magnitude * builtin('uint64', 10) + builtin('uint64', builtin('double', digit) - builtin('double', '0'));
    endfor
    if (negative)
      if (builtin('strncmp', original_class, 'uint', 4))
        scalar = builtin('uint64', 0);
      elseif (magnitude == 0)
        scalar = builtin('int64', 0);
      else
        scalar = -builtin('int64', magnitude - builtin('uint64', 1)) - builtin('int64', 1);
      endif
    else
      scalar = magnitude;
    endif
  endif
  value(request.row, request.column) = scalar;
  if (! builtin('strcmp', builtin('class', value), original_class) || ! mathlab_same_dimensions (value, original_dimensions))
    builtin('error', 'MathLab:VariableAssignment', 'Element edit would change the class or dimensions.');
  endif
  if (commit)
    builtin('assignin', 'base', request.name, value);
  endif
endfunction

function same = mathlab_same_dimensions (value, original)
  builtin = mathlab_intrinsics ();
  dimensions = builtin('size', value);
  same = builtin('numel', dimensions) == builtin('numel', original) && ...
         ! builtin('any', dimensions != original);
endfunction

function result = mathlab_workspace ()
  builtin = mathlab_intrinsics ();
  info = mathlab_metadata ();
  result = builtin('cell', 1, builtin('numel', info));
  for index = 1:builtin('numel', info)
    entry = info(index);
    preview = '';
    try
      value = mathlab_intrinsic_base_value (entry.name);
      if (builtin('isnumeric', value) || builtin('islogical', value))
        preview = '[';
        for element = 1:builtin('min', builtin('numel', value), 12)
          if (element > 1); preview = [preview ', ']; endif
          preview = [preview mathlab_primitive_preview(value(element))];
        endfor
        preview = [preview ']'];
        if (builtin('numel', value) > 12)
          preview = [preview ' ...'];
        endif
      elseif (builtin('ischar', value))
        preview = value(1:builtin('min', builtin('numel', value), 240));
        preview = preview(:).';
      elseif (builtin('is_function_handle', value))
        preview = builtin('func2str', value);
      elseif (builtin('isstruct', value))
        fields = builtin('__fieldnames__', value);
        fields_text = '';
        for field = 1:builtin('min', builtin('numel', fields), 12)
          if (field > 1); fields_text = [fields_text ', ']; endif
          fields_text = [fields_text fields{field}];
        endfor
        preview = ['fields: ' fields_text];
      elseif (builtin('iscell', value))
        preview = 'cell (read-only preview)';
      else
        preview = [entry.class ' object (shape/bytes unavailable)'];
      endif
      preview = builtin('strrep', preview, builtin('sprintf', '\n'), ' ');
      if (builtin('numel', preview) > 240)
        preview = [preview(1:237) '...'];
      endif
    catch
      preview = '[preview unavailable]';
    end_try_catch
    result{index} = builtin('struct', 'name', entry.name, 'class_name', entry.class, ...
                           'dimensions', entry.size, 'bytes', entry.bytes, 'preview', preview, ...
                           'dimensions_known', entry.dimensions_known, 'bytes_known', entry.bytes_known);
  endfor
endfunction

function result = mathlab_error (caught)
  builtin = mathlab_intrinsics ();
  frames = builtin('cell', 1, builtin('numel', caught.stack));
  for index = 1:builtin('numel', caught.stack)
    frame = caught.stack(index);
    frames{index} = builtin('struct', 'file', frame.file, 'name', frame.name, 'line', frame.line);
  endfor
  result = builtin('struct', 'message', caught.message, 'identifier', caught.identifier, 'stack', {frames});
endfunction

function [figures, messages] = mathlab_figures (session_directory, request_id)
  builtin = mathlab_intrinsics ();
  figures = {};
  messages = {};
  ## Enumerate native figure handles without findall's M-file traversal, which
  ## can invoke a project's shadowed size helper even when no figures exist.
  handles = builtin('__go_figure_handles__', (1 == 1));
  for index = 1:builtin('numel', handles)
    handle = handles(index);
    image_path = [session_directory '/' builtin('sprintf', 'figure-%d-%d.png', request_id, index)];
    try
      ## Export warnings are reported separately, so a renderer failure cannot erase results.
      ## Export callbacks and printer diagnostics share the same live console
      ## stream as foreground source. Only caught export failures are duplicated
      ## as structured figure_warnings; no extra captured output string exists.
      print (handle, image_path, '-dpng', '-r120');
      title = builtin('get', handle, 'name');
      if (builtin('isempty', title))
        title = builtin('sprintf', 'Figure %g', builtin('get', handle, 'number'));
      endif
      figures{end + 1} = builtin('struct', 'path', image_path, 'title', title);
    catch caught
      messages{end + 1} = builtin('sprintf', 'Figure export failed: %s', caught.message);
    end_try_catch
  endfor
endfunction

## SPDX-License-Identifier: GPL-3.0-or-later
## Private helper text appended to the owned worker source by Engine startup.
## It is not installed as a separately shadowable helper on the load path.
## The dispatch owner must check request.path.root against its current binding
## context immediately before calling this helper in the same ordinary operation.
function result = mathlab_value_page (request, value_bridge = [])
  builtin = mathlab_intrinsics ();
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
  row_start = mathlab_page_index (request.window.row_start, (1 == 0));
  column_start = mathlab_page_index (request.window.column_start, (1 == 0));
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
    index = mathlab_page_index (higher{axis - 2}, (1 == 0));
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

  ## Dense complex values MUST cross the native component route before any
  ## complex subsref, flattening or scalar narrowing can erase imaginary -0.
  if (complex_value && ! sparse_value)
    if (! builtin('is_function_handle', value_bridge))
      builtin('error', 'MathLab:ValuePagePrecisionUnavailable', ...
              'Dense complex pages require the verified native component adapter.');
    endif
    parts = value_bridge('read_components', value, subscripts);
    mathlab_value_pair_validate (parts, type, rows, columns, builtin('true'), 'MathLab:ValuePageInvariant');
    storage = 'dense';
    data = builtin('struct', 'kind', 'dense', ...
                   'real', {mathlab_page_components(parts.real, type)}, ...
                   'imag', {mathlab_page_components(parts.imag, type)});
  else
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
    index = mathlab_page_index (encoded{axis}, (1 == 0));
    if (index > dimensions(axis))
      builtin('error', 'MathLab:ValuePagePath', 'A container index is outside its dimensions.');
    endif
    indices{axis} = index;
  endfor
endfunction

function dimensions = mathlab_page_dimensions (value)
  builtin = mathlab_intrinsics ();
  rank = builtin('ndims', value);
  if (rank < 2 || rank > 32)
    builtin('error', 'MathLab:ValuePageUnsupported', 'Typed-page rank must be between 2 and 32.');
  endif
  dimensions = builtin('size', value);
  ## builtin('size') exposes doubles. Refuse the threshold itself as well as larger
  ## extents: a backend dimension of 2^53+1 could already round down to 2^53.
  if (builtin('any', dimensions < 0) || builtin('any', dimensions >= builtin('flintmax')) || ...
      builtin('any', ! builtin('isfinite', dimensions)) || ...
      builtin('any', builtin('fix', dimensions) != dimensions))
    builtin('error', 'MathLab:ValuePageUnsupported', 'A source dimension is not exactly representable by this backend.');
  endif
endfunction

function encoded = mathlab_page_index_strings (value)
  builtin = mathlab_intrinsics ();
  if (builtin('isempty', value))
    encoded = {};
  elseif (builtin('iscell', value) && builtin('numel', value) <= 32)
    encoded = value;
  else
    builtin('error', 'MathLab:ValuePageSelection', 'Expected at most 32 decimal-string indices.');
  endif
endfunction

function index = mathlab_page_index (text, allow_zero)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
  if (! builtin('isnumeric', value) || builtin('numel', value) != 1 || ...
      ! builtin('isreal', value) || ! builtin('isfinite', value) || ...
      value < 1 || value > 2000 || builtin('fix', value) != value)
    builtin('error', 'MathLab:ValuePageLimit', 'Page dimensions must be integers between 1 and 2000.');
  endif
  count = builtin('double', value);
endfunction

function valid = mathlab_page_identifier (name)
  builtin = mathlab_intrinsics ();
  valid = (1 == 0);
  if (! builtin('ischar', name) || builtin('isempty', name) || ...
      builtin('numel', name) > 63 || builtin('size', name, 1) != 1)
    return;
  endif
  letters = (name >= 'A' & name <= 'Z') | (name >= 'a' & name <= 'z') | name == '_';
  digits = name >= '0' & name <= '9';
  valid = letters(1) && ! builtin('any', ! (letters | digits));
endfunction

function strings = mathlab_page_components (values, type)
  builtin = mathlab_intrinsics ();
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

## Shared private validation of the adapter's selected-only component pair.
## Reads may be empty. Edit/history callers pass allow_empty=false and their
## admitted exact rows/columns. Never derive the patch count from struct numel.
function count = mathlab_value_pair_validate (pair, type, rows, columns, allow_empty, error_id)
  builtin = mathlab_intrinsics ();
  if (! builtin('ischar', type) || ! builtin('any', builtin('strcmp', type, {'double','single'})) || ...
      ! builtin('isnumeric', rows) || ! builtin('isnumeric', columns) || ...
      ! builtin('isreal', rows) || ! builtin('isreal', columns) || ...
      builtin('numel', rows) != 1 || builtin('numel', columns) != 1 || ...
      ! builtin('isfinite', rows) || ! builtin('isfinite', columns) || ...
      rows < 0 || columns < 0 || rows > 2000 || columns > 2000 || ...
      builtin('fix', rows) != rows || builtin('fix', columns) != columns || ...
      ! builtin('islogical', allow_empty) || builtin('numel', allow_empty) != 1)
    builtin('error', error_id, 'Invalid bounded component-pair descriptor.');
  endif
  count = rows * columns;
  if (count > 2000 || (! allow_empty && count == 0))
    builtin('error', error_id, 'Selected component pairs contain at most 2000 cells; edit/history pairs must be nonempty.');
  endif
  if (! builtin('isstruct', pair) || ! builtin('strcmp', builtin('class', pair), 'struct') || ...
      builtin('numel', pair) != 1 || builtin('numfields', pair) != 2 || ...
      ! builtin('isfield', pair, 'real') || ! builtin('isfield', pair, 'imag'))
    builtin('error', error_id, 'Expected exactly one genuine real/imag component-pair struct.');
  endif
  components = {pair.real, pair.imag};
  for index = 1:2
    value = components{index};
    ## Primitive class/type checks precede dimensions and indexing. No object
    ## getter, conversion, sparse densification or complex constructor is used.
    if (! builtin('strcmp', builtin('class', value), type) || ! builtin('isnumeric', value) || ...
        ! builtin('isreal', value) || builtin('issparse', value))
      builtin('error', error_id, 'Component pairs require genuine dense real arrays of the leaf precision.');
    endif
    if (builtin('ndims', value) != 2 || builtin('size', value, 1) != rows || ...
        builtin('size', value, 2) != columns || builtin('numel', value) != count)
      builtin('error', error_id, 'A native component pair disagrees with the exact selected shape.');
    endif
  endfor
endfunction

## Startup/recovery only. Engine supplies an explicitly trusted matching .oct;
## no project/load-path discovery and no function-cache clearing is performed.
function bridge = mathlab_value_bridge_open (path)
  builtin = mathlab_intrinsics ();
  bridge = [];
  if (builtin('ischar', path) && builtin('isempty', path)); return; endif
  bridge = mathlab_validate_value_bridge (path);
endfunction

## Native admission establishes identity; this helper checks the same retained
## callback and exact admitted path before every checked operation. No name lookup.
function bridge = mathlab_validate_value_bridge (path)
  builtin = mathlab_intrinsics ();
  bridge = mathlab_native_adapter ('value', path);
  try
    protocol = bridge('protocol');
  catch caught
    builtin('error', 'MathLab:ValuePagePrecisionProtocol', 'Cannot load the configured component adapter: %s', caught.message);
  end_try_catch
  if (! builtin('isstruct', protocol) || ! builtin('strcmp', builtin('class', protocol), 'struct') || builtin('numel', protocol) != 1 || ...
      ! builtin('isfield', protocol, 'protocol') || ! builtin('ischar', protocol.protocol) || builtin('numel', protocol.protocol) > 32 || ...
      ! builtin('strcmp', protocol.protocol, 'mathlab-value') || ...
      ! builtin('isfield', protocol, 'version') || ! mathlab_value_bridge_constant(protocol.version, 1) || ...
      ! builtin('isfield', protocol, 'dense_complex_components') || ! builtin('islogical', protocol.dense_complex_components) || ...
      builtin('numel', protocol.dense_complex_components) != 1 || ! protocol.dense_complex_components || ...
      ! builtin('isfield', protocol, 'max_cells') || ! mathlab_value_bridge_constant(protocol.max_cells, 2000) || ...
      ! builtin('isfield', protocol, 'max_rank') || ! mathlab_value_bridge_constant(protocol.max_rank, 32) || ...
      ! builtin('isfield', protocol, 'max_replace_leaf_bytes') || ! mathlab_value_bridge_constant(protocol.max_replace_leaf_bytes, 134217728))
    builtin('error', 'MathLab:ValuePagePrecisionProtocol', 'The configured component adapter has an unsupported protocol or limit profile.');
  endif
endfunction

function valid = mathlab_value_bridge_constant (value, expected)
  builtin = mathlab_intrinsics ();
  valid = builtin('isnumeric', value) && builtin('isreal', value) && builtin('numel', value) == 1 && ...
          builtin('isfinite', value) && value == expected;
endfunction

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
  builtin = mathlab_intrinsics ();
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
    ## nonzeros; no full-source builtin('full')/dense conversion is requested anywhere.
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
    value = mathlab_intrinsic_integer (type, signed_value);
  else
    value = mathlab_intrinsic_integer (type, magnitude);
  endif
endfunction

function greater = mathlab_edit_decimal_greater (digits, maximum)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
  if (complex_value && builtin('isreal', value))
    ## The selected backend must qualify that genuine complex(sparse) remains
    ## sparse. A signature failure rejects the private candidate, never base.
    value = builtin('complex', value);
  endif
endfunction

function mathlab_edit_signature (value, type, sparse_value, complex_value, dimensions)
  builtin = mathlab_intrinsics ();
  if (! builtin('strcmp', builtin('class', value), type) || ...
      builtin('issparse', value) != sparse_value || (! builtin('isreal', value)) != complex_value || ...
      ! mathlab_edit_same_dimensions (value, dimensions))
    builtin('error', 'MathLab:ValueEditInvariant', 'Edit construction changed class, shape, storage or complex metadata.');
  endif
endfunction

function same = mathlab_edit_same_dimensions (value, dimensions)
  builtin = mathlab_intrinsics ();
  actual = builtin('size', value);
  same = builtin('numel', actual) == builtin('numel', dimensions) && ...
         ! builtin('any', actual != dimensions);
endfunction

function patch = mathlab_edit_patch (value, type, sparse_value, complex_value, rows, columns)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
  if (! builtin('isempty', state.context) && ! mathlab_same_context (state.context, context))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'An intervening operation invalidated this history.');
  endif
endfunction

function token = mathlab_edit_history_next_token (before, token)
  builtin = mathlab_intrinsics ();
  token = mathlab_page_token (token);
  expected_revision = mathlab_next_revision (mathlab_edit_integer(before.context.revision, 'uint64'));
  expected = before;
  expected.context.revision = builtin('sprintf', '%u', expected_revision);
  if (! builtin('strcmp', token.name, expected.name) || ! mathlab_same_context (token.context, expected.context))
    builtin('error', 'MathLab:ValueEditHistoryConflict', 'An accepted transaction must advance the exact binding revision once.');
  endif
endfunction

function charge = mathlab_edit_history_charge (records)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
  if (builtin('isempty', records))
    ## Octave jsonencode represents NaN as JSON null (ConvertInfAndNaN=true).
    ## [] would be an array and is not an optional HistoryHead on the wire.
    head = builtin('NaN');
  else
    record = records{end};
    head = builtin('struct', 'transaction_id', builtin('sprintf', '%u', record.id), 'root_name', record.name);
  endif
endfunction

function receipt = mathlab_edit_history_receipt (state, operation, id, token, evicted)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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

## SPDX-License-Identifier: GPL-3.0-or-later
## Private helper appended to the owned worker, not installed on its load path.
## Octave 8.4 can reject native integer values in jsonencode, or spell a double
## limit with a fractional suffix. Only readiness's three unsigned wire fields
## are encoded here after an explicit owner flag. Ordinary values, including
## user data shaped exactly like Ready, retain the builtin encoder's behavior.
function text = mathlab_ready_json (response, is_ready = (1 == 0))
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
  valid = builtin('isstruct', value) && builtin('strcmp', builtin('class', value), 'struct') && ...
          ! builtin('isobject', value) && builtin('numel', value) == 1;
endfunction

## This is only scalar-object assembly at the four owned readiness levels.
## Field names, capabilities, context, extra fields and arbitrary string values
## are escaped/encoded by the genuine builtin. No JSON text is searched/replaced.
function text = mathlab_ready_json_object (value, names, encoded)
  builtin = mathlab_intrinsics ();
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
  builtin = mathlab_intrinsics ();
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
      || builtin('numfields', selected) != 4 ...
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

function result = mathlab_intrinsic_status ()
  [builtin, bridge] = mathlab_intrinsics ();
  result = bridge('intrinsic_status');
endfunction

function present = mathlab_intrinsic_path_contains (entry)
  [builtin, bridge] = mathlab_intrinsics ();
  present = bridge('intrinsic_path_contains', entry);
endfunction

## Engine-generated factories use absolute Unix .m paths; no path M-file lookup.
function name = mathlab_intrinsic_factory_name (file)
  [builtin, bridge] = mathlab_intrinsics ();
  if (! builtin('ischar', file) || builtin('rows', file) != 1)
    builtin('error', 'MathLab:RuntimeIdentity', 'Expected a generated factory path.');
  endif
  selected = builtin('regexp', file, '^/.*[/]([^/]+)\.m$', 'tokens', 'once');
  if (builtin('isempty', selected) || ! builtin('isvarname', selected{1}))
    builtin('error', 'MathLab:RuntimeIdentity', 'Expected a generated factory .m filename.');
  endif
  name = selected{1};
endfunction

function output = mathlab_intrinsic_integer (type, value)
  [builtin, bridge] = mathlab_intrinsics ();
  switch (type)
    case 'int8'; output = builtin('int8', value);
    case 'uint8'; output = builtin('uint8', value);
    case 'int16'; output = builtin('int16', value);
    case 'uint16'; output = builtin('uint16', value);
    case 'int32'; output = builtin('int32', value);
    case 'uint32'; output = builtin('uint32', value);
    case 'int64'; output = builtin('int64', value);
    case 'uint64'; output = builtin('uint64', value);
    otherwise
      builtin('error', 'MathLab:ValueEditPayload', 'Expected a fixed integer class.');
  endswitch
endfunction
