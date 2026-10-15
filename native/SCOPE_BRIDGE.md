# Optional native script-local scope adapter

`mathlab_scope_bridge.cc` is an original GPL-3.0-or-later optional adapter using publicly
installed GNU Octave interpreter headers. It addresses a measured Octave behavior:
private function lookups can rebind after a load-path change, resetting persistent
variables even when another handle still owns the previous function instance.

The adapter prepares and installs retained **function objects** as local bindings
keyed by exact source filenames. Optional original-source metadata changes the
display filename without changing those lookup keys. Numerical local names never
become global. The genuine internal registry helper has an unlocked command-line
binding to prevent path-cache reloads; this preserves ordinary function/all clear
lifetimes. The adapter does not change base variables, rewrite user identifiers,
or lock numerical functions with `mlock`.
Octave continues to evaluate function bodies, arguments, workspaces, indexing,
exceptions, nested functions, and variable precedence.

## API and optional activation

Build with the `mkoctfile` belonging to the runtime that will load the module:

```sh
bash scripts/build-debugger.sh
MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  cargo test -p mathlab-engine --test octave_session
```

The build script builds both native adapters. Set `MATHLAB_OCTAVE` and, when
needed, `MATHLAB_MKOCTFILE` to select the matching runtime/SDK. A scope-only build
can use `mkoctfile native/mathlab_scope_bridge.cc -o native/mathlab_scope_bridge.oct`.
An application can call `Engine::enable_scope_bridge(path)` explicitly; the
environment variable enables it when the engine starts. The selected path is
available through `Engine::scope_bridge_path()` and persists across restart.

Validate the protocol before using a discovered binary:

```matlab
metadata = mathlab_scope_bridge('protocol', 0);
assert(strcmp(metadata.protocol, 'mathlab-scope'));
assert(metadata.version == 2);
assert(metadata.pre_entry && metadata.original_source_reflection && ...
       metadata.source_entry && metadata.stable_registry && metadata.off_path_registry);
```

This query reports `mathlab-scope` version 2 with `pre_entry`,
`original_source_reflection`, `source_entry`, `stable_registry` and
`off_path_registry` capabilities,
and performs no symbol-table
installation. The engine validates the protocol before enabling scope
binding. Without activation it uses the interpreted registry fallback. A requested
missing, incompatible, or older adapter returns a diagnostic; it does not silently
claim that native binding was enabled. An already selected adapter path cannot be
replaced in that session; restart MathLab to choose a different module.

Prepare a new generation before entering its script:

```matlab
retained_local_handles = mathlab_scope_bridge('prepare', ...
    exact_generated_source_path, {'counter','helper','make_nested'}, ...
    original_source_path, original_full_source);
```

The final two arguments are optional. The adapter parses the generated script and
each exact `private/name.m` file using public `parse_fcn_file`; it executes no user
statements. It validates names and capsule membership, marks the outer functions
private, and returns public scoped handles to those objects. It installs only
filename-keyed local bindings. No static native function cache owns these handles.
The inactive runtime registry must retain the first returned cell for each immutable
generation and reuse it until ordinary function/all clear or session restart.
Repeated `prepare` calls create new objects; preparing again is not how persistent
state is reused.

The worker establishes the registry helper before configuring or looking it up:

```matlab
mathlab_scope_bridge('registry', exact_canonical_registry_helper_path);
```

The resolved user-function scope must match that exact `mathlab_scope_register.m`
file. A shadowed function is rejected without replacement. The public
`symbol_table::install_cmdline_function` call reuses the current genuine helper,
including its persistent keys, and prevents it from disappearing through ordinary
path-cache refresh. Only this existing transport helper receives a command-line
binding; numerical local functions remain filename-scoped. The helper is unlocked
and is removed by ordinary `clear functions`/`clear all`, then a fresh helper is
established on the next request. The module owns no static helper/function cache.

The engine stores this helper in a dedicated `session/registry` folder. The
interpreted fallback appends that folder to the load path. Native activation
preserves the genuine helper object and its persistent keys, clears only its
on-path cache slot, and removes that helper-only directory from the load path.
After ordinary function/all clearing, native registration parses the exact helper
file without adding its folder back. This avoids a measured Octave lookup path:
class-constructor probing precedes command-line lookup and can refresh a duplicate
on-path helper, erasing the host's breakpoint-table membership while retained
alias AST marks remain. Keeping a single native command-line owner prevents that
cache conflict. A visible foreign global helper is rejected even when the genuine
command-line helper already exists; the adapter neither executes nor replaces it.

Rebind the retained generation on each entry:

```matlab
mathlab_scope_bridge(exact_generated_source_path, retained_local_handles, ...
                     original_source_path, original_full_source);
```

