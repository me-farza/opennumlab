# MATLAB script local functions

`prepare_script(path, source)` recognizes MATLAB script-local function definitions
using a lexer and a block stack. It leaves ordinary function and class files
unchanged. For scripts, it removes each outer local function by replacing its
bytes with whitespace and creates one padded function source per definition.
Comments, character/string literals, transposes, matrix/cell expressions, indexing
`end`, continuation lines, multiple outputs, control blocks, and nested functions
are handled without rewriting calls or identifiers.

`PreparedScript::source_kind()` uses the same lexer to report `SourceKind::Script`,
`Function`, or `Class` from the first executable token, skipping leading comments
and a UTF-8 BOM. Extracting local functions does not turn a script into a function
file. The engine uses this classification when an unsaved debug buffer must be
saved before native function/class breakpoints can be bound.

`PreparedScript::write_to(unique_session_directory)` writes a uniquely named
`.mlsource` script and `private/name.m` function files. Octave may otherwise parse
a same-named original in the current directory while resolving private scope.
Octave's `source` accepts non-`.m` source files, while load-path function lookup
ignores them: the generated script cannot be called globally by its name. The
engine retains that script directory at the end of Octave's load path. The
private directory itself must **never** be added to the path.
All files must survive unchanged until session shutdown so returned `@local`
handles can still call their siblings and local `persistent` state survives
repeated script execution. Every returned `FileMapping` points a
generated filename to the original source; line and column numbers are retained
by padding. Materialized paths are canonicalized to match Octave stack frames,
which may resolve `/tmp` to `/private/tmp` on macOS. Reuse the same generated
directory for unchanged source; new source versions need new directories to
preserve existing handles. Removing the parent from Octave's path or deleting and
recreating a cached script can evict its private scope, resetting local
`persistent` state on a later dispatch. Keep cached `.mlsource` files immutable;
do not delete/recreate them between requests or add artificial `mlock` calls.

Octave can also release private function scopes when their last handle disappears.
The engine therefore registers local handles in an inactive transport helper
outside the base workspace before executing the script. It does not inject a
registration statement or temporary variable into user source. The portable
backend uses a return-only, uniquely named factory `.m` in the generated capsule,
verifies the factory's exact canonical identity before calling it, and retains the
returned private handles. This internal factory can appear in raw global lookup;
user local numerical names remain private. The optional native adapter creates
scoped handles through exact private-file parsing without a factory call.
The registry retains each generation's first bindings until `clear functions`,
`clear all`, or restart; clearing workspace variables alone preserves state.
`scope_registration_offset()` remains a lexical helper for callers, but the engine
no longer uses source insertion. Full original snapshots are retained separately
as `.mlsnapshot` files for exact old/new debugger source display.

This strategy gives forward definitions, separate local-function workspaces,
local precedence over same-name global functions, base-variable precedence, and
script-specific private scopes. Actual Octave tests cover two saved originals
with the same basename, retained handles after path restoration, siblings,
nested closures, persistent state, and hidden generated script names. The default
fallback uses Octave's existing private-function resolution. The optional native
adapter adds explicit per-file local bindings; both strategies retain the original
user call identifiers and keep local names out of global lookup.

## Optional native scope bindings

The integrated [`mathlab_scope_bridge`](../../native/SCOPE_BRIDGE.md) binds the
registry's retained function objects to the canonical `.mlsource` script and
every extracted outer function file through Octave's public
`symbol_table::install_local_function` API. This avoids the private-cache
rebinding deviation after load-path changes, package loading, or switching source
generations. Sibling calls and nested closures keep their original scopes; base
variables retain their native precedence.

Build using the SDK belonging to the selected numerical runtime:

```sh
bash scripts/build-debugger.sh
MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  cargo test -p mathlab-engine --test octave_session
```

`scripts/build-debugger.sh` currently builds both native adapters. The engine
activates scope binding through `Engine::enable_scope_bridge(path)` or
`MATHLAB_SCOPE_BRIDGE` at startup. It validates the `mathlab-scope` version 2
protocol and its pre-entry/reflection/source-entry capabilities, retains the selected adapter path across restart, and reapplies runtime
configuration after `clear functions`/`clear all`. Those clear operations still
reset local persistent state; Clear Workspace preserves it. Without explicit
activation the interpreted registry fallback remains in use. A requested invalid
adapter produces a diagnostic rather than silently claiming native behavior.
The native adapters retain the canonical internal registry in Octave's public
command-line function table, preserving its cells through path-cache refreshes
without `mlock`. Ordinary `clear functions`/`clear all` removes it; the worker
reinstalls/configures a fresh helper on the next entry. A foreign same-name global
helper is rejected, and no local numerical function name is installed globally.

