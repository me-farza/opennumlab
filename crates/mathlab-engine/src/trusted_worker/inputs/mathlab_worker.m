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
  registry_directory = fullfile (session_directory, 'registry');
  registry_path = fullfile (registry_directory, 'mathlab_scope_register.m');
  if (builtin('isempty', console_helper_path) || ! builtin('exist', console_helper_path, 'file'))
    builtin('error', 'MathLab:ConsoleHelper', 'The session console fence helper is unavailable.');
  endif
  if (announce_ready)
    page_screen_output (false);
    page_output_immediately (true);
    toolkits = available_graphics_toolkits ();
    if (builtin('any', builtin('strcmp', toolkits, 'gnuplot')))
      ## On native macOS Octave, the first figure initializes the graphics manager
      ## and can reset an earlier default-toolkit choice to FLTK. Warm that manager
      ## with an explicitly selected invisible figure before choosing our default.
      evalc ("graphics_toolkit ('gnuplot'); bootstrap_figure = figure ('visible', 'off', '__graphics_toolkit__', 'gnuplot'); close (bootstrap_figure); graphics_toolkit ('gnuplot');");
    endif
    set (0, 'defaultfigurevisible', 'off');
  endif
  ## Retained code is ready before project entry; no path-based adapter load.
  if (announce_ready && ! builtin('isempty', initial_directory))
    builtin('cd', initial_directory);
  endif
  request_path = fullfile (session_directory, 'request.json');
  worker_active = true;
  response_sent = true;
  request_id = 0;
  execution_request = false;
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
      startup_response = builtin('struct', 'id', 0, 'ok', true, 'result', ...
                                 mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge));
      [startup_response, startup_encoded] = mathlab_encode_response (startup_response, (1 == 1));
    catch caught
      ## Encoding is part of readiness validation. Do not replace a producer
      ## failure with Interrupted/R0 and recursively re-enter the same failure.
      ## Disable recovery even if encoding the diagnostic itself then fails.
      worker_active = false;
      startup_response = builtin('struct', 'id', 0, 'ok', false, 'error', mathlab_error(caught));
      [startup_response, startup_encoded] = mathlab_encode_response (startup_response, (1 == 0));
    end_try_catch
    if (! startup_response.ok); worker_active = false; endif
    ## Publish once, outside the validation catch: an I/O/fence failure must
    ## never overwrite metadata that may already have been committed.
    mathlab_publish (startup_response, startup_encoded, session_directory, response_prefix, ...
                     generation, console_helper_path, invocation);
    response_sent = true;
    builtin('clear', '-variables', 'startup_response', 'startup_encoded', 'caught');
    if (! worker_active); return; endif
    if (debugger_enabled && ! builtin('isempty', input_bridge_path))
      input_bridge = mathlab_native_adapter ('input', input_bridge_path);
      debug_bridge ('input_adapter', input_bridge);
    endif
    while (true)
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
      range_edit_started = false;
      result_is_ready = (1 == 0);
      ## Request metadata and error structs are not durable session state.
      ## This outer guard also runs if publication or SIGINT leaves the try.
      unwind_protect
      try
        request = builtin('jsondecode', request_line);
        request_id = request.id;
        response_sent = false;
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
            execution_request = true;
            execution_error = [];
            captured = '';
            retained_handles = {};
            started = builtin('tic');
            if (! builtin('isempty', scope_bridge_path))
              ## User clear functions/all may reset the unlocked numerical
              ## registry. Rebind it without retaining its user-function roots.
              scope_bridge = mathlab_native_adapter ('scope', scope_bridge_path);
              scope_bridge ('registry', registry_path);
              mathlab_scope_call ('__configuration__', {}, true);
            endif
            if (debugger_enabled)
              debug_bridge ('registry', registry_path);
              ## The debugger marks the exact retained local-function objects
              ## after each script's scope registration and optional native bind.
              mathlab_scope_call ('__debug_configuration__', {}, true);
              ## Reconcile user dbclear commands from prior requests before a
              ## direct retained-handle call can encounter stale AST marks.
              debug_bridge ('synchronize', 0);
            endif
            if (builtin('strcmp', request.command, 'run_file'))
              source_expression = "source (request.path, 'base');";
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
                  addpath (injected_path, '-end');
                endif
                unwind_protect
                  ## Restore configuration after adding the capsule, and retain
                  ## exact handles in transport scope before any user statement.
                  ## Native command-line registration keeps the helper stable
                  ## across Octave's path-cache refreshes.
                  try
                    if (! builtin('isempty', scope_bridge_path))
                      mathlab_scope_call ('__configuration__', {}, true);
                    endif
                    if (debugger_enabled)
                      mathlab_scope_call ('__debug_configuration__', {}, true);
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
                          [~, factory_name] = fileparts (request.factory_path);
                          mathlab_require_function (factory_name, request.factory_path);
                          retained_handles = feval (factory_name);
                        endif
                      endif
                      mathlab_scope_call (request.path, retained_handles, request.original_path, request.original_source);
                    endif
                  catch caught
                    execution_error = mathlab_error (caught);
                  end_try_catch
                  if (builtin('isempty', execution_error))
                    try
                      eval (source_expression);
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
                    rmpath (injected_path);
                  endif
                end_unwind_protect
              else
                try
                  eval (source_expression);
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
                  evalin ('base', code);
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
            result = builtin('struct', 'token', request.expected, 'slice', mathlab_read_variable (request, true));
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
            replacement = mathlab_set_variable_element (request, false);
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
            range_edit_started = true;
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
            cd (request.path);
            result = builtin('struct', 'working_directory', builtin('pwd'));
          case 'clear'
            evalin ('base', 'clear -variables');
            result = [];
          case 'list_packages'
            installed = pkg ('list');
            result = builtin('cell', 1, builtin('numel', installed));
            for package_index = 1:builtin('numel', installed)
              package = installed{package_index};
              result{package_index} = builtin('struct', 'name', package.name, 'version', package.version, ...
                                             'description', package.description, 'loaded', logical (package.loaded));
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
                ! builtin('isnumeric', bridge_protocol.version) || ! isscalar (bridge_protocol.version) || ...
                bridge_protocol.version != 4 || ...
                ! builtin('isfield', bridge_protocol, 'streamed_evaluation') || ! builtin('islogical', bridge_protocol.streamed_evaluation) || ...
                ! isscalar (bridge_protocol.streamed_evaluation) || ! bridge_protocol.streamed_evaluation || ...
                ! builtin('isfield', bridge_protocol, 'input_wait_cancel') || ! builtin('islogical', bridge_protocol.input_wait_cancel) || ...
                ! isscalar (bridge_protocol.input_wait_cancel) || ! bridge_protocol.input_wait_cancel || ...
                ! builtin('isfield', bridge_protocol, 'stable_registry') || ! builtin('islogical', bridge_protocol.stable_registry) || ...
                ! isscalar (bridge_protocol.stable_registry) || ! bridge_protocol.stable_registry || ...
                ! builtin('isfield', bridge_protocol, 'off_path_registry') || ! builtin('islogical', bridge_protocol.off_path_registry) || ...
                ! isscalar (bridge_protocol.off_path_registry) || ! bridge_protocol.off_path_registry || ...
                ! builtin('isfield', bridge_protocol, 'source_entry') || ! builtin('islogical', bridge_protocol.source_entry) || ...
                ! isscalar (bridge_protocol.source_entry) || ! bridge_protocol.source_entry || ...
                ! builtin('isfield', bridge_protocol, 'breakpoint_source_metadata') || ...
                ! builtin('islogical', bridge_protocol.breakpoint_source_metadata) || ...
                ! isscalar (bridge_protocol.breakpoint_source_metadata) || ! bridge_protocol.breakpoint_source_metadata)
              builtin('error', 'MathLab:DebuggerProtocol', 'Debugger adapter protocol is unsupported; rebuild the adapter.');
            endif
            debug_bridge (request.debug_directory);
            debug_bridge ('registry', registry_path);
            if (mathlab_path_contains (registry_directory)); rmpath (registry_directory); endif
            debugger_enabled = true;
            if (! builtin('isempty', input_bridge_path))
              input_bridge = mathlab_native_adapter ('input', input_bridge_path);
              debug_bridge ('input_adapter', input_bridge);
            endif
            mathlab_scope_call ('__debug_configuration__', {}, true);
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
                ! isscalar (input_protocol.version) || input_protocol.version != 1)
              builtin('error', 'MathLab:InputProtocol', 'Input adapter protocol is unsupported; rebuild the adapter.');
            endif
            input_capabilities = {'original_builtin', 'caller_scope', 'separate_reply', 'genuine_reads'};
            for capability_index = 1:builtin('numel', input_capabilities)
              capability = input_capabilities{capability_index};
              if (! builtin('isfield', input_protocol, capability) || ! builtin('islogical', input_protocol.(capability)) || ...
                  ! isscalar (input_protocol.(capability)) || ! input_protocol.(capability))
                builtin('error', 'MathLab:InputProtocol', 'Input adapter lacks a required capability; rebuild the adapter.');
              endif
            endfor
            input_bridge ('install', fullfile(session_directory, 'input'), response_prefix, generation, invocation, request_id);
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
                ! builtin('isnumeric', scope_protocol.version) || ! isscalar (scope_protocol.version) || ...
                scope_protocol.version != 2 || ...
                ! builtin('isfield', scope_protocol, 'stable_registry') || ! builtin('islogical', scope_protocol.stable_registry) || ...
                ! isscalar (scope_protocol.stable_registry) || ! scope_protocol.stable_registry || ...
                ! builtin('isfield', scope_protocol, 'off_path_registry') || ! builtin('islogical', scope_protocol.off_path_registry) || ...
                ! isscalar (scope_protocol.off_path_registry) || ! scope_protocol.off_path_registry || ...
                ! builtin('isfield', scope_protocol, 'source_entry') || ! builtin('islogical', scope_protocol.source_entry) || ...
                ! isscalar (scope_protocol.source_entry) || ! scope_protocol.source_entry || ...
                ! builtin('isfield', scope_protocol, 'pre_entry') || ! builtin('islogical', scope_protocol.pre_entry) || ...
                ! isscalar (scope_protocol.pre_entry) || ! scope_protocol.pre_entry || ...
                ! builtin('isfield', scope_protocol, 'original_source_reflection') || ...
                ! builtin('islogical', scope_protocol.original_source_reflection) || ...
                ! isscalar (scope_protocol.original_source_reflection) || ! scope_protocol.original_source_reflection)
              builtin('error', 'MathLab:ScopeProtocol', 'Scope adapter protocol is unsupported; rebuild it.');
            endif
            scope_bridge ('registry', registry_path);
            if (mathlab_path_contains (registry_directory)); rmpath (registry_directory); endif
            scope_bridge_path = request.bridge_path;
            mathlab_scope_call ('__configuration__', {}, true);
            result = [];
          case 'shutdown'
            mathlab_emit (builtin('struct', 'id', request_id, 'ok', true, 'result', []), session_directory, response_prefix, generation, console_helper_path, invocation);
            response_sent = true;
            worker_active = false;
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
        mathlab_emit (builtin('struct', 'id', request_id, 'ok', true, 'result', {result}), session_directory, response_prefix, generation, console_helper_path, invocation, 'E', result_is_ready);
        response_sent = true;
      catch caught
        if (range_edit_started)
          ## Revocation/assignment or publication may already have happened.
          ## Retire history and force explicit client reconciliation; no replay.
          edit_history = mathlab_edit_history_invalidate (edit_history);
          caught.identifier = 'MathLab:ValueEditUncertain';
          caught.message = ['Edit outcome is uncertain; refresh values and explicitly reset history. ' caught.message];
        endif
        mathlab_emit (builtin('struct', 'id', request_id, 'ok', false, 'error', mathlab_error (caught)), session_directory, response_prefix, generation, console_helper_path, invocation);
        response_sent = true;
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
          response = builtin('struct', 'id', request_id, 'ok', true, 'result', result);
        else
          response = builtin('struct', 'id', request_id, 'ok', false, 'error', interrupted);
        endif
      else
        response = builtin('struct', 'id', 0, 'ok', false, 'error', interrupted);
      endif
      mathlab_emit (response, session_directory, response_prefix, generation, console_helper_path, invocation, 'R');
    endif
  end_unwind_protect
