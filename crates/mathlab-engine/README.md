# GNU Octave execution engine

This crate runs one persistent GNU Octave process per `Engine`. Rust owns process
lifecycle, request transport, cancellation, and deserialization. Octave executes
MATLAB-style code and performs the numerical work. The engine contains no substitute
parser or simulated interpreter.

`Engine::start` accepts an executable path and a working directory. Discovery checks
`MATHLAB_OCTAVE`, `OCTAVE_EXECUTABLE`, `PATH`, and common macOS/Linux install locations.
GNU Octave 11.3.0 is tested locally; Octave must have built-in JSON support.

## Explicit qualified startup

`Engine::start_qualified(working_directory, TrustedStartupConfig)` selects the
`qualified-core-v2` startup profile. Ordinary `Engine::start` retains its existing
behavior; frontend opt-in is described below. The qualified profile currently selects
GNU Octave **11.3.0 / api-v61**, the v2 intrinsic provider and an explicit subset of
the input, scope, value and debugger adapters. Explicitly selecting an additional
`AdapterRole::Graphics` chooses the experimental `qualified-graphics-v2` profile
and its exact graphics worker/service closure. It admits 120 native primitives;
the graphics-disabled core retains its original 115 and unchanged worker bytes.
Call `enable_graphics_service` before user execution to activate the bounded
object registry, line queries, mutations and programmatic callback service.
The combined qualification fixture selects six native modules: the provider,
four core adapters and the graphics bridge. Selection itself allows a declared
subset of core adapters. It does not activate a frontend or establish full figure,
native mouse or MATLAB graphics compatibility. Graphics recovery and native
behavior require their own runtime evidence.

The caller supplies canonical absolute paths and SHA-256 pins through
`PinnedFile`, `AdapterConfig` and `AdapterRole`. `TrustedStartupConfig` includes
the installation root, executable, provider, gnuplot and Ghostscript executables, explicit runtime
function directories, adapter selections and runtime/API versions. Files and
directory identities are checked again before startup; no project directory or
native module directory is added to the admitted runtime function path. The
private transport session is created with owner-only permissions. Inherited
runtime and adapter environment settings do not select this profile.

The qualified child starts with a cleared environment. On POSIX its utility
`PATH` contains only the selected Octave executable's parent, `/usr/bin` and
`/bin`, in that order. This lets user programs and Octave M files invoke system
utilities such as the `mv` used by `movefile`, without inheriting ambient `PATH`
entries or adding project, function or native-module directories. Runtime startup,
the provider, adapters, gnuplot and Ghostscript still use their admitted absolute paths and
pins; the utility path does not participate in selecting them. System utilities
remain part of the host trust boundary, rather than individually pinned files.
Qualified startup still requires Unix session semantics.

Ordinary PNG export requires a selected Ghostscript pin even with the core
profile. The child receives that absolute executable through Octave's `GSC`
environment setting; its directory is not appended to `PATH`. Both export
auxiliary paths must contain only ASCII letters, digits, `/`, `_`, `-`, `+` and
`.` because the selected Octave print/gnuplot helpers can invoke a shell.
After setting the selected `gnuplot_binary`, cold startup explicitly registers
the runtime's gnuplot toolkit before the worker's existing invisible warmup.
This avoids relying on Octave's earlier `PKG_ADD` executable probe under the
constrained `PATH`. Recovery leaves the user's current toolkit choice intact.

The session still follows the caller's temporary root, including roots with
spaces. When its pathname is not a literal shell word, qualified startup gives
the child a `TMPDIR` alias in a private owner-only directory under canonical
`/tmp`. The alias points to the original session: temporary file contents stay
on the caller-selected filesystem, and `HOME`, working directory, worker paths
and admitted runtime paths do not move. Only the alias directory and one symlink
use the system temporary filesystem. Startup checks its ownership, permissions
and target before each spawn; the prepared startup owns its cleanup. An unusable
system alias root fails startup explicitly. This is a Unix compatibility measure
for Octave's unquoted temporary EPS paths, not a change to its installed helpers.

After construction, enable each selected adapter through its ordinary public
`enable_*` method before executing numerical user source. In particular, enable
input before the first execution; previously captured input handles cannot be
redirected safely. Selecting an adapter in configuration admits its identity but
does not enable its user-facing functionality. The five-module release selection
uses the intrinsic provider and all four adapters.

