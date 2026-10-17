# Native Octave debugger adapter

`mathlab_debug_bridge.cc` connects GNU Octave's interpreter events to a private file protocol for breakpoints, stepping, stack inspection and evaluation while paused. It is project-owned GPL-3.0-or-later adapter code. Octave retains ownership of program execution and scopes.

This component is optional and under development. `Engine::enable_debugger` and the desktop's `--debugger`/`MATHLAB_DEBUG_BRIDGE` setting enable it. Build it against the selected Octave version with `bash scripts/build-debugger.sh`; this builds the debugger, script-local scope and authentic input adapters and replaces each generated file atomically after all three compile. The runtime ABI is platform/version dependent; compiled artifacts are generated locally and are not source files. The worker requires `mathlab-debugger` protocol version 4 and checks its capabilities before installing the event bridge:

| Required logical capability | Purpose |
| --- | --- |
| `source_entry` | Guarded execution of source/base evaluation. |
| `stable_registry` | Preserve the genuine inactive helper's retained handle state. |
| `off_path_registry` | Resolve that helper without reserving its folder on the user load path. |
| `breakpoint_source_metadata` | Bind final original-source metadata before breakpoint registration. |
| `streamed_evaluation` | Direct scoped evaluation with bounded quiet/live output, without `evalc`. |
| `input_wait_cancel` | Cooperate with the verified input adapter when a paused read must be canceled. |

The companion [scope adapter](SCOPE_BRIDGE.md) uses `mathlab-scope` version 2; scope and debugger modules share public-API validation, reflection and binding helpers in [`mathlab_source_context.h`](mathlab_source_context.h). The [input adapter](INPUT_BRIDGE.md) uses `mathlab-input` version 1 with `original_builtin`, `caller_scope`, `separate_reply` and `genuine_reads`. Enable input before first user execution with `MATHLAB_INPUT_BRIDGE` or `Engine::enable_input_adapter`; previously captured original input handles prevent safe late activation.

The Rust `mathlab-debugger` client reads pause state without blocking the engine execution request. It submits revision-checked commands through an atomic private mailbox. The native adapter dispatches commands on Octave's interpreter thread; no monitor thread accesses user variables or evaluates code. A paused scope can be inspected, evaluated, stepped and continued. Idle workers must call `mathlab_debug_bridge('poll')` to service breakpoint commands. Calling the bridge without arguments disconnects and joins the monitor thread; an Octave exit hook also performs cleanup.

Setting a breakpoint in an existing saved `.m` file appends its source directory to the session search path if absent, without changing the working directory or moving an existing path entry. Octave otherwise reports line zero for files outside its path; zero is rejected, rather than displayed as an installed breakpoint. A shadowed same-name file or unsupported target produces a diagnostic. Ordinary function/class buffers must be saved first.

`Engine::prepare_debug_file` and `Engine::prepare_debug_buffer` register immutable script snapshots without executing their statements. Saved or unsaved scripts support original-source breakpoints in script statements, forward local functions and nested functions. Pause and stack mappings distinguish retained old/new generations, including handles whose function bodies were never invoked before debugger setup. Unsaved function/class sources receive a save-first diagnostic from the language adapter's `SourceKind` classification.

Prepared script ASTs are cached separately from numerical functions. The genuine inactive `mathlab_scope_register` helper owns retained local handles; public `symbol_scope::install_subfunction` aliases there allow `bp_table::add_breakpoints_in_function` to maintain statement debugging across Continue. Aliases preserve each function's scope/name and do not publish local names globally. The helper's canonical identity is validated before it can host aliases. Native activation preserves that helper's existing persistent keys in an unlocked public command-line binding, removes its dedicated registry folder from the load path, and clears the duplicate on-path cache slot. Later function/all clearing removes the helper and resets local persistent state; native setup then parses its exact file outside the load path. This prevents path-cache refresh from discarding breakpoint-table membership. The native debugger has no owning numerical-function cache or `mlock` exception for local state. On entry, configured points can bind to the new objects.

The worker registers local handles before entering user source. Native scope preparation parses exact private definitions without executing their bodies; the interpreted fallback uses a return-only factory with an exact canonical identity check. No registration prefix or transport variable is injected into the script. Prepared native execution receives the exact retained handle cell through `mathlab_debug_bridge('source', generated_path, retained_handles, original_path, original_source)`, parses or retrieves the executing AST before installing its filename-local bindings, and calls it in the base frame.

