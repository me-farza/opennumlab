# Kernel readiness

Audit date: October 14, 2026. This document describes the implemented repository
interfaces and their evidence. It is a development readiness assessment, not a
MATLAB compatibility certification or a performance comparison. Local runtime
checks use GNU Octave 11.3.0 on macOS arm64. Test totals are deliberately omitted:
optional adapters, packages, and the selected runtime change what actually runs.

The foundation is usable for trusted, local scientific scripts, matrix inspection,
and deterministic batch fixtures. It is not a complete MATLAB replacement or a
Rust numerical interpreter. New product work can build on the session API while
keeping the execution backend and the compatibility boundaries explicit.

The later [workspace protocol](WORKSPACE_PROTOCOL.md) adds ordinary capability
negotiation, exact binding-context counters, safe metadata and revision-checked
primitive inspection/editing. Object dimensions/bytes can be explicitly unknown;
listing the workspace must not invoke an overloaded `size`. Its implementation
and validation checkpoint is separate from this October 14 baseline. Atomic
range transactions, general resource accounting and interactive graphics remain
unfinished. The later optional [typed-page service](VALUE_PAGE_PROTOCOL.md) adds
lossless bounded N-D reads and genuine cell/struct paths for declared primitive
leaves. Its exact-value fixtures, page measurements and frontend checks qualify
that read subset separately; they do not complete all CP03 data tools.

## Architecture and ownership

| Component | Actual responsibility | Source |
| --- | --- | --- |
| Rust engine | Start/own one Octave process; publish requests; decode typed results; stream merged console bytes; enforce transport limits; manage cancellation, restart, source generations, and original-source error mappings. | [`Engine`, `EngineConfig`, `ExecutionResult`, `CancellationHandle`](../crates/mathlab-engine/src/lib.rs), [console transport](../crates/mathlab-engine/src/console.rs) |
| Rust language adapter | Lex MATLAB-shaped source, recognize script-local blocks, extract private definitions, preserve source positions, classify script/function/class files, and materialize immutable source generations. It does not evaluate expressions or implement MATLAB array semantics. | [`prepare_script`, `PreparedScript`, `SourceKind`, `FileMapping`](../crates/mathlab-language/src/lib.rs), [lexer](../crates/mathlab-language/src/lexer.rs) |
| Octave worker | Execute source in the actual base workspace; keep protocol variables in function scope; inspect variables; edit validated numeric elements; enumerate packages; capture output/errors; export live figures. | [`mathlab_worker`, `mathlab_workspace`, `mathlab_read_variable`, `mathlab_figures`](../octave/mathlab_worker.m) |
| GNU Octave and installed libraries/packages | Parse/evaluate native source; own values, function scopes, class dispatch, numerical algorithms, file readers/writers, and graphics objects. Their code and dependencies are not implemented in Rust. | [Runtime selection and startup](../crates/mathlab-engine/src/lib.rs), [package/setup policy](../scripts/setup-runtime.sh) |
| Optional C++ scope adapter | Parse locals before source entry, install retained objects in per-file symbol maps, reflect original source metadata, and stabilize the genuine internal registry. It does not publish numerical local names globally or rewrite calls. | [`mathlab_scope_bridge`, `install_local_function`](../native/mathlab_scope_bridge.cc), [shared source context](../native/mathlab_source_context.h), [scope protocol](../native/SCOPE_BRIDGE.md) |
| Rust debugger client and optional C++ debugger adapter | Separate control channel, actual interpreter-thread inspection/evaluation, revision-checked stepping/resume, breakpoint binding, and prepared-source mappings. These depend on the selected Octave ABI. | [`Debugger`, `SourceMapping`](../crates/mathlab-debugger/src/lib.rs), [`debug_events`](../native/mathlab_debug_bridge.cc) |
| Rust input state and optional C++ input adapter | Publish genuine reads, validate generation/request identities, accept replies independently of the busy execution owner, and delegate evaluation to the original builtin in its caller's scope. | [`InputHandle`, `InputRequest`](../crates/mathlab-engine/src/input.rs), [native input contract](../native/INPUT_BRIDGE.md) |
| Rust CLI and desktop | Consume kernel results and control operations. They do not supply a second numerical implementation. | [CLI](../crates/mathlab-cli/src/main.rs), [desktop crate](../crates/mathlab-desktop/) |