`qualified_startup_status()` returns a serializable `QualifiedStartupStatus` only
after a genuine Ready has matched the admitted identity and while that invocation
still owns an open transport. It reports the selected core or graphics profile, accepted
session context and intrinsic provider/adapter identities. Ordinary startup,
unaccepted or retired Ready, stale contexts and closed transports return `None`.
The intrinsic wire protocol remains `mathlab-intrinsics`, string version `"1"`;
the profile name does not change the wire protocol version. The status proves
startup admission, rather than numerical compatibility or full product readiness.

Caller-supplied pins do not authenticate a vendor. Runtime directory contents,
native dependencies and mutable filesystem state remain trust boundaries; the
checks do not make executing user code a sandbox.

### Explicit frontend configuration

The source integration adds `--startup-config FILE.json` to the CLI and desktop.
It conflicts with `--octave`; the desktop also rejects a debugger-path override.
Without this option, frontend runtime selection keeps its legacy behavior. With
it, the frontends use only the selected configuration and report failures without
runtime discovery or fallback. Compiler, runtime and installed validation of this
new frontend route remain pending.

`startup_config::QualifiedStartupConfig::load` reads a regular UTF-8 JSON file of
at most one MiB. Its envelope is `{ "schema_version": 2, "profile":
"qualified-core-v2", "startup": { ... } }`. `startup` has the fields of
`TrustedStartupConfig` above. File pins are `{ "path": "/absolute/path",
"sha256": "64 lowercase hexadecimal digits" }`; adapter entries are
`{ "role": "input", "file": { ... } }`. All paths must be absolute; existing
startup admission checks canonical paths, file hashes, directories and SDK identity.
Unknown and duplicate fields are rejected.

`startup.ghostscript` is a required file pin. The private schema 1 predecessor is
rejected; callers must explicitly supply schema 2 and the export executable.
There is no automatic upgrade, inherited executable choice or discovery fallback.

This serialized profile requires exactly input, scope, value and debug adapters,
with their standard `mathlab_*_bridge.oct` basenames, plus
`mathlab_intrinsic_bridge.oct`. The `qualified-graphics-v2` envelope instead
requires those five modules plus `mathlab_graphics_bridge.oct` and activates the
graphics registry. `start` admits the selection and enables all selected adapters
before returning an Engine. Configuration-aware `restart` restores that selected
activation, while raw `Engine::restart` continues to disable graphics. The fixed
installed `qualified-core-v2.json` slot selects either profile through its envelope;
only its native paths are rebound to the installed bundle. External runtime,
gnuplot and Ghostscript pins remain literal. CLI doctor and value-probe reports
bind actual accepted Ready status to the five or six native identities;
Ghostscript is an external auxiliary, not an additional native module. The reported context belongs
to that accepted Ready; it is not a later workspace revision.

The private predecessor passed the selected startup, four-adapter, complex-edit,
`clear functions`, Stop/recovery, cold restart, input and normal-exit workflow on
local Octave 11.3. That evidence does not qualify a newly built shared or installed
artifact. Native lifecycle tests are explicitly ignored by the default Rust
test run because they need selected runtime and auxiliary pins plus five or six
already-built modules;
select `trusted_startup_runtime_tests` with `--include-ignored --test-threads=1`
when those prerequisites and the workload lane are available. Missing selected
prerequisites fail these tests; they do not return a successful skip. The registry
lifecycle test additionally checks `clear all` and rejects a foreign command-line
registry before any replacement or requested body executes.
The new core and graphics ordinary-PNG fixtures also remain unrun. They require
`MATHLAB_QUALIFIED_TEST_GHOSTSCRIPT` alongside the other explicit selections, and
check export after finite-loop Stop, input Stop, `clear functions` and configured
cold restart while retaining the user's explicit toolkit choice.

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
`input-engine-tests.log` (local evidence unavailable in source exports: `../../artifacts/input-engine-tests.log`). Separate real
native proofs cover quiet and streamed paused evaluations while waiting, evaluating
a submitted long expression, and nesting a watch inside an outer input wait. Both
SIGINT and debugger Quit unwind the watch, run cleanup, finish the outer execution,
retain the process/workspace and permit another input; see
`debug-input-stop-report.json` (local evidence unavailable in source exports: `../../artifacts/debug-input-stop-report.json`) and
`debug-input-quit-report.json` (local evidence unavailable in source exports: `../../artifacts/debug-input-quit-report.json`).
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
full original text without executing code. With debugging enabled, saved plain
scripts also use immutable generations selected by their exact source bytes.
Changing a saved file therefore changes the executed generation even when its
length and modification time stay the same. Existing frontend breakpoints follow
the new generation, and clearing one original-file point removes its retained
generation targets. Saved function/class files keep their direct paths; unsaved
function/class buffers must be saved before debugging.