endfunction

function result = mathlab_ready (worker_id, generation, invocation, revision, value_bridge = [])
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
endfunction

function context = mathlab_binding_context (worker_id, generation, invocation, revision)
  if (generation > builtin('flintmax') || invocation > builtin('flintmax'))
    builtin('error', 'MathLab:RevisionLimit', 'Worker generation/invocation cannot be represented exactly. Restart the session.');
  endif
  context = builtin('struct', 'worker_id', worker_id, 'generation', builtin('sprintf', '%.0f', generation), ...
                    'invocation', builtin('sprintf', '%.0f', invocation), 'revision', builtin('sprintf', '%u', revision));
endfunction

function next = mathlab_next_revision (revision)
  if (revision == builtin('intmax', 'uint64'))
    builtin('error', 'MathLab:RevisionLimit', 'Binding revision exhausted; restart the session.');
  endif
  next = revision + builtin('uint64', 1);
endfunction

function mathlab_check_binding (request, context)
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
  same = false;
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
  same = true;
endfunction

function result = mathlab_safe_workspace ()
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
  ## A base variable named builtin/feval/whos must not become the dispatcher.
  ## A literal function handle ignores variable-call precedence in that scope.
  ## whos itself calls overloaded size on objects. Enumerate names first, and
  ## only request byte accounting after proving a primitive runtime class.
  names = builtin('evalin', 'base', "(@builtin) ('who')");
  info = builtin('struct', 'name', {}, 'class', {}, 'size', {}, 'bytes', {}, ...
                  'dimensions_known', {}, 'bytes_known', {});
  primitive = {'double','single','int8','uint8','int16','uint16','int32', ...
               'uint32','int64','uint64','logical','char'};
  containers = {'cell','struct','function_handle'};
  for index = 1:builtin('numel', names)
    name = names{index};
    value = builtin('evalin', 'base', name);
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
      selected = builtin('evalin', 'base', builtin('sprintf', "(@builtin) ('whos', '%s')", name));
      bytes = selected.bytes;
    endif
    info(index) = builtin('struct', 'name', name, 'class', type, 'size', dimensions, 'bytes', bytes, ...
                           'dimensions_known', dimensions_known, 'bytes_known', bytes_known);
  endfor