Each `Engine` owns a temporary session directory, process, merged console reader,
bounded subscriber queues, monotonically increasing request identifiers, and source-generation cache.
Calls through `&mut Engine` are sequential; independent engines can run separate
sessions. `CancellationHandle` and `Debugger` clones provide control while the
execution owner waits on a different thread.

Rust atomically renames a JSON request into `request.json`. The worker polls the
mailbox while idle and writes bounded structured response files. A standalone Rust
helper emits atomic Begin/End/Recovery fences into the same pipe as child stdout
and stderr. A fresh nonce identifies each process generation, and invocation IDs
distinguish same-process interrupt recovery. Ordinary source travels as
data; file paths are structured request fields. Stdin is reserved for worker
re-entry after an interrupt, so user `input()` cannot consume the next mailbox
command. This is process isolation and crash containment, not a security sandbox:
scripts retain the interpreter's filesystem, process, network, and extension
privileges.

## Current session contract

- `execute` evaluates source in the persistent base workspace. `run_file` reads
  saved source and `run_buffer` stages an unsaved snapshot without overwriting the
  original file. Relative API paths resolve against the session's last reported
  current directory; running a script does not implicitly switch to its folder.
  Debugger callers should use the returned `PreparedDebugSource.original_path`:
  the engine resolves an unsaved file's existing parent, while the debugger client
  does not independently normalize symlink aliases of nonexistent files.
- `ExecutionResult` contains output, optional structured `EngineError`, workspace
  metadata, figure paths/warnings, elapsed milliseconds, and the resulting current
  directory. User source failures are results, not successful computations:
  callers must inspect `result.error`. Process, JSON/protocol, timeout, and API
  failures return Rust `Error`. Lexical preparation failures also return
  `Error::Worker` with `MathLab:SourcePreparation`; native pre-entry parse failures
  are structured source errors in `ExecutionResult`.
- A user error leaves assignments made before the error intact. Mapped errors
  identify the original prepared source and line; the copied worker is filtered
  by exact path, so a user file named `mathlab_worker.m` is not hidden by basename.
  Mapping returned errors is distinct from the optional native original-source
  reflection described below; it does not establish complete `dbstack` or
  introspection compatibility.
- On Unix, `CancellationHandle::interrupt` sends SIGINT. The worker's
  `unwind_protect` emits a recovery response and Rust re-enters the same process.
  Partial assignments and already emitted console bytes survive. A separate
  Recovery fence prevents a late interrupt from replacing an already completed
  result. Cancellation and normal completion synchronize through the process
  mutex; a cancellation marker covers requests not yet consumed, and a ping
  checkpoint settles a sent SIGINT before completion is returned. The interrupt
  response does not export figures. Foreign native routines can defer interrupt
  handling. Native Windows engine startup is explicitly unsupported pending
  validated helper dispatch and cancellation.
- `terminate`, automation deadlines, and `restart` discard the runtime workspace.
  Restart clears prepared source generations and reinstalls configured adapters.
  Cancellation clones target the replacement process through shared ownership;
  debugger clones refer to the old private directory and must be reacquired.
- `clear` means base `clear -variables`, not a session restart or a removal of
  local persistent definitions. Native `clear functions`/`clear all` reset the
  inactive local-handle registry. Adapter setup is restored before subsequent
  execution when configured.

The optional input adapter is enabled before the first user execution through
`Engine::enable_input_adapter` or `MATHLAB_INPUT_BRIDGE`. Its cloneable
`InputHandle` exposes a reliable pending snapshot and accepts replies independently
of the execution owner. The full generation/invocation/request/input identity
rejects stale and duplicate replies. A request remains Submitted until the
original builtin consumes the line or EOF. Its Finished event precedes expression
evaluation, so nested input obtains a fresh identity. Recovery, restart and
disconnect invalidate pending state. Without the adapter, `input()` can block
until cancellation instead of receiving a frontend reply. Late activation is
rejected because original `@input` handles may already have escaped.