This migration tracks breakpoints installed through `Debugger`. Arbitrary native
`dbstop` marks are not imported into prepared generations, and native clear
commands combined with a newly prepared generation need separate qualification.
These command-parity limits do not establish complete MATLAB debugger coverage.

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

`rust_program::ProgramSessionV1` executes the immutable located program AST through
an explicit continuation machine. It supports isolated local-function frames,
atomic multioutput caller assignment, column-valued loops and typed logical
conditions. Errors, Stop and resource exhaustion retain completed script effects
and previously emitted callee displays; subsequent requests can recover. The
separate keyed cancellation barrier stays active across statement publications.

This initial `rust-program-execution-v1` profile retains 16-by-16 values and
explicit frame/task/value/work/event budgets. The single `Arc` control allocation
uses stable std allocation, with a 64-byte logical metadata allowance and test
preflight; allocator overhead/Arc ABI/RSS are not inferred and global OOM may
abort. Value/vector/name reservations are fallible. Arity, output, resolution,
condition and empty-column policies are qualified project rules pending native
reconciliation. Variadic formals, general matrix powers/solves, growth and
logical-mask assignment are unsupported. Existing v1 APIs/protocols stay intact;
CLI, larger arrays, advanced types and desktop integration are later milestones.

Reference reconciliation separates an unassigned local call's observed `nargout`
(0) from implicit result delivery. An available first declared output becomes
`ans`; zero outputs or an unset first output leave `ans` unchanged. Explicit
output requests retain their counts and required-output checks. F05 qualifies
its single assigned-output case. Phase B G16 also qualifies successful bare calls
with an unset first output and no new `ans`; G18 qualifies requested counts
0/1/2/2 for bare/single/multiple/discarded outputs. This Rust profile qualifies
only its admitted bare/single/named-multiple contexts (0/1/2); ignored-output
syntax `[~,x]` remains unsupported and open. It does not execute full G18.
Event details and unobserved output combinations remain project policies. Repeated simple output targets are now admitted: RHS executes once and
one atomic transaction applies occurrences left to right, so the rightmost value
wins without duplicate bindings. Unsuppressed per-occurrence value snapshots
follow the existing project event policy; native F15 only qualifies suppressed
last-write behavior. Existing quotas and failure/Stop prefix guarantees remain.

Phase B control reconciliation rejects NaN in an `if`/`elseif`/`while` condition
with the existing located `Type` diagnostic, preserving prior effects. Native
G01 establishes the NaN rejection only: later Inf/logical cases were unreached.
The Rust profile scans each complete nonempty condition for NaN, even after a
zero, as explicit project array policy; empty/all-nonzero truth is unchanged.
Short-circuit operand conversion and other logical operations are unchanged.
`for` over `Mx0` now initializes its loop variable to canonical double `0x0`
without executing its body (G08); `0xN` still executes N times with `0x1` columns
(G07). Empty initialization uses the existing atomic publication, revision and
suppressed-event policy; allocation failure and Stop preserve a complete prefix.

## Opt-in scientific-v1 program API (draft)

`rust_program::scientific::ScientificProgramSessionV1` accepts the same immutable
located `ProgramModuleV1`. `prepare` returns a keyed prepared request; cancellation,
function scopes, branches, loops and atomic caller publications use the same
machine source as the legacy profile. Private profile methods select values and
kernels. Legacy concrete value/task/frame types are retained; no enlarged value
enum is inserted into the old session. Test-only frozen c355 source witnesses
compare layouts, initial reservation formulas, allocation faults, Stop/fuel and
complete effect/value traces. Those tests must pass before asserting equivalence.

The scientific session exposes borrowed `binding(name)` and `bindings()` views.
Values expose full canonical shapes, class and complex flag, plus
`read_range(offset,count)` with complete real/complex/logical borrowed slices.
No mutable workspace reference or fallible workspace enumeration is exposed.

This first profile has rank≤8, ≤2^20 elements/value, ≤16 MiB/value, a 64 MiB
payload budget and 16 MiB visible metadata budget. Maximum request allowances are
5 million control transitions and 1 billion scalar-work units. Source, function,
frame, binding and event caps remain bounded. Actual provider reservations are
charged individually before allocation, with successful capacities checked.
Prepared copy/fill/finite-check futures suspend before ≤128-unit chunks;
arithmetic jobs resume through bounded ≤32768-unit continuations. Binding,
caller-output, display-snapshot and indexed-candidate copies await those same
preparation futures. Primitive checkpoint callbacks also forward
Stop during provider preparation. Budgets do not reset across local calls or
loop statements. Only completed candidates enter the existing publication CAS.
The three stable std source/module/control Arc allocations retain their qualified
global-OOM boundary; fallible provider storage is separate.