The final two arguments are again optional; the legacy two-argument binding form
is supported by this version. The optional return value is the number of locals.

For actual scope-only script execution, use the source-entry boundary:

```matlab
mathlab_scope_bridge('source', exact_generated_source_path, ...
                     retained_local_handles, original_source_path, original_full_source);
```

It parses the executing script AST first, validates and installs the exact retained
handles after parsing, reflects script/local metadata, and calls the AST at the
base frame. An unwind guard restores the caller frame and debug state; ordinary
errors and interrupts propagate. The script AST is temporary, with no native
owning cache. Empty handles support a plain script without local definitions.
The ordering keeps bindings attached to the actual executing AST rather than
depending on a separate `source()` call's parsing/cache decisions. The
debugger uses its own cached script AST and the same shared validation/reflection/
installation helpers in [`mathlab_source_context.h`](mathlab_source_context.h).

`exact_generated_source_path` includes the `.mlsource` extension and is
canonicalized by the adapter. `mfilename('fullpath')` strips the extension; passing
that value alone is incorrect. `retained_local_handles` is a cell array such as
`{@counter, @helper, @make_nested}` containing named outer local functions from
the corresponding generated `private` directory. Anonymous/nested handles,
built-ins, foreign definitions, duplicate names and invalid input types are
rejected before any bindings are installed. Membership is checked through the
immutable `scope().fcn_file_name()`, never the reflected display filename.

Each local name is installed for the generated script **and every outer local
function's source file**. The latter keeps sibling calls bound to the same
objects. Nested function scopes stay attached to their existing outer function;
the adapter never extracts or reconstructs a dynamic nested closure.

The engine registry retains its first handles for each immutable source
generation and passes those retained handles to the adapter at each script entry.
The native `prepare` operation permits registration from the worker before user
source entry, without injecting a helper identifier into that source. Engine code
must retain and reuse unchanged
`.mlsource` and private files; edits get a new generation. `clear functions` and
`clear all` clear Octave's local bindings and the interpreted registry naturally.
The worker keeps the selected adapter path outside the base workspace and restores
autoload/configuration before the next execution after those operations. This
adapter adds no locking exception. Clear Workspace (`clear -variables`) retains
the registry and local state; `clear functions`/`clear all` reset that state.

## Original-source reflection

With the optional original path/text arguments, public
`octave_user_code::stash_fcn_file_name` and `cache_function_text` update outer
locals and their nested/subfunction user-code metadata. The original path must be
absolute; it may name an unsaved, nonexistent file. The text is the exact original
snapshot, not a reread of the latest on-disk file. The parse timestamp and immutable
generated scope filename stay unchanged, preserving old-generation lookup and
cached source lines.

Outer and nested `mfilename('fullpath')` then identify the original project file,
so relative resources based on that folder can be loaded. Outer scoped handles
created after reflection report the original `.m` file through `functions`; the
five-argument `prepare` form reflects before constructing its returned handles.
An already-created handle keeps its cached file string: later metadata changes
cannot repair that field through a public setter.