The desktop uses a separate reply field and preserves its command draft/history;
the CLI uses one stdin router for both replies and REPL commands. Replies are
single UTF-8 lines up to 1 MiB, prompts at most 64 KiB, and EOF is distinct from
an empty line. Native echo is disabled for terminal input to avoid double echo.
The original Octave builtin owns caller-scope evaluation, string values, output
arity and errors. Its invalid-expression behavior differs from MATLAB's retry
loop; EOF also retains Octave's original error. No prompt-text heuristic creates
an input request. See [native input](../native/INPUT_BRIDGE.md).

Paused debugger evaluation has normal mutation privileges and can enter another
breakpoint. User evaluations stream output while automatic previews use quiet
capture; the returned copy captures the first 1 MiB of raw output, with bounded UTF-8 replacement. Both paths evaluate directly in the
paused frame without `evalc`, whose saved pager buffer is invalidated by this
runtime's `input()`. A debugger-client timeout alone does not terminate execution.
An independent control worker keeps the desktop event loop responsive. Actual
Stop after input consumption ends the enclosing execution with
`MathLab:Interrupted`, releases the pending evaluation and preserves the base
workspace. Explicit Quit uses `MathLab:DebugQuit`. Native probes cover waiting,
entered-expression and nested-watch cancellation in quiet/live modes, including
real cleanup and no continuation. The [desktop acceptance log](../artifacts/desktop-app-stop-test.log)
and [native Stop report](../artifacts/debug-input-stop-report.json) record these
distinct checks. Exact pending reply files are removed when a read finishes or
is revoked; stale replies cannot accumulate through normal restart/cancellation.

Console subscriptions remain live while the execution owner waits and while the
worker is idle, including across process restart. Output preserves the byte order
accepted by the common pipe, without later prepending or sorting. Background
writers and user-selected buffering do not have a universal statement order or
proven request ownership. Each subscriber retains at most 1 MiB of output bytes
and 256 queued events; eviction produces an explicit loss marker without blocking
computation. The legacy aggregate retains at most 32 MiB of raw bytes and reports
truncation and decoding loss. Lossy UTF-8 conversion can expand that aggregate's
text allocation. Desktop scrollback separately caps retained text at 16 MiB and
4,096 entries, and completion waits for its public console sequence barrier.
The CLI streams raw bytes in human-readable modes and keeps JSON output clean.
The actual contract, historical probes, and remaining validation limits are
recorded in [Console protocol](CONSOLE_PROTOCOL.md).

These behaviors are exercised by named integration tests in
[`octave_session.rs`](../crates/mathlab-engine/tests/octave_session.rs), including
`persistent_workspace_executes_matrix_code_and_keeps_protocol_variables_private`,
`errors_preserve_console_output_and_session_remains_usable`,
`interruption_preserves_workspace_and_allows_fresh_requests`,
`deadlines_and_hard_termination_allow_a_clean_restart`, and
`clear_all_and_multiple_variable_types_do_not_break_worker`.

## Source scope and persistence

`prepare_script` uses a lexer and block stack, rather than global identifier
replacement. It recognizes quoted text, comments, transposes, continuations,
delimiters, indexing `end`, control blocks, and nested functions. Ordinary saved
function/class files are left to Octave's parser. Scripts with local definitions
become a unique non-autoloadable `.mlsource` plus padded `private/name.m` files.
The generated parent is appended to the load path; the private directory is never
added directly. Local definitions retain their original line positions.

An exact original-path/source pair reuses its immutable generation. Edited source
gets a new generation; retained user handles keep the older definition. The
transport helper `mathlab_scope_register` owns first-generation local handles
outside the base workspace, including when the user retained no handle. Registry
setup and handle lookup occur in the worker before entering user source. No
registration call or temporary protocol variable is injected into the script.
Returned numerical local names do not become globally callable, and base
variables retain native precedence.

The interpreted fallback obtains private handles through a generated, return-only
factory whose body constructs a handle cell without calling the numerical
functions. The worker verifies that ordinary global lookup resolves to the exact
canonical factory path before invoking it; a foreign same-name factory is rejected
without execution. Native scope preparation bypasses the factory and parses the
exact private files directly. Malformed local definitions become structured source
errors before script statements execute, and the next request remains usable.