endfunction

function result = mathlab_interrupted ()
  result = builtin('struct', 'message', 'Execution interrupted. Workspace preserved.', ...
                   'identifier', 'MathLab:Interrupted', 'stack', {{}});
endfunction

function result = mathlab_interrupted_result (interrupted, started)
  result = builtin('struct', 'output', '', 'error', interrupted, ...
                   'workspace', {mathlab_workspace()}, 'figures', {{}}, ...
                   'working_directory', builtin('pwd'), 'duration_ms', builtin('toc', started) * 1000, ...
                   'figure_warnings', {{}});
endfunction

function mathlab_require_function (name, expected_path)
  actual_path = which (name);
  if (builtin('isempty', actual_path) || ! builtin('strcmp', canonicalize_file_name (actual_path), canonicalize_file_name (expected_path)))
    builtin('error', 'MathLab:RuntimeIdentity', 'Internal transport function %s is shadowed; expected %s.', name, expected_path);
  endif
endfunction

function present = mathlab_path_contains (entry)
  present = builtin('any', builtin('strcmp', strsplit (path (), pathsep ()), entry));
endfunction

function result = mathlab_debug_quit ()
  result = builtin('struct', 'message', 'Debug execution stopped. Workspace preserved.', ...
                   'identifier', 'MathLab:DebugQuit', 'stack', {{}});
