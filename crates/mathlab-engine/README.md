# GNU Octave execution engine

This crate runs one persistent GNU Octave process per `Engine`. Rust owns process
lifecycle, request transport, cancellation, and deserialization. Octave executes
MATLAB-style code and performs the numerical work. The engine contains no substitute
parser or simulated interpreter.

`Engine::start` accepts an executable path and a working directory. Discovery checks
`MATHLAB_OCTAVE`, `OCTAVE_EXECUTABLE`, `PATH`, and common macOS/Linux install locations.
GNU Octave 11.3.0 is tested locally; Octave must have built-in JSON support.

## Operations

- `execute`: evaluate code in the persistent base workspace.
- `run_file`: execute an existing `.m` script in that workspace using a structured
  path argument, including filenames with spaces and apostrophes. MATLAB script
  local functions are prepared automatically by `mathlab-language`.
- `run_buffer(source, path)`: execute an unsaved `.m` editor buffer without saving
  it. Source frames refer to the intended original file and original line numbers.
- `prepare_debug_file`, `prepare_debug_buffer`: stage an immutable source snapshot
  and register original-path debugger mappings without executing user statements.
  Unsaved function/class files must be saved before setting debug breakpoints;
  classification uses the same lexer as script preparation.
- `workspace`: return names, classes, available dimensions/byte counts, and bounded
  previews. `dimensions_known`/`bytes_known` distinguish unavailable object or
  recursive-container metadata from an empty/zero-byte value.
- `capabilities`, `require_capability`: inspect the negotiated ordinary worker
  protocol and reject unsupported dependent operations before dispatch.
- `workspace_snapshot`: obtain safe metadata with binding tokens; it never invokes
  object size/display/getters just to list a workspace.
- `read_value_page_checked`: read a lossless numeric/logical N-D window using an
  accepted root token and typed cell/struct path. Integer components retain their
  declared Rust types; complex and sparse metadata survive transport. Every higher
  axis has an explicit selector. Unsupported objects never trigger getters.
- `read_value_page_checked_with_metrics`: the same read plus actual typed-result
  JSON bytes and elapsed request/settlement/decode time. This is not a physical
  allocator count or a numerical-kernel benchmark.
- `read_variable_checked`, `set_variable_element_checked`: inspect primitive pages
  and commit class-preserving scalar edits against the exact accepted token.
  Execution, rebinding, errors, Stop, recovery and restart revoke older tokens.
  Failed validation leaves values unchanged; an uncertain commit is never retried.
- `read_variable`: read a row-major page with one-based starts, clipped to the
  existing dimensions. Pages are limited to 2000 cells; N-D arrays display their
  first plane. Numeric/logical 2-D matrices are editable, including sparse matrices.
  Char/cell/struct values have bounded read-only cell previews.
- `set_variable_element`: assign a scalar literal to an existing numeric/logical
  matrix element. Bounds, class, and dimensions are preserved. Source expressions
  are rejected; exact decimal int64/uint64 values do not round through double.

- `set_directory`, `clear`, `restart`: manage the session.
- `list_packages`, `load_package`: inspect and explicitly enable installed toolboxes.
- `cancellation_handle().interrupt()`: interrupt a busy process from another
  thread with SIGINT on Unix/macOS, preserving its base workspace.
- `cancellation_handle().terminate()`: force termination from another thread,
  discarding workspace; restart is required afterwards.
- `execute_with_timeout`, `run_file_with_timeout`: run with a deadline for automation.
- `enable_debugger(bridge_path)`: install an optional native `.oct` adapter compiled
  against the selected Octave ABI, returning a clonable `mathlab_debugger::Debugger`.
- `debugger_handle`: clone that handle while another thread runs the engine.
- `enable_scope_bridge(path)`, `scope_bridge_path`: opt into the native per-file
  binding adapter for retained script-local functions. `MATHLAB_SCOPE_BRIDGE` enables
  it when starting an engine; without that setting the Rust preparation fallback
  remains available.