The Rust/private-file fallback has a documented deviation: a load-path mutation,
package load, or same-name function in another generation can reset subsequent
named local lookup while old handles retain their prior state. Optional
`mathlab_scope_bridge` installs the retained objects for the script and each
private function file on entry, using Octave's public `install_local_function` API.
That stronger behavior is covered separately by
`optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears`;
fallback tests explicitly assert the weaker behavior when the adapter is absent.
Neither mode silently changes user call identifiers.

With a native adapter enabled, the genuine registry receives an unlocked
command-line symbol binding through public `install_cmdline_function`. Its dedicated
`session/registry` directory is removed from the native load path and its duplicate
on-path function slot is cleared. After a normal clear, native setup parses that
exact helper file directly. The interpreted fallback alone keeps the helper directory
on the path. This keeps the same registry and retained handles across native
path-cache refreshes, including late adapter activation. Only the
internal helper receives that binding; numerical functions remain per-file locals.
`clear -variables` preserves native retained state, while ordinary `clear functions`
and `clear all` remove the helper and reset local persistent state. The next request
establishes a fresh helper. No `mlock`, static native owner of numerical functions,
or exception to normal function/all clear lifetime is added. The current enabled
engine regressions cover this lifetime path, as recorded below.

Native source entry reflects the executing script and its outer/nested local
user-code metadata to the original absolute path and exact source snapshot. The
immutable generated scope filename and parse timestamp remain unchanged. Checked
saved and nonexistent unsaved source cases use original `mfilename('fullpath')`
inside scripts, outer locals and nested closures to load a project resource.
Outer scoped handles report the original `.m` path through `functions(handle).file`.
Old handles keep their old numerical body and cached source when the original
file is edited or another generation becomes current. These are selected native
reflection cases, not a complete help/source/introspection implementation.

Boundaries remain:

- Without the native reflection/source-entry path, prepared script/function
  introspection can still reveal generated contexts and resource folders. The
  fallback does not promise the native reflection behavior.
- On the audited Octave version, `functions(nested_handle).file` is empty even
  though that closure's `mfilename`, resource lookup and error filename identify
  the original source. Its public nested-handle constructor leaves that cached
  string empty; the adapter preserves the closure rather than replacing it.
  Already-created handles likewise retain their cached file strings. Complete
  `dbstack`, `localfunctions`, help/source enumeration and MATLAB reflection parity
  remain unassessed.
- User-issued Octave `run`/`source` calls and selection/section execution bypass
  whole-script preparation. There is no general interception of every load path.
- Local class definitions and local functions inside control blocks are rejected.
  Explicit local-function ends are required. `restoredefaultpath` or user removal
  of generated parents can disrupt fallback private resolution.
- `prepare_debug_file` and `prepare_debug_buffer` stage/register source without
  running user statements. `SourceKind` rejects unsaved function/class debugger
  buffers with a save-first diagnostic. Saved function/class files continue to
  use native Octave breakpoint resolution; prepared scripts use the optional
  debugger path described below.

The language evidence is in [preparation tests](../crates/mathlab-language/tests/preparation.rs),
[real runtime tests](../crates/mathlab-language/tests/octave_local_functions.rs), and
engine tests for source mapping, same-name isolation, immutable preparation,
persistent state, clear operations, and prepared-script interruption.
The pre-entry/reflection regressions include
`pre_entry_registration_does_not_reserve_user_local_or_base_handle_names`,
`pre_entry_local_parse_errors_return_execution_errors_and_keep_the_session`,
`fallback_factory_identity_guard_rejects_shadowing_without_executing_foreign_code`,
`optional_scope_pre_entry_reflects_original_paths_resources_and_retained_generations`,
`reflected_caught_error_frames_preserve_retained_nested_generation_names`,
`optional_scope_reflection_keeps_immutable_state_after_cache_refresh_and_original_file_edits`,
and `optional_native_plain_buffer_entry_uses_original_script_path_and_resources`.

## Checked prepared-source debugger behavior

With both optional adapters enabled on the audited runtime, real engine tests
pause in saved and unsaved script statements, forward local functions, and nested
functions. Continue crosses these scopes and retained old/new source generations,
including handles whose functions had never been called before debugger setup.
Reported pause filenames and stack names map to the original script and its local
or nested function. An unsaved plain script executes its registered snapshot;
preparing a snapshot does not execute its statements or overwrite the saved file.