endfunction

function mathlab_begin (request_id, nonce, generation, console_helper_path, invocation)
  mathlab_fence ('B', request_id, nonce, generation, console_helper_path, invocation);
endfunction

function [response, encoded] = mathlab_encode_response (response, is_ready = (1 == 0))
  ## Preflight is separate from physical publication so a startup producer
  ## failure can become one ordinary E0 diagnostic before any file is written.
  encoded = builtin('__unicode2native__', mathlab_ready_json(response, is_ready), 'UTF-8');
  if (builtin('numel', encoded) > 32 * 1024 * 1024)
    limit_error = builtin('struct', 'message', 'Worker metadata exceeds the 32 MiB response limit.', ...
                         'identifier', 'MathLab:ResponseLimit', 'stack', {{}});
    response = builtin('struct', 'id', response.id, 'ok', false, 'error', limit_error);
    encoded = builtin('__unicode2native__', builtin('jsonencode', response), 'UTF-8');
  endif
endfunction

function mathlab_emit (response, session_directory, nonce, generation, console_helper_path, invocation, kind = 'E', is_ready = (1 == 0))
  [response, encoded] = mathlab_encode_response (response, is_ready);
  mathlab_publish (response, encoded, session_directory, nonce, generation, console_helper_path, invocation, kind);
endfunction