- `subscribe_console`: receive a session-lifetime subscription to ordered raw-byte
  console events independently of the thread running a request. It survives restart;
  `console_generation` identifies the current process generation.
- `enable_input_adapter(path)`, `input_handle`, `input_bridge_path`: opt into
  authentic independent input replies. `MATHLAB_INPUT_BRIDGE` enables the native
  adapter during startup, before user execution. See the input contract below.
- `worker_source_path`: return the exact copied transport-worker path for filtering
  internal frames without hiding user files that share its basename.

Execution returns a bounded aggregate of the streamed console bytes, structured error details with source
frames, workspace metadata, PNG figure paths, renderer warnings, execution time,
and the resulting current directory. A script error is part of `ExecutionResult`;
lexical preparation errors carry `MathLab:SourcePreparation` source frames in
`Error::Worker`, while transport/process failures return other `Error` variants. Assignments made before a script error
remain available, as they do in a normal interpreter session.
Relative script and directory paths resolve against the session's current folder,
including after user code changes that folder with `cd`.

The [workspace protocol](../../docs/WORKSPACE_PROTOCOL.md) defines canonical
counter encoding, conservative invalidation, safe metadata and graphics
integration seams. Existing unconditional APIs remain available; editable desktop
pages use the checked APIs. The optional [typed-page service](../../docs/VALUE_PAGE_PROTOCOL.md)
adds exact N-D reads; atomic ranges and undo remain separate work.

## Transport

The bundled `octave/mathlab_worker.m` is copied into a private temporary session
directory. Rust atomically publishes a UTF-8 JSON line as `request.json`; stdin is
reserved for worker recovery. Calls are sequential within one Engine; independent
Engine instances can run concurrently.

Stdout and stderr share one OS pipe, including inherited subprocess writes. An
independent reader consumes chunks of at most 8 KiB, publishes monotonically
sequenced `ConsoleEvent`s, and assembles `ExecutionResult.output` from those same
bytes. The order is the common pipe's byte order; it does not reconstruct source
statement order across concurrent writers or user-selected buffering. Channel
identity is intentionally merged. The request ID is transport context rather than
proof that a background producer belongs to that command. Output arriving while
idle remains available through the session subscription.

Begin, End, and Recovery fences contain a random generation nonce, invocation, and
request ID. A small embedded Rust executable publishes each fence with one OS
write of at most 512 bytes. The worker launches it through `popen`, leaving SIGINT
enabled; `system` would temporarily ignore SIGINT and lose an early Stop. Large
completion metadata travels in separate bounded files, atomically renamed before
End. Current-generation malformed fences, unknown IDs, and malformed metadata
terminate the session. Ordinary protocol-like output and invalid console UTF-8
remain console data; the display aggregate marks decoding loss explicitly.

On Unix/macOS, Octave runs with `--persist`. Its SIGINT bypasses language
`try/catch`; cleanup writes separate immutable interruption metadata and a
Recovery fence. Rust re-enters the same process with a fresh invocation and waits
for its actual ready fence. If normal End won a cancellation race, its result is
preserved and a real interrupt-checking ping settles recovery before completion is
published. Old invocation fences cannot complete a later request. A durable
cancellation marker prevents a request interrupted before mailbox consumption
from running after re-entry. Cancellation signaling and End deactivation share
the process mutex. Repeated Stop clicks within one request send only one SIGINT.

The worker's local scope keeps protocol variables out of the base workspace.
Startup has a 30-second deadline. Inline source is limited to 8 MiB and metadata
to 32 MiB. The legacy output aggregate retains at most 32 MiB of raw bytes, with explicit
truncation and dropped-byte fields; excess output does not kill the session.
Each subscriber retains at most 1 MiB/256 queued events and reports delivery gaps
without blocking another subscriber or completion. Streaming callers must not
append the aggregate again. The result's `console_end_sequence` is a real public
event cursor, including recovery/ping completion where applicable. See
[the protocol contract](../../docs/CONSOLE_PROTOCOL.md) for limits and states.