The debugger caches script ASTs in `debug_events::prepared_sources`, while the
ordinary inactive registry owns the numerical function objects. Public
`symbol_scope::install_subfunction` aliases under that real helper let Octave's
public breakpoint table retain breakpoint membership after Continue. These aliases
do not alter the functions' source scopes/names or publish numerical function names
globally. `clear functions` and `clear all` release the registry and reset local
persistent state; the debugger's script cache does not keep that state alive.
The debugger establishes the same genuine unlocked command-line registry
binding before configuration or host lookup, including from a paused user's local
scope. Exact helper identity checks reject a foreign global definition rather than
replacing it. This is separate from allowing a script-local helper or same-name
base variable, which no longer intercepts an injected call.

`source_prepared` calls the prepared script AST at the base frame. While prepared
points are configured, `evaluate_base` explicitly enters that frame and uses
public `interpreter::eval_string`, avoiding the extra `evalin` boundary that was
observed to miss retained-handle pauses. Both paths restore the caller frame and
debug state through an unwind guard. Engine regressions verify a parse error leaves
the process, workspace, and breakpoint usable, and a requested Quit preserves prior
assignments, skips subsequent statements, and permits the next request. Ordinary
execution errors and interrupts retain their separate propagation paths.

Native `dbclear all`, host-specific clear, and individual frontend removal are
honored without silently recreating cleared points on the next script entry.
`synchronize_prepared` also reconciles marks before each execution request, so a
later direct old-handle call does not encounter a cleared mark even when an
ordinary file breakpoint is active. After registry-clearing operations, configured
points can bind to fresh objects while local persistent state starts again.

The Rust client tracks confirmed breakpoint installations in logical groups. Each
group retains its pinned generation targets and updates its visible lines after
runtime relocation. Clearing a current frontend point removes the group's marks
from all retained generations; a newly added point at an older historical line
gets its own group. The checked relocation case moves one point twice and clears
it while preserving that independent historical-line point. Failed clears retain
tracking records for retry. This does not intercept arbitrary user breakpoint
commands outside the documented reconciliation boundaries.

`DebugState.runtime_file` retains the actual scope identity before original-path
mapping. `SourceMapping.snapshot_path` identifies the exact full immutable source;
`Debugger::source_snapshot(runtime_file)` returns a bounded UTF-8 `SourceSnapshot`
without evaluating code. Old/new handle regressions check the respective full
source text. The desktop accepts snapshot replies only for its current paused
revision and shows a green paused-line arrow only when the snapshot belongs to
the active original source and its text equals the editor buffer. Loading, missing
or unreadable snapshots hide the arrow; errors stay visible. An edited buffer gets
a warning and a bounded read-only executing-source context. Ordinary native
saved files registered directly with the debugger, including scripts and
function/class files, have no immutable snapshot through this API, so the desktop
reports that limitation instead of using a clean/saved flag to infer identity.

The checked contract still has two debugger-specific boundaries:

- Raw Octave `dbstatus` describes the registry host and its helper/subfunction
  metadata. The frontend's original-source mapping is authoritative; arbitrary
  debugger introspection is not rewritten.
- Reconciliation occurs at request/script-entry boundaries. A single user
  evaluation that performs `dbclear all`, installs an ordinary breakpoint, and
  calls an old handle can run before another reconciliation. This combined case
  is not covered by the next-request clear guarantee.