Original-source metadata comes from the full immutable snapshot. Before the first prepared breakpoint is registered, the debugger reads that snapshot with an 8 MiB bound and validates its generation directory, then reflects the actual script/function object. Execution keeps the same display metadata and immutable `scope().fcn_file_name()` lookup key. Checked native script, outer-local, and nested execution reports original `mfilename('fullpath')` and loads resources relative to that original folder. Outer handles constructed after reflection report the original file; nested `functions(handle).file` remains empty on the audited Octave version, and later metadata changes cannot repair file strings cached by already-created handles. This is selected reflection support, not full MATLAB introspection parity.

User `dbclear all`, host-specific clear and individual frontend removal are honored through public table operations and reconciliation. `synchronize_prepared` checks retained scopes before each execution request; subsequent direct old-handle calls do not encounter cleared AST marks. Raw `dbstatus` still reports helper/subfunction metadata rather than original-source frontend locations. A single evaluation that clears all breakpoints, installs an ordinary breakpoint and calls an old handle can run before another reconciliation; that combined case remains outside the checked clear contract. Pre-entry registration allows user script-local `mathlab_scope_register` definitions and same-name base handle variables. Internal helper names still share the global function namespace: a foreign global helper or fallback factory is rejected by its canonical identity guard without execution or replacement. See the [readiness contract](../docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior) and [collision history and remaining global boundary](../docs/KERNEL_READINESS.md#registration-name-collision).

Run the real numerical-runtime proofs with `MATHLAB_DEBUG_BRIDGE="$PWD/native/mathlab_debug_bridge.oct" cargo test -p mathlab-debugger --test octave_debugger`. They check an actual function breakpoint, local variable scope, stack, stale revision rejection, stepping into/out of functions, continuation, a nested evaluation breakpoint and restoration of the enclosing pause, guarded quit and clean exit. The tests are skipped when the adapter path is not supplied. The adapter has been exercised against GNU Octave 11.3.0 on macOS arm64; other runtime versions and operating systems still need validation.

The enabled engine suite additionally checks saved/unsaved script-local/nested pauses, cold retained-generation handles, registry and breakpoint clears, original-source snapshots, and parse/Quit recovery. Run the enabled integrations with matching modules:

```sh
MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  MATHLAB_DEBUG_BRIDGE="$PWD/native/mathlab_debug_bridge.oct" \
  MATHLAB_INPUT_BRIDGE="$PWD/native/mathlab_input_bridge.oct" \
  CARGO_INCREMENTAL=0 cargo test -p mathlab-engine -p mathlab-debugger -p mathlab-desktop --locked
```

## Scoped evaluation and output

Scoped evaluation has the same mutation privileges as Octave's debugger prompt.
Both file-protocol actions use public `interpreter::eval_string` in the actual
paused frame. Neither uses `evalc`: on Octave 11.3.0, genuine `input()` can reset
the pager buffer that `evalc` later tries to restore, reproducing a segmentation
fault even without this input adapter.

| Rust API / command action | Output behavior |
| --- | --- |
| `Debugger::evaluate` / `evaluate` | Returns the legacy captured string and suppresses forwarding of the observed sinks. Used for validated primitive previews. |
| `Debugger::evaluate_output` / `evaluate_stream` | Forwards observed output live and returns `EvaluationOutput { output, streamed, truncated }`. Used for user evaluation in the desktop. |

The observer collects language-output callbacks and stable C++ stdout/stderr
sinks, preserving the existing pager ownership. It keeps the first 1 MiB of
observed bytes per scoped evaluation, then counts further bytes and adds a short
clipping notice to the returned copy. The public `octave::string::u8_validate`
utility replaces invalid UTF-8 before JSON encoding, including a character split
at the raw capture boundary. This can expand the returned text to at most 3 MiB
plus the notice; it is not a promise of byte-for-byte equality with that raw
prefix. Relying on JSON encoding alone produced an invalid Unicode response in
the boundary regression. `output_truncated` reports the raw copy's clipping;
`evaluation_streamed` reports live forwarding. Live output continues past the
capture limit through the session console, whose subscriber and display bounds
are independent. Nested evaluations use separate captures; only the active top
capture observes a byte, avoiding duplication into every enclosing watch.

Quiet evaluation is not a sandbox or complete OS-output capture. Direct stdio,
including genuine input prompt/echo, and subprocess writes can still reach the
session's shared pipe. The input adapter retains the original builtin's caller
scope and uses a separate reply mailbox; neither mode reads user answers from
worker recovery stdin. The desktop displays user evaluation output in the Command
Window when subscribed, without appending its returned copy again. Its primitive
preview requests stay quiet, so routine scope inspection does not print previews
into the Command Window.

A command timeout does not terminate evaluation. Entered code can pause at a nested
breakpoint; the Rust client serializes mailbox publication, then releases that
lock while waiting so another thread can resume the nested pause. Unwind guards
restore the enclosing pause and output observers on normal return or error. The
outermost evaluation temporarily owns the interpreter safe-point signal hook,
chains the previous hook and restores the previous hook/debug mode on exit.
Actual SIGINT and requested Quit disable per-statement debug-mode interrupt
swallowing during cancellation, so later watch statements do not run. SIGINT
during a watch also requests genuine debugger Quit to terminate the enclosing
execution. The worker's durable cancellation marker distinguishes that Stop from
an explicit debugger Quit and reports `MathLab:Interrupted`.

The debugger stores a verified native input-adapter function handle and calls its
`cancel_wait` operation before Quit. `dbquit` by itself changes debugger state but
does not release a blocked genuine input read. The adapter releases that reader
without consuming recovery stdin. Interrupted/quit evaluation publishes a failed
debugger response before unwinding, releasing its pending client instead of leaving
it to time out. User cleanup still needs to execute, and the original numerical
workspace must remain usable afterward.

Targeted paused input reply evidence (local evidence unavailable in source exports: `../artifacts/desktop-paused-input-test.log`)
checks caller-local expression input and live forwarding. Native quiet/live
causal probes additionally check Busy rejection, requested Quit without later
watch continuation, user cleanup and same-process reuse. The actual desktop
Stop regression (local evidence unavailable in source exports: `../artifacts/desktop-app-stop-test.log`),
`app_stop_interrupts_a_paused_evaluation_after_input_has_already_been_consumed`,
passes against the final module: Stop ends the outer request with
`MathLab:Interrupted`, releases the pending Evaluate, clears busy state and
preserves a base-workspace value for the next request. The native causal Stop
report (local evidence unavailable in source exports: `../artifacts/debug-input-stop-report.json`) separately checks waiting,
entered-expression and nested-watch cancellation in both quiet and live modes,
including cleanup and absence of later statements. Current full-suite and packaging
records are maintained in VALIDATION.md (local evidence unavailable in source exports: `../artifacts/VALIDATION.md`); optional
native tests that take a skip branch do not validate their native path.

Broader debugger, terminal and cross-platform behavior remains development work.
These checked paths do not establish MATLAB debugger parity or faster execution.

## Guarded worker entry

Programs hosted under `--eval` must enter user code through `mathlab_debug_bridge('source', path)` or `mathlab_debug_bridge('evaluate', code)` when the debugger is enabled. These methods return true on normal completion and false after a requested debug quit, preserving the base workspace. The boundary catches Octave's otherwise private debug-quit exception only after the adapter's explicit Quit command or an actual SIGINT during a scoped watch requested it. The worker maps that result to `MathLab:Interrupted` when its durable cancellation marker identifies Stop. Ordinary execution errors and interrupts outside scoped watches retain their existing propagation paths. Direct unguarded `dbquit all` under Octave 11.3.0's `--eval` was observed to abort the process; the adapter regression checks both guarded source and evaluation, verifies assignments before quit survive, later statements do not run, and subsequent code executes successfully.

Prepared script execution explicitly enters the base frame and calls its cached AST. When prepared points are configured, general evaluation uses public `interpreter::eval_string` at that frame; the additional `evalin` boundary was observed to miss retained-handle pauses. Unwind guards restore the caller frame and debug state on completion, errors, interrupts and controlled Quit. The parse-error regression verifies the original process, workspace and prepared breakpoint remain usable.

Interpreter event semantics are documented in the [GNU Octave 11 evaluator source](https://docs.octave.org/doxygen/11/df/d57/pt-eval_8cc_source.html).

The session console's ordering, I/F input records, reliable pending-input handle,
frontend EOF/echo behavior and resource bounds are documented in
[CONSOLE_PROTOCOL.md](../docs/CONSOLE_PROTOCOL.md). The input adapter's original
builtin delegation, FILE callback lifetime and compatibility limits are described
in [INPUT_BRIDGE.md](INPUT_BRIDGE.md).