Nested closure execution, resource lookup and error filenames reflect the
original source, but `functions(nested_handle).file` remains empty on the audited
Octave version. Its public nested-handle constructor does not initialize that
cached string. The adapter preserves the closure rather than replacing it to
alter introspection. The source-entry command also reflects the executing script's
own `mfilename` and cached source. A binding-only call changes extracted functions,
not a separately parsed script. The
[public handle constructors](https://docs.octave.org/doxygen/11/d0/d92/ov-fcn-handle_8cc_source.html)
show the distinct scoped and nested metadata behavior.

## Measured proof on Octave 11.3.0

Raw interpreter calls and actual MathLab CLI sessions first proved the binding
mechanism. The prototype CLI proof used prepared script/private extraction and a
same-line binding call. The adapter is now integrated into the engine's runtime
registry and has a separate real-session regression:
`optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears`
in [`octave_session.rs`](../crates/mathlab-engine/tests/octave_session.rs).
That regression covers path changes, installed package loading, variable clears,
edited-generation switches, sibling/nested scopes, base-variable precedence,
function/all clears, selected-path retention, replacement rejection, and restart.
Fallback tests explicitly check different expected named-call behavior when this
capability is disabled.

The version 2 public pre-entry/reflection proof also checks a real on-disk source
edit after a timestamp advance, distinct old/new counter generations, cached old
and new source lines, original outer/nested resource paths, outer handle filenames,
an original error line, a nonexistent unsaved source path and hidden global names.
An ordinary inactive registry proof checks repeated state, variable clearing,
function clearing and all clearing. A buffered native trace reproduced the real
registry-loss failure without output before the reset: the same generation was
prepared a second time, with a new function pointer, although the script scope
key exactly matched its installed bindings. This isolated helper lifetime from
numerical lookup and justified retaining only the genuine internal helper.
The registry-lifetime proof checks path/variable preservation, normal function/all
clear reset, the exact `which` path and the absence of locking. The integrated
engine regressions also check pre-entry helper-name/base-variable collisions,
original script/outer/nested resource paths, retained nested error names and
named state after original-file edits and a cache-refresh timestamp advance:

- `pre_entry_registration_does_not_reserve_user_local_or_base_handle_names`
- `optional_scope_pre_entry_reflects_original_paths_resources_and_retained_generations`
- `reflected_caught_error_frames_preserve_retained_nested_generation_names`
- `optional_scope_reflection_keeps_immutable_state_after_cache_refresh_and_original_file_edits`
- `optional_native_registry_late_activation_preserves_state_and_rejects_global_shadowing`

Source-entry proofs additionally check an unsaved plain script's original
filename, project resource and base assignments with no local handles. The
independently reproducible proof sources
are documented in
`/private/tmp/mathlab-native-preregister-proof/README.md`; these temporary proofs
are separate from packaged engine regressions.

The same original file was edited and restored interactively between requests:

| Operation | Observed local counter |
| --- | ---: |
| First script execution, with no user-returned handle | 1 |
| Repeat unchanged source | 2 |
| Add an empty directory to the load path, then repeat | 3 |
| Load the signal package, then repeat | 4 |
| Clear workspace variables, then repeat | 5 |
| Edit the same original file to a new initial state | 101 |
| Restore the original source generation and call by name | 6 |
| Call a retained handle from that original generation | 7 |
| `clear all`, then repeat the original source | 1 |
| `clear functions`, then repeat | 1 |

Additional actual CLI assertions passed:

- A local function calling a sibling retained the counter through a path change;
  its returned handle continued from the same state.
- A returned nested closure retained its captured state independently from a
  newly created closure (`12` versus `11`).
- A base-workspace `counter = @() -9` took precedence over the local function;
  clearing that variable exposed the existing local counter state again.
- Direct command-window calls to the local names failed. `exist(name,'file')`
  returned zero for the local names; registry keys/handle arrays never appeared
  in the base workspace.
- Missing source paths returned `MathLab:ScopeBridgeFile`; protocol inspection
  returned the expected metadata without creating a local/global binding.

These are Octave execution proofs, not comparisons against a licensed MATLAB
reference runtime or a claim of complete MATLAB language conformance. Optional
tests require the compiled adapter setting; a fallback test run does not validate
the native capability.

## API basis and limits

The adapter uses public `parse_fcn_file`, the scoped `octave_fcn_handle`
constructor, `interpreter::get_symbol_table`, `symbol_table::install_local_function`,
`octave_fcn_handle::fcn_val` / the public function-value accessor, and borrowed
`octave_value` reference ownership.
[Octave's function-info source](https://docs.octave.org/doxygen/11/d6/d07/fcn-info_8cc_source.html)
shows separate filename-keyed local bindings and directory-keyed private caches,
with scoped local lookup preceding private lookup. Installed `symtab.h` and
`fcn-info.h` expose the installation operation directly. The
[public parser source](https://docs.octave.org/doxygen/11/d5/d60/oct-parse_8cc_source.html)
documents exact-file parsing and function metadata setters.

No private member access, patched interpreter, global dispatch hook, or replacement
of numerical operations is used. This is still an Octave C++ API adapter: compile
it for the selected runtime, test that runtime, and do not assume binary ABI
compatibility across Octave releases. The original-source arguments address the
selected local-function reflection cases above; the adapter does not emulate all
help/source enumeration, nested-handle file metadata or MATLAB
string/property-validation semantics. Without those arguments, generated display
filenames remain in use. Prepared debugger mapping/AST execution
uses the separate `mathlab-debugger` version 3 adapter. Its enabled engine tests
cover saved/unsaved script, local/nested, and cold retained-generation pauses,
clear/lifetime behavior, and parse/Quit recovery. Raw debugger introspection and
the single-evaluation clear boundary remain documented in the
[checked debugger contract](../docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior).

The engine retains immutable generation files until restart/drop. The registry
retains scoped handles until function/all clearing or session restart; user
handles can additionally retain their own function objects. Generation retention
currently has no generation/disk/RAM budget. Original-source text is cached on each reflected
outer/nested function object, adding memory cost for large many-function scripts.
That cost and repeated metadata refresh are not yet benchmarked. See the
[kernel readiness assessment](../docs/KERNEL_READINESS.md)
for resource, transport, figure-lifetime, and performance boundaries.