The evidence is in [`octave_session.rs`](../crates/mathlab-engine/tests/octave_session.rs):
`optional_prepared_debugger_breaks_in_saved_and_unsaved_script_local_and_nested_sources`,
`optional_prepared_debugger_maps_retained_handles_without_debugger_warmup_calls`,
`optional_prepared_debugger_ast_cache_does_not_prevent_function_and_all_clear`,
`optional_prepared_debugger_honors_dbclear_all_and_registry_clear_lifetime`,
`optional_prepared_debugger_host_clear_preserves_ordinary_and_individual_breakpoints`,
`optional_prepared_debugger_synchronizes_cleared_old_handles_before_new_requests`,
`optional_prepared_handle_quit_preserves_process_workspace_and_next_command`, and
`optional_prepared_debugger_parse_error_preserves_workspace_and_breakpoints`.
The current pre-entry/snapshot/clear extensions add
`optional_prepared_debugger_handles_user_registry_name_and_base_variable_collisions`,
`optional_prepared_debugger_single_original_clear_removes_all_retained_generation_marks`,
and `optional_prepared_debugger_clear_tracks_two_relocations_without_clearing_independent_historical_line`.
`optional_native_registry_late_activation_preserves_state_and_rejects_global_shadowing`
starts a child with the scope environment variable removed. It verifies existing
fallback state survives either native activation order, same-process SIGINT recovery
keeps the helper off the path, a prepared breakpoint still pauses afterward, foreign
file and command-line definitions are rejected without execution, and normal
function clearing resets local state. The formerly intermittent initial script
breakpoint test passed 20 consecutive runs after isolating the native registry
from ordinary on-path lookup; the unchanged clear reconciliation still passes.
These checks establish selected Octave behaviors, not MATLAB debugger parity.

## Variable, file, and figure transport

`workspace` returns names, classes, dimensions, byte counts, and bounded previews;
it does not transfer full arrays. `read_variable` returns at most 2,000 formatted
display cells with one-based coordinates. N-D values show their first plane.
Numeric/logical 2-D matrices, including sparse matrices, support literal element
edits; cell/struct/char previews are read-only. Arbitrary class objects, tables,
and specialized package values do not have a general Variable Editor protocol.

`set_variable_element` validates identifiers and parses scalar components without
evaluating the supplied literal. Bounds, dimensions, and destination class are
preserved. Decimal int64/uint64 literals avoid conversion through double. These
are meaningful editing guarantees, not a lossless bulk-data interface; displayed
floating-point cells are strings with formatting precision.

File I/O currently runs through Octave source commands, not a Rust format layer.
The [MAT fixture](../tests/fixtures/mat_roundtrip.m) checks Version 7 round trips for
selected numeric, complex, logical, sparse, char, cell, struct, and uint8 values.
The engine test checks the MAT5 header and numerical readback. This does not prove
MAT 7.3 writing, every MATLAB object/container, or `.mlx`/`.mlapp`/`.fig`/`.slx`
interchange. There is no independent MATLAB-reference reader validation here.

The worker keeps Octave's figure objects alive and exports every open figure to a
request-specific PNG after execution. Renderer failures become `figure_warnings`
and do not erase numerical results. PNG paths remain valid while the engine owns
its temporary directory; clients must copy an image for durable export. The CLI's
`preserve_figures` does so. Closing a figure removes it from future results. The
[graphics fixture](../tests/fixtures/graphics_core.m) also checks selected line
properties and SVG export. Previews do not implement interactive graphics object
transport, callbacks, hit testing, zoom/edit event routing, or MATLAB `.fig` parity.

## Reliability and performance work still required