Constructors accept dimension arguments or a real row-vector shape.
Trailing-one axes normalize to rank≥2; interior singleton and zero axes persist.
Indexing initially accepts one linear selector or exactly the canonical rank.
Full axes stay symbolic, `end` uses the corresponding extent, and positive
integer vector selectors are checked completely. Fixed-shape writes stage one
candidate and scatter in column-major order; duplicate visits are rightmost wins.
Complex RHS promotes real-double targets; logical class does not silently become
double. Growth, empty-RHS deletion and other-axis collapse are unsupported.

Real/complex N-D element operations, real/imag/conj, 2-D transpose/adjoint,
2-D matrix multiplication and matrix left solve are included. Ordered complex
comparisons are refused. General noninteger powers, matrix right solve, N-D
matrix products/for iteration, sparse storage and classes are not silently
approximated. The legacy exact sum/mean/product proof is not widened; larger
scientific reductions remain separate work. `norm_inf` is a bounded sequential
component-L1 diagnostic, not exact reduction or native bit parity.

Solver stats expose the last attempted factorization's initial/max-U component
metrics, work, completion flag and maximum observed growth, including partial
attempts that fail after construction. Completed solve count is separate.
The original frequency-response example runs actual functions, construction,
indexing, complex solve, residual diagnostics and atomic tensor page writes.
Sparse CSC/Poisson-PCG, classes, broader numerical operations and desktop
integration remain required milestones.

Cooperative execution uses stack-pinned standard Rust futures and a nonallocating
yield primitive, without Box, thread, new dependency or host-stack recursion for
language calls. The internal suspended state is required to fit 1 MiB (checked
by the targeted future-size test); hosts must provide at least 2 MiB available
stack for the core plus their own caller/checkpoint/waker needs. Allocation
system-call latency and arbitrary caller callbacks/wakers remain outside CPU
quantum guarantees. Legacy layout/operation/fault equivalence is an execution
gate, not inferred from the mechanism. Dropping the cooperative future abandons
its key, restores the completed workspace prefix including an empty reserved
binding pool, and discards the deliberately abandoned result/event report.
Request Stop and continue polling to obtain the normal retained-events outcome.
Growth diagnostics preserve exact initial/max-U metric words. Ratio overflow is
reported as hex Inf with an explicit nonfinite/overflow classification and never
fails an otherwise successful solve; zero initial metric is explicitly undefined.

Scientific matrix literals admit at most 256 source rows. This fixed assembly
metadata cap returns Resource before publication when exceeded; it does not
limit the row axis of constructed arrays to 256. Each literal operand shape/class
scan charges one unit and awaits a cooperative checkpoint before inspecting it.


### Scientific V2 program profile

`rust_program::scientific_v2::ScientificProgramSessionV2` specializes the same located-AST machine with typed dense/CSC storage. Every accepted legacy and scientific-v1 public value/session/task layout, operation order, allocation site, quota and schema remains unchanged. The new profile has its own value/task/frame specialization; no storage tag is added to V1. `bindings()` exposes borrowed typed values.

Supported sparse operations are five-argument `sparse(i,j,v,m,n)`, scalar index/value expansion, `issparse`, `nnz`, `nzmax`, `size`, `numel`, and sparse-left/dense-right multiplication. Unsupported operations refuse before publishing a partial result. Bare native constants remain outside this scope; `inf()` and `nan()` provide explicit values for qualified edge fixtures. Sparse function arguments/results and displays use independent copies, with existing caller isolation and publication barriers.

A stack-only operation scope charges every actual provider reservation against64MiB aggregate retained value payload. It includes retained bindings/operands, converted index buffers, constructor permutations and private candidates; replacement and successful output transfer release consumed input/scratch charges exactly once. Error and Future drop destroy child candidates/owned operands before releasing their scope charge. Borrowed operands remain charged until the shared machine releases them. Failed accounting invariants poison the request rather than resetting a counter. Stats report operations, sparse constructors/products, peak operation reservations, peak live payload, primitive work and transitions.