Gnuplot is selected when available so invisible figures can render without a
display. Child processes receive `QT_QPA_PLATFORM=offscreen` and a private font/cache
directory. PNG paths survive until Engine is dropped. The desktop can copy files
for permanent exports. The original figure objects remain in Octave and can still
be modified by code.

## Authentic interactive input

The optional native input adapter requires protocol `mathlab-input` version 1
and the original-builtin, caller-scope, separate-reply and genuine-read
capabilities. It delegates expression/string semantics to Octave's saved original
builtin and routes only its real command-editor reads through a private mailbox.
It does not guess waiting state from printed text or use recovery stdin for answers.

Install it before the first user execution, including package initialization.
An already captured `@input` retains its original builtin; late activation is
therefore rejected until restart. A different module cannot replace an installed
adapter in the same process. Restart re-enables the configured adapter before
user execution, and existing cloned `InputHandle`s continue to target the session.

`InputHandle::pending()` returns a reliable `PendingInput` snapshot while execution
blocks elsewhere. Its request contains a complete generation/invocation/request/input
key, original prompt, expression/string mode and public console sequence. Each
snapshot is `Waiting` or `Submitted`; the latter means an atomic reply was published,
not that expression evaluation succeeded. `reply(key, text)` sends one exact UTF-8
line with native echo enabled. `reply_with_echo(key, text, false)` serves terminals
that already echoed the user's line. `eof(key)` sends EOF, which is distinct from
an empty line. Stale, duplicate, canceled, restarted or closed keys cannot publish
answers for another read. Answers never appear in control events.

The console stream includes authentic `InputRequested` and `InputFinished` events.
The prompt's bytes precede its request event; the pending snapshot is installed with
that event's sequence before subscribers wake. Slow consumers recover pending state
from the snapshot rather than relying on an unevictable event. Line/EOF finish means
the reader consumed the response **before** original-builtin expression evaluation,
so entered expressions can request nested input under a fresh key. Normal execution
completion, interruption and disconnection revoke any unfinished input. A native
read abort can finish with `Interrupted` or `Error` without pretending a line was
consumed; a caught read error therefore does not strand a subsequent input call.

Replies are limited to 1 MiB of decoded UTF-8 and reject NUL/CR/LF within one logical
line. Encoded reply files are capped at 8 MiB, request/finish metadata at 128 KiB,
and decoded prompts at 64 KiB. The original EOF behavior on this runtime is an
`input: reading user-input failed!` error in both modes; an empty line instead
returns an empty numeric value or empty string as appropriate.

Actual native-gated engine tests exercise caller scope, Unicode/whitespace,
single native echo, prompt ordering, nested expressions, `builtin('input',...)`,
EOF and subsequent requests, `clear all`, no-output calls, and same-process SIGINT
recovery/restart with stale-key rejection. A caught malformed reply also finishes
with `Error` before the next input in the same execution. Logs are in
[`input-engine-tests.log`](../../artifacts/input-engine-tests.log). Separate real
native proofs cover quiet and streamed paused evaluations while waiting, evaluating
a submitted long expression, and nesting a watch inside an outer input wait. Both
SIGINT and debugger Quit unwind the watch, run cleanup, finish the outer execution,
retain the process/workspace and permit another input; see
[`debug-input-stop-report.json`](../../artifacts/debug-input-stop-report.json) and
[`debug-input-quit-report.json`](../../artifacts/debug-input-quit-report.json).
These are scoped runtime checks, not full MATLAB input conformance. Windows support
is not established.

## Script local functions