| Finding | Evidence and implication | Practical next change |
| --- | --- | --- |
| Console framing and interruption require explicit identity and commit rules. | Structured metadata now uses separate files; ordinary JSON-looking markers remain console bytes. Nonce/generation/invocation fences separate normal completion from recovery. A late Recovery preserves an earlier committed End, and old helper fences cannot reopen an earlier invocation. Malformed current-identity frames terminate the transport. | Retain boundary-race, partial-output, background-writer, slow-subscriber, and restart regressions. Framing is not a security boundary against trusted source. |
| Resource bounds cover transport, not execution memory or session disk use. | Inline/editor source is limited to 8 MiB; encoded response metadata and captured raw output each to 32 MiB. Subscriber queues and UI text are independently bounded. Four control slots can retain captured results; lossy UTF-8 expands invalid bytes. Workspace count, open figures, retained generations/handles, and saved PNGs are not budgeted. `run_file` also reads source before any editor-size limit. | Add observable usage and explicit limits; retain source generations safely while handles live; prune obsolete previews with a documented client lifetime. Do not evict live function definitions blindly. |
| Figure snapshots add unmeasured cost to every execution. | `mathlab_figures` exports all open figures at 120 dpi each time; numerical duration includes workspace inspection and export, not just evaluation. Restart removes prepared source directories but does not remove prior PNGs. | Measure/export changed figures selectively; report evaluation, metadata, export, and transport time separately. |
| Source bookkeeping grows with edit history. | `prepared_sources` retains full source snapshots and mappings. Cache lookup compares path/full source across the vector; error mapping scans retained mappings. Native scope registration installs each local for each source context, and reflection caches the full original text on outer/nested user-code objects. These copies are not budgeted. | Benchmark long editor sessions and many-local-function scripts, then index snapshots and mappings without losing generation identity. |
| Protocol handshake is not broad ABI/platform validation. | Scope protocol 2 requires `pre_entry`, `original_source_reflection`, `source_entry`, `stable_registry` and `off_path_registry`. Debugger protocol 4 also requires `breakpoint_source_metadata`, `streamed_evaluation` and `input_wait_cancel`. Input protocol 1 requires `original_builtin`, `caller_scope`, `separate_reply` and `genuine_reads`. Older/incompatible binaries fail before activation. Selected adapter paths persist across restart. Native headers and compiled modules still depend on the runtime ABI. | Validate the runtime/build matrix and package selection explicitly; report adapter capability/version diagnostics. |
| Cancellation does not imply arbitrary foreign-process cleanup. | Engine ownership and force stop operate on the Octave child; foreign calls can defer SIGINT. There is no general child-process tree or untrusted-code resource policy. | Test native/subprocess cancellation, process-tree cleanup, and recovery on each supported platform. |
| Native debugger coverage is optional and runtime-specific. | The enabled macOS arm64 suite passed the current prepared-source, off-path registry, late activation, interruption, and clear cases. Tests return early without adapter environment variables; configured Linux CI runs `verify.sh` without `--with-debugger`. | Add a supported native CI lane with matching `mkoctfile`, plus explicit enabled/skipped capability reporting. |
| Core language availability exceeds established compatibility. | The compatibility manifests explicitly use an Octave smoke-test reference. Measured gaps include typed string literals/native formatting, class validation, events, and enumerations. | Preserve precise unsupported diagnostics and obtain release-specific differential evidence before broadening claims. |
| Internal transport names still share the global function namespace. | Pre-entry registration removed the reproduced local/base helper collisions. Exact registry and fallback-factory identity checks reject foreign global definitions before invoking them; they do not provide a fully private global namespace. | Keep collision and recovery regressions and precise diagnostics; assess a separate namespace mechanism if global coexistence is required. |

There is no measured MATLAB speed advantage, Rust numerical backend, or JIT in
this implementation. Numerical speed comes from the selected Octave build and
its libraries; Rust orchestration does not automatically accelerate algorithms.
JSON metadata, formatted variable cells, polling, full figure export, source
preparation, short fence-helper processes, and process startup can add overhead. Bulk numerical data should not
be routed through formatted cells when a binary/file/shared-memory contract is
eventually introduced.

### Registration-name collision

The previous implementation invoked `mathlab_scope_register` through a source
prefix and ordinary language lookup. In that historical reproduction, the
following otherwise unused local function intercepted the internal call and the
script failed before assigning `result`:

```matlab
result = helper(3);
function y = helper(x)
  y = x * 2;
end
function mathlab_scope_register(varargin)
  error('Audit:RegistryCollision', 'User local intercepted registration');
end
```

The earlier audit saved this source in
`/private/tmp/mathlab-registry-collision/local_collision.m` and ran it through
`mathlab run ... --workspace`; the engine reported `Audit:RegistryCollision` at
the original function line. That temporary proof is not a packaged core fixture;
the inline source makes the boundary reproducible in any project.

A second historical proof assigns
`mathlab_scope_register = @(varargin) error('Audit:BaseRegistryCollision', 'User base handle intercepted registration');`
in a script containing a local helper. Running that same script twice through
one CLI REPL previously succeeded once, then failed during the second registration prefix.
Its audited path is `/private/tmp/mathlab-registry-collision/base_collision.m`.
These paths are historical temporary evidence, not required fixtures. The current
worker registers before entering user source and the prepared script contains no
prefix call, so these local/base names are no longer reserved by registration.
`pre_entry_registration_does_not_reserve_user_local_or_base_handle_names` and
`optional_prepared_debugger_handles_user_registry_name_and_base_variable_collisions`
retain regression coverage, including a real paused user local with that name.