Real engine regressions distinguish these modes explicitly and check optional
native named-call state through path changes, installed package loading,
edited-generation switches, variable/function clears, sibling calls, nested
closures, base-variable precedence, and restart. These are tested Octave cases,
not comparisons against a licensed MATLAB build. Build and test each runtime ABI.

Current limitations:

- With the native scope adapter, outer/nested function `mfilename`, source-derived
  relative resources, outer `functions(handle).file`, and cached function text use
  the original path/snapshot. Immutable lookup keys remain generated. Native nested
  `functions(handle).file` remains empty. Without the adapter, function introspection
  exposes generated paths. Error frames are mapped conservatively when a caught
  reflected stack lacks enough generation/name information.
- Native source entry reflects script `mfilename` and source-derived resources to
  the original path; the interpreted fallback exposes the generated basename/path.
  Session implementation directories appear in `path()` so private function caches can survive. User
  `restoredefaultpath` or removal of those directories resets the caches and may
  prevent later local calls in the same script from resolving.
- In the interpreted fallback, changing the load path (including adding another generated source generation)
  can cause Octave to rebind a later named private-function lookup to a fresh
  instance. Its persistent state then restarts even though a retained old handle
  keeps its original state. The registry fixes consecutive runs and workspace
  clearing; it does not emulate MATLAB's full lookup behavior across path changes.
  The optional native scope adapter addresses the checked named-call cases.
- In the interpreted fallback, relative resources obtained from a local function's
  `mfilename('fullpath')` need an explicit original project path.
- Local classes and function definitions inside control blocks are rejected.
  Scripts require explicit ends for every local function. Ordinary function
  files keep Octave's own parsing behavior.
- Octave `localfunctions` reports function-file context, so script-local
  `localfunctions` enumeration and native help/reflection metadata are not
  emulated here. Prepared debugger source mapping uses a separate optional
  adapter. Its enabled engine tests cover saved/unsaved script, local, nested,
  and cold retained-generation pauses; error-frame mapping alone does not enable
  it. Raw `dbstatus` still exposes registry-host metadata. See the
  [checked debugger contract](../../docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior).
- Direct Octave `run`/`source` calls made by user code bypass engine preparation;
  only files submitted through the MathLab execution API receive this adaptation.
- Worker and registry helpers remain internal global runtime functions; a foreign
  global file shadowing the canonical helper is rejected. User local functions and
  base function-handle variables named `mathlab_scope_register` no longer intercept
  execution entry. Explicitly clearing the internal registry helper resets its
  retained bindings.

These checked adaptations do not establish complete MATLAB language equivalence.
See the
[kernel readiness assessment](../../docs/KERNEL_READINESS.md) for session resource
budgets, output/figure lifetimes, and current native-debugger boundaries.

Reference behavior: [MATLAB script local functions](https://www.mathworks.com/help/matlab/matlab_prog/local-functions-in-scripts.html)
and [Octave private functions](https://docs.octave.org/latest/Private-Functions.html).


## Rust literal-expression component

The explicit `literal-expression-v1` library component keeps the original UTF-8 source and byte spans while parsing signed integer spellings, parentheses, rectangular comma/semicolon matrix literals, `[]`, and binary `+`. `expression::lower_literal_expression_v1` produces admitted IR; `literal_evaluation::evaluate_literal_expression_v1` consumes that IR and returns an owned double array with its original AST/source. It composes the `mathlab-array` constructors and addition without invoking Octave.

This first profile admits integer magnitudes through 2^53 with at most 16 digits, one physical code line with surrounding line comments, arrays up to 16 by 16 and 256 elements, and the other resource bounds in the N01 contract. Project syntax/resource diagnostics and host allocation failures are distinct; leaf errors preserve their original context and code. These are project rules for an incremental language component, not MATLAB's complete syntax or native error identifiers.

The Engine, desktop and CLI still use their existing execution path. This library is not an application backend selection. Its original fixture predictions require affected Rust checks; fresh MATLAB source oracles and full N01/R03/K04/N03 acceptance remain separate. See `docs/research/matlab-r2026b/N01_LITERAL_EXPRESSION_HANDOFF_V1.md` at the repository root for the exact contract.