Scripts with local definitions are lexically prepared into an immutable, uniquely
named `.mlsource` script plus padded `private/*.m` function files. Octave's native
`source` reads `.mlsource`; it is not autoloaded as a public script. The generated
parent stays at the end of the load path, while its private directory is never
added directly. Exact original path/source pairs share that generation, retaining
local persistent state; edited buffers create new generations so old function
handles continue to use their original definitions. Each generation also retains the full original text in a nonautoloadable
`.mlsnapshot` file for debugger source display. Generated scopes remain until
restart/drop. Ordinary saved function/class/script files without locals run
directly through Octave. Source errors from every retained generation are mapped
back to original paths, line numbers, and `script>local` names; worker frames are
removed. Cleanup preserves intentional user path changes and the current folder.

Before executing a transformed script, the worker obtains its local handles in
transport scope and retains them in a small inactive interpreter-side registry.
No registration call is injected into user source and no temporary variable enters
the base workspace. A user local function or base handle variable named
`mathlab_scope_register` keeps its normal meaning. The portable backend uses a
return-only internal factory in the generated capsule; its exact canonical file
identity is checked before invocation. The factory creates handles without calling
any user function body. Its unique internal symbol and `.m` file can appear in
raw global lookup, unlike the user local names, which remain private.
The registry holds scoped function handles outside the base workspace, so consecutive runs retain local persistent state even without a user
handle and after Clear Workspace (`clear -variables`). User `clear functions` and
`clear all` reset the inactive registry. Without the optional scope adapter, a
native cache deviation remains: after
another generation uses the same local function name, rerunning an older source
can bind its named calls to a fresh function object. Explicit retained handles
continue to use their prior definition and state; this behavior has a regression
test and is not presented as full MATLAB persistent-state compatibility. More
generally, a load-path mutation (adding another generated scope, user `addpath`,
or package loading) can reset named private-function lookup even while retained
handles still own their original state. Stable-path repeated runs are tested.

The optional `mathlab_scope_bridge.oct` adapter prepares named scoped handles from
exact private-file parsing before script entry, then uses Octave's public per-file
local-function binding API to bind the retained function objects to the canonical
`.mlsource` script and each extracted private function file on every entry. It
parses the actual executing script AST before installing the retained bindings,
then runs that AST in an explicit base frame with unwind restoration. The stable
registry retains its numerical objects across path-cache refreshes. It preserves
named-call state across load-path changes, package loading, and switching
between source generations without creating global local-function names or
rewriting user call identifiers. Base-variable precedence and sibling/nested
functions remain owned by Octave. The worker restores the adapter configuration
after user `clear functions`/`clear all`; those operations still reset persistent
state. Enabling validates `mathlab-scope` protocol version 2 with pre-entry,
original-source reflection, source-entry, and stable-registry capabilities first.
It rejects changing an already selected adapter path and persists the selected
path across restart.
The adapter is optional and must be compiled against the selected runtime ABI.

Native adapters keep the genuine canonical internal registry in Octave's public
command-line function table. A normal function-on-path could be reloaded during a
path refresh and lose its persistent cells. This internal entry survives path
changes without `mlock`; `clear functions` and `clear all` still clear it, and the
worker reinstalls/configures a fresh helper before the next request. A foreign
helper definition is rejected rather than replaced. Local numerical function names
are never installed globally. Both adapters advertise `stable_registry` and
`source_entry` capabilities, which the worker checks before activation.

When the native scope adapter is enabled, outer and nested local function
`mfilename` and relative resources derived from it use the original editor path.
Outer `functions(handle).file` also reports that path, including nonexistent
unsaved paths. Cached function text retains the original source snapshot. Immutable
lookup scope identities stay generated so old/new debugger generations remain
separate. Native nested `functions(handle).file` remains empty in Octave's current
nested-handle implementation. The interpreted fallback exposes generated local
paths instead. Native prepared script entry also reflects script-level `mfilename`
and source-derived resources to the original path. The interpreted fallback still
reports its generated script context.

This is a scoped adaptation, not complete MATLAB language conformance. Help and
`localfunctions` enumeration are not emulated. Restoring/removing load paths can
remove generated parents and evict private scopes. User `run`/`source` and bare
script-name calls bypass engine preparation. Local classes and local function
definitions inside control blocks are not supported. See `mathlab-language` for
parser boundaries.