function mathlab_publish (response, encoded, session_directory, nonce, generation, console_helper_path, invocation, kind = 'E')
  ## Metadata is never multiplexed with console bytes: write a bounded temporary
  ## file, close it, rename atomically, then place End in the common output pipe.
  if (builtin('strcmp', kind, 'R')); basename = 'interruption'; else; basename = 'response'; endif
  target = [session_directory '/' builtin('sprintf', '%s-%.0f-%.0f.json', basename, invocation, response.id)];
  temporary = [target '.tmp'];
  stream = builtin('fopen', temporary, 'wb');
  if (stream < 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot create response metadata.'); endif
  closed = false;
  unwind_protect
    count = builtin('fwrite', stream, encoded, 'uint8');
    if (count != builtin('numel', encoded)); builtin('error', 'MathLab:ResponseWrite', 'Incomplete response metadata write.'); endif
    status = builtin('fclose', stream);
    closed = true;
    if (status != 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot close response metadata.'); endif
    [status, message] = builtin('rename', temporary, target);
    if (status != 0); builtin('error', 'MathLab:ResponseWrite', 'Cannot publish response metadata: %s', message); endif
    mathlab_fence (kind, response.id, nonce, generation, console_helper_path, invocation);
  unwind_protect_cleanup
    if (! closed); builtin('fclose', stream); endif
  end_unwind_protect
endfunction

function mathlab_fence (kind, request_id, nonce, generation, console_helper_path, invocation)
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
  ## POSIX shell literal quoting, including session paths containing apostrophes.
  quoted = [builtin('char', 39), builtin('strrep', value, builtin('char', 39), builtin('char', [39 92 39 39])), builtin('char', 39)];
endfunction

function value = mathlab_named_value (name)
  if (! builtin('ischar', name) || ! builtin('isvarname', name) || builtin('numel', name) > 63)
    builtin('error', 'MathLab:VariableName', 'Expected a valid variable identifier.');
  endif
  names = builtin('evalin', 'base', "(@builtin) ('who')");
  if (! builtin('any', builtin('strcmp', names, name)))
    builtin('error', 'MathLab:VariableNotFound', 'Variable does not exist in the workspace: %s', name);
  endif
  ## Only a validated identifier is evaluated, never caller-supplied expressions.
  value = builtin('evalin', 'base', name);
endfunction

function result = mathlab_read_variable (request, primitive_only = false)
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
  if (islogical (value) && isscalar (value))
    if (value)
      text = 'true';
    else
      text = 'false';
    endif
  elseif (isnumeric (value))
    if (isinteger (value) && isscalar (value))
      if (strncmp (class(value), 'uint', 4))
        text = sprintf ('%u', value);
      else
        text = sprintf ('%d', value);
      endif
      return;
    endif
    sample = value(1:min (numel (value), 6));
    text = mat2str (full (sample), 16);
    if (numel (value) > 6)
      text = [text ' ...'];
    endif
  elseif (ischar (value))
    text = ['''' value(1:min (numel (value), 120))(:).' ''''];
  elseif (isstruct (value))
    fields = fieldnames (value);
    text = ['struct: ' strjoin(fields(1:min(numel(fields), 8)).', ', ')];
  elseif (iscell (value))
    text = [mat2str(size(value)) ' cell'];
  else
    text = [mat2str(size(value)) ' ' class(value)];
  endif
  text = strrep (text, sprintf ('\n'), ' ');
  if (numel (text) > 240)
    text = [text(1:237) '...'];
  endif
endfunction

function value = mathlab_set_variable_element (request, commit = true)
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
    assignin ('base', request.name, value);
  endif
endfunction

function same = mathlab_same_dimensions (value, original)
  dimensions = builtin('size', value);
  same = builtin('numel', dimensions) == builtin('numel', original) && ...
         ! builtin('any', dimensions != original);
endfunction

function result = mathlab_workspace ()
  info = mathlab_metadata ();
  result = builtin('cell', 1, builtin('numel', info));
  for index = 1:builtin('numel', info)
    entry = info(index);
    preview = '';
    try
      value = builtin('evalin', 'base', entry.name);
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
  frames = builtin('cell', 1, builtin('numel', caught.stack));
  for index = 1:builtin('numel', caught.stack)
    frame = caught.stack(index);
    frames{index} = builtin('struct', 'file', frame.file, 'name', frame.name, 'line', frame.line);
  endfor
  result = builtin('struct', 'message', caught.message, 'identifier', caught.identifier, 'stack', {frames});
endfunction

function [figures, messages] = mathlab_figures (session_directory, request_id)
  figures = {};
  messages = {};
  ## Enumerate native figure handles without findall's M-file traversal, which
  ## can invoke a project's shadowed size helper even when no figures exist.
  handles = builtin('__go_figure_handles__', true);
  for index = 1:numel (handles)
    handle = handles(index);
    image_path = fullfile (session_directory, sprintf ('figure-%d-%d.png', request_id, index));
    try
      ## Export warnings are reported separately, so a renderer failure cannot erase results.
      ## Export callbacks and printer diagnostics share the same live console
      ## stream as foreground source. Only caught export failures are duplicated
      ## as structured figure_warnings; no extra captured output string exists.
      print (handle, image_path, '-dpng', '-r120');
      title = get (handle, 'name');
      if (isempty (title))
        title = sprintf ('Figure %g', get (handle, 'number'));
      endif
      figures{end + 1} = struct ('path', image_path, 'title', title);
    catch caught
      messages{end + 1} = sprintf ('Figure export failed: %s', caught.message);
    end_try_catch
  endfor
endfunction