Budgets remain1,000,000,000 primitive units and5,000,000 transitions. Each complete32768-unit operation quantum charges a transition in addition to AST scheduling. New provider/adapter/copy CPU checkpoints suspend in blocks of at most128 charged units; unchanged fallback numeric jobs poll at most128 units apart and yield before/after at most32768 charged units. Existing allocator/system-call and three host-Arc OOM qualifications remain. The private request future is stack pinned, not boxed or threaded; tests bound its size and exercise every suspension drop/Stop and allocation-fault prefix. Actual measured full-workload resource fit remains a validation gate. The 32³ Poisson-PCG workload passed within these bounds; other workloads require their own measurement.

`examples/scientific/poisson_pcg.m` assembles a32³ Poisson matrix from bulk triplets and solves it in a user-defined Jacobi-PCG function, capped at512 iterations. It uses generic numeric/matrix operations, explicit true-residual confirmation, and no production domain builtin or manufactured solution shortcut. Independent tests inspect every223232 CSC entry and32768 returned solution values. Full complex/sparse/N-D/scaling, classes, numerical coverage and desktop integration remain broader goals.


### Class-v1 runtime (opt-in)

The bounded library/CLI profile passed 191 acceptance cases, including 52 class cases and four original-versus-current numeric-profile compatibility cases. Desktop integration is a separate milestone.

The class profile specializes the existing AST/control machine with genuine scalar value-object records and generational handle references. Numeric/dense/CSC properties execute through the accepted scientific-v2 providers. Value assignment and returned value mutators eagerly copy value records/numeric buffers while retaining shared handle identities. Constructors seed their output receiver without adding a visible argument; dot-method nargin includes the explicit receiver. Derived methods resolve against the dynamic class and inherit prelinked properties. No classes are encoded as numeric scalars.

Source modules are an explicit immutable closure. A session fixes ordered class-source identities on first admission, permits new entry text with identical definitions, and rejects changed/reordered definitions before effects until reset. Object IDs validate session owner, reset epoch, slot and generation. Borrowed session views are live; retained event snapshots hold immutable graph/type/identity/validity metadata and remain readable after deletion or session drop. Host-created copies of opaque IDs are not portable strong heap roots.

Private constructor receiver writes do not advance the workspace revision or handle_mutations. An unsuppressed display remains visible and advances the ordinary effect sequence once; semicolons suppress that event. This is a project output policy, not a native constructor-formatting claim. Writes/delete of published shared handles advance revision/effect sequence even inside a method/function which later errors. Property/default preparation is cooperative; candidates and counters precede the no-allocation/no-await publication barrier. The prepared heap version must still match at commit; accounting/version invariant failures poison the session until reset.

The flat heap supports up to1024 records,64 properties each,64MiB payload and16MiB runtime metadata, with unchanged1B/5M work limits. Actual retained vectors and numeric capacities are counted once, including private/deferred garbage. Fixed-stack tracing visits VM/default/private roots; partial GC remains safe on Stop/drop. No host-stack recursive object destruction or COW is used. One bounded Arc-backed private-root registry supplies owned RAII leases across suspended operations; its stable-std allocation/system OOM boundary is explicitly qualified, while all Vec/numeric reserves remain fallible. Class sessions are thread-affine (!Send/!Sync); immutable outcome snapshots and the existing keyed cancellation handle are independent from mutable heap access.

Initial limits are explicit: acyclic objects only; no class arrays, multiple inheritance, superclass-constructor calls/custom-base construction, overloaded operators/custom indexing, user delete/finalizers, access attributes, reflection or validators. Handle identity equality is built in. Handle-valued defaults are cached once per declaring class/session, shared between instances; constructor-created handles remain distinct. `persistent` and char-valued `class()` instrumentation remain Unsupported rather than simulated. These exclusions remain roadmap work, not full class completion.

Numeric properties support whole-value reads and replacement. Chained member-array indexing such as `obj.Dense(1)` and `obj.Dense(1)=value` is not admitted initially; bind the numeric property to a local variable, index that value, then replace the whole property. This is separate from custom object indexing.

Class value-copy traversal uses a lazily fallible, session-owned1024-entry work Vec rather than a large inline async-frame array. Its actual capacity stays charged to metadata across requests, is reused without cumulative reservation, and is dropped by reset/session destruction. Request completion/drop clears traversal IDs; private candidates remain charged in the object heap until safe GC. The1MiB suspended-future assertion does not alone guarantee default-stack execution: constructor, move/pin, polling and synchronous API paths must also pass the unchanged default-stack tests.