Worker and debugger use the canonical registry helper outside user source; a
foreign global file shadowing that exact transport helper is rejected precisely.
Explicitly clearing that internal helper can reset the retained registry. Caught
reflected `.m` error stacks preserve native enclosing names when present. If only a
nested name and original line survive, qualification is added only when retained
snapshots agree on the enclosing function; ambiguous frames keep their raw name.
Native debugger pause/stack frames retain immutable scope identity and can map
old and new generations exactly.

## Optional debugger

Build the adapter using `bash scripts/build-debugger.sh`, then pass the resulting
`native/mathlab_debug_bridge.oct` to `enable_debugger`. Debugger handles inspect
actual paused Octave scopes, set/clear file breakpoints while idle, evaluate in the
paused scope, and issue revision-checked Continue, Step, Step In/Out, and Quit.
The worker services idle debugger commands and keeps debugger transport independent
of the main execution request. Restart reinstalls the configured bridge in a new
private directory; old debugger clones are invalidated, so callers must obtain the
replacement with `debugger_handle()`.
Setting a breakpoint outside the load path appends its source directory at the
end of that path; it preserves the current folder and existing path precedence.

Enabled execution runs through a narrow native source/eval boundary because Octave
11.3's debugger quit exception otherwise aborts a `--eval` process. Requested Quit
becomes `ExecutionResult.error.identifier == "MathLab:DebugQuit"`, preserving base
assignments made before stopping and retaining the process for the next command.
Ordinary errors and SIGINT keep their usual error/unwind behavior. The adapter is
optional, disabled by default, and must match the runtime's ABI. Breakpoints in
prepared saved scripts, unsaved script snapshots, local functions, and nested
functions map to original paths and source lines. Paused names include enclosing
functions, such as `script>outer>inner`. The mapping is shared by debugger clones,
selects the latest prepared generation for new breakpoints, and retains older
generations for existing handles. Native breakpoint registration reads the validated
same-generation full snapshot and installs final display metadata before registering
a point, so reflection cannot silently change a script point's bookkeeping identity.
The worker requires the `breakpoint_source_metadata` capability.
`DebugState.runtime_file` preserves the immutable
runtime identity, and `Debugger::source_snapshot` reads that generation's exact
full original text without executing code. Saved ordinary files use their direct paths;
unsaved function/class buffers must be saved before debugging.

Prepared breakpoints use scoped aliases in the inactive runtime registry helper
for Octave's normal breakpoint bookkeeping. Original local names stay private,
and helper aliases do not change user function names. Optional scope reflection
changes display paths while preserving immutable lookup identities. A pre-entry
registration refresh marks the
exact retained function objects. The debugger cache owns script ASTs and does not prevent
`clear functions`/`clear all` from resetting local state.
Generic prepared execution uses Octave's public `eval_string` in an explicitly
selected base frame with unwind restoration. `evalin` could skip retained-handle
breakpoints despite marked bodies and active global bookkeeping. Actual warm and
previously uncalled old/new handles now pause in their respective generations;
no user function calls are added to warm a handle, and call identifiers are not
rewritten.

The worker synchronizes cleared AST marks before each execute/run-file request.
`dbclear all`, clearing the registry host, and individual prepared clears are
tested across later requests while preserving unrelated ordinary breakpoints.
One combined evaluate request that clears prepared points, sets an ordinary point,
and then invokes an old handle can still encounter a stale mark; split those steps
into separate commands. This is not complete native `dbclear` conformance.