A same-name foreign **global function file** remains a different boundary. Native
registry setup validates the exact resolved helper and rejects a foreign object
without replacement. It also rejects a visible foreign on-path file with that name,
even when a genuine command-line definition already exists. The interpreted fallback checks the internal helper and generated
factory's canonical identity before a call; its factory-shadow regression checks
that the foreign body does not execute. A diagnostic in this global case must not
be described as restored local/base collision behavior or general namespace parity.

## What can be built on now

The session API supports CLI jobs and a responsive local desktop with command
history, unsaved script execution, bounded matrix pages, literal edits, figures,
explicit package loading, and Stop/Restart controls. Algorithms can be delivered
as independently tested `.m` modules or curated existing packages. Their tested
signatures, types, dependencies, and limitations should remain visible. Existing
array/class/function, dense/sparse linear algebra, FFT, and MAT/graphics fixtures
are useful regression foundations, not a percentage of MATLAB functionality.

The next kernel stages should introduce session resource accounting,
extend native ABI/platform validation, and cover the remaining reflection
and global transport-namespace boundaries. Then add representative project fixtures and
performance measurements, followed by a lossless bulk-data contract and audited
file/type extensions. A Rust value/runtime or numerical backend should enter
behind a documented execution contract with semantic and numerical checks; it
must not silently replace established behavior to obtain a faster benchmark.
Simulink/model execution, GPU/parallel execution, code generation, deployment,
and specialized toolbox equivalence remain separate major implementations.

## Reproduce the evidence

The audit reran real fallback engine session tests with all native adapter
variables unset. That run exercises the nonoptional session, source, variable,
MAT, PNG, error, cancellation, and deadline checks; optional test entry points
return early and must not be counted as native validation. The stdout-marker
failure and its subsequent fix were reproduced through an independent actual
`Engine` harness; the subprocess text is now returned as output and the following
request remains aligned.
The audit also enabled only the compiled scope adapter and reran
`optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears`;
that actual native integration passed. An earlier enabled engine run supplied both
compiled adapters and passed its actual session suite, including saved/unsaved
and cold retained-handle pauses, clear/lifetime behavior, parse/Quit recovery,
SIGINT, and framing regressions. This is separate from a fallback run whose
optional entry points return early. The earlier scope version 2/debugger version 3
suite, with required `off_path_registry`, passed after the pre-entry,
reflection, registry isolation, immutable snapshot and logical-clear changes.
Recorded local command output is `artifacts/engine-registry-tests.log` and
`artifacts/verification.log`; the repeated initial-breakpoint proof is
`artifacts/ast-clear-registry-separation-tests.log`. Current input/debugger evidence
uses scope 2, debugger 4 and input 1. The [validation record](../artifacts/VALIDATION.md)
separates targeted runs, full enabled/fallback verification and package checks.
Local source packaging includes available generated records; a clean checkout
does not require them to build.

```sh
env -u MATHLAB_SCOPE_BRIDGE -u MATHLAB_DEBUG_BRIDGE -u MATHLAB_INPUT_BRIDGE \
  cargo test -p mathlab-engine --test octave_session --locked
cargo test -p mathlab-language --locked
./scripts/verify.sh --require-octave
env -u MATHLAB_DEBUG_BRIDGE -u MATHLAB_INPUT_BRIDGE \
  MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  cargo test -p mathlab-engine --test octave_session \
  optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears --locked
```

Use the matching runtime SDK and execute enabled adapter checks, without assuming
the fallback run tested them:

```sh
bash scripts/build-debugger.sh
CARGO_INCREMENTAL=0 \
  MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  MATHLAB_DEBUG_BRIDGE="$PWD/native/mathlab_debug_bridge.oct" \
  MATHLAB_INPUT_BRIDGE="$PWD/native/mathlab_input_bridge.oct" \
  cargo test -p mathlab-engine --locked
./scripts/verify.sh --require-octave --with-toolboxes --with-debugger
```

[Verification script](../scripts/verify.sh), [configured CI](../.github/workflows/ci.yml),
[compatibility evidence policy](../compatibility/README.md), and
[compatibility/core.json](../compatibility/core.json) define what the commands
actually cover. Keep runtime/platform/build details with recorded results and
refresh this assessment when contracts or checks change.