Before installation, the worker requires the adapter's `mathlab-debugger` protocol
version 4 handshake with the `streamed_evaluation` and `input_wait_cancel`
capabilities. Outdated adapters are rejected before debugger execution is
enabled. A different adapter path cannot replace an already loaded, locked module;
restart MathLab to select another adapter. A failed replacement leaves the current
debugger handle and restart configuration intact.
A desktop Stop action issues revision-checked debugger Quit while paused; ordinary
running code uses the cancellation handle's SIGINT operation. For paused scoped
evaluations waiting on authentic input, the adapters cancel the read before leaving
the debugger loop. Native SIGINT also unwinds that watch and its outer execution;
the durable cancellation marker distinguishes interruption from an ordinary Quit.

## Current boundaries

Restart, force termination, and automation deadlines clear the runtime workspace.
The worker launcher currently requires a Unix shell; Windows session startup
returns an explicit unsupported diagnostic before spawning a worker. Windows
native pipe/helper code is present but is not a validated Windows launcher.
Native foreign routines and user-selected I/O buffering may defer SIGINT until
return; callers can force termination if necessary. Interrupted execution keeps
already streamed output, assignments, and live figure objects. Figure previews
refresh on the next successful execution. Package initialization and figure export
output use the same console stream; successful printer warning text is displayed
there, while caught export failures also appear in `figure_warnings`.
Interactive `input()` requires the optional adapter described above. Without it,
there is no authenticated reply route. Interactive figure protocols are not implemented.
Figure previews are raster exports. Scripts execute with the session's current
directory; callers should set that directory deliberately when running a project.
Packages are not installed or loaded implicitly. User code has the same filesystem
and process access as the Octave process.

`cargo test -p mathlab-engine` verifies framing bounds and real Octave sessions:
matrix solving, persistence, errors and source frames, `.m` execution, MAT7 binary
round trips, plotted PNGs, package registry operations, interruption, deadlines,
restart, `clear all`, and previews for complex/sparse/nonfinite/container values.
Infinite inline loops and script loops are actually interrupted, then subsequent
commands verify the retained matrix values, process identity, and response framing.
Variable Editor tests cover clipped/empty/N-D pages, sparse/logical/complex/NaN/Inf
values, class-preserving edits, exact 64-bit limits, out-of-bounds edits, read-only
containers, and rejection of injected expressions while the session stays usable.
Real console tests prove non-newline output before completion, merged
`disp`/subprocess/stderr byte order, invalid UTF-8 retention, idle background
output, hard-exit output, a 33 MiB output flood with a slow subscriber, and
subscription continuity across restart. Fault-injected worker tests exercise Stop
before Begin, after metadata publication, after committed End, before mailbox
consumption, and an unrelated idle recovery followed by a cancellable request.
Integration tests report a skip only if Octave is absent.
Set `MATHLAB_DEBUG_BRIDGE` to the compiled adapter to enable engine debugger tests:
actual function breakpoints, paused scopes, stepping, continuation, safe Quit for
inline/source runs, restart handle invalidation, and SIGINT recovery with the
debugger enabled. These tests skip if that optional adapter path is absent.
Prepared-debugger tests stage without executing or saving editor text, then pause
in script, private, and nested bodies through consecutive Continue operations.
They cover shared clone mappings, old/new generation handles without warm-up,
unsaved plain-script Step and saved-file restoration, user clear bookkeeping,
retained-handle Quit, and syntax errors followed by usable workspace and breakpoints.
Subprocess marker-looking output (including valid JSON and no final newline) stays
console text; a deliberately corrupt current frame terminates and restarts cleanly.
Set `MATHLAB_SCOPE_BRIDGE` to the compiled scope adapter to test named local state
through path changes, package loading, edited-generation switches, variable clears,
function clears, restart, sibling calls, nested closures, base-variable
precedence, original-path resource lookup, outer handle-file reflection, retained
nested error parentage, and cache refresh after original-file edits. Collision
regressions execute genuine helper-named user functions and same-name base handles,
including an actual paused debugger scope. Malformed local expressions return
structured execution errors without running preceding user statements. Known-deviation tests retain explicit fallback assertions when this
capability is disabled and check the stronger native behavior when it is enabled.
