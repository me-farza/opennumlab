# Compatibility evidence

The manifests define runnable **smoke tests**, not a count of MATLAB support. Each fixture uses original source and assertions against a known result, algebraic identity, or round-trip equality. The command runs each unique fixture once and associates its result with the feature rows that reference it.

```sh
cargo run -p mathlab-cli -- check compatibility/core.json
cargo run -p mathlab-cli -- check compatibility/control.json
cargo run -p mathlab-cli -- check compatibility/toolboxes.json
```

The optional control manifest requires a separately installed Octave `control` package. The optional toolboxes manifest requires `control`, `signal`, `image`, `statistics`, and `optim`, with their declared dependencies such as `datatypes`/`struct` where required by the selected releases. It checks only the named numerical cases. The graphics fixture requires an installed graphics toolkit and fonts. A failed or unavailable prerequisite must remain visible in the report.

The core manifest also covers selected value/handle class operations, inheritance,
dependent/private properties, object arrays, nested closures, function arity and
typed numerical indexing. The script-local fixture runs through the MathLab
preparation API; executing it directly in an unmodified Octave process is a
different path and is not equivalent evidence.

`status` records the declared evidence level:

| Status | Meaning |
| --- | --- |
| `targeted` | A fixture exists; this row does not claim a recorded passing run. |
| `verified-octave` | A recorded Octave run passed the stated fixture on the identified version/platform. |
| `unassessed` | The feature has not been assessed. |

Passing the command does not upgrade the manifest automatically. Keep the command's report with the runtime version, platform, and commit before changing a status. Even a `verified-octave` row does not mean all variants of the API match MATLAB. It only describes that fixture.

These fixtures have not been compared with a licensed MATLAB reference environment. The following remain separate workstreams: all specialized toolboxes, complete `classdef` behavior, MATLAB graphics callbacks and properties, `.mlx`, `.mlapp`, `.fig` interchange, `.slx`/Simulink, state machines, physical modeling, MATLAB MEX binary compatibility, GPU/parallel behavior, code generation, deployment, and MAT 7.3 writing. See the development plan and functionality catalog for the complete intended scope.

## Measured language and type gaps

The following behavior was observed on GNU Octave 11.3.0, macOS arm64, on October
14, 2026. These findings limit compatibility claims even when the core fixtures
pass. They have not been independently compared against a licensed MATLAB build.

| Area | Observed behavior | Work needed |
| --- | --- | --- |
| Property validation | Class property size/type/validator syntax produces a warning, and an invalid negative value can be accepted despite a positive-value validator. | Implement or reject unsupported validation precisely; parsing alone is insufficient. |
| Class events | Event declarations parse, but `notify` is unavailable. | Event dispatch, listener lifecycle, access rules and tests. |
| Enumerations | Enumeration class syntax fails to parse. | Compatible enum parsing, values, dispatch, serialization and introspection. |
| String literals | Double-quoted literals remain `char`, including after loading `datatypes` 1.5.0. | MATLAB-mode typed string semantics and tested literal parsing. |
| String/native API bridge | An explicit package `string('abc')` value fails when passed directly to native `fprintf`. | Audited formatting and data API conversions that preserve types and missing values. |
| Script introspection and resources | The native pre-entry/source-entry path reflects scripts and outer/nested local code to the original path and exact source snapshot. Checked saved/unsaved cases use original `mfilename('fullpath')` to load project resources; outer `functions(handle).file` reports the original `.m` file. The interpreted fallback can still expose generated paths. | Extend ABI/platform evidence and help/source/introspection coverage; retain distinct native/fallback claims. |
| Nested-handle file metadata | On the audited Octave version, `functions(nested_handle).file` is empty although the closure's `mfilename`, resource lookup and error filename identify the original source. Already-created handles retain cached file strings. | A compatible public metadata mechanism and MATLAB-reference evidence without replacing or changing the closure. |
| Local persistent state after path changes | The native implementation retains the genuine unlocked registry outside the load path and binds retained per-file local objects. Enabled regressions cover paths, packages, edits, late activation, interruption and variable clearing. Ordinary `clear functions`/`clear all` remove the helper and reset state. Without native binding, path changes can still rebind named private lookup while old handles keep prior state. | Broaden native ABI/platform validation and retain distinct enabled/fallback evidence. |
| Prepared source debugging | Real native regressions cover saved/unsaved script, local/nested and cold old/new handle pauses with original-source mapping. Ordinary function/class buffers require saving first. Source snapshots preserve executing-generation text; logical breakpoint groups track relocations and removals across retained generations. The desktop shows a paused arrow only when the active editor exactly matches the executing snapshot. Files registered directly with the native debugger lack immutable snapshots. | Extend ABI/platform/MATLAB-reference validation. Raw `dbstatus` exposes helper metadata; an evaluation combining clear, ordinary breakpoint installation and an old-handle call can precede reconciliation. See the [checked contract](../docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior). |
| Internal registration identifier | Registration now happens in worker scope before source entry, without an injected call or protocol variable. Script-local `mathlab_scope_register` and same-name base handles no longer intercept registration. The interpreted fallback uses a return-only, canonical-identity-checked factory. A foreign global transport-helper/factory definition is rejected rather than replaced or executed. | Keep collision/recovery regressions and precise global-shadow diagnostics. The earlier [collision reproduction](../docs/KERNEL_READINESS.md#registration-name-collision) is historical evidence, not a current local/base reservation. |

Current native capability gates require `mathlab-scope` version 2 with `pre_entry`,
`original_source_reflection`, `source_entry`, `stable_registry` and `off_path_registry`, and
`mathlab-debugger` version 4 with `source_entry`, `stable_registry`,
`breakpoint_source_metadata`, `off_path_registry`, `streamed_evaluation` and
`input_wait_cancel`. Authentic input requires `mathlab-input` version 1 with
`original_builtin`, `caller_scope`, `separate_reply` and `genuine_reads`.
Older binaries lacking the required capabilities fail before activation. A handshake
does not establish compatible native ABI or MATLAB behavior. Missing adapter
environment variables make optional test entry points return early; that run
does not validate the native features.

The last complete enabled suite recorded 204 Rust tests with all three adapters,
including authentic input and quiet/live paused evaluation. Earlier pre-entry,
reflection, off-path registry, snapshot and logical-clear checks remain local
evidence is recorded in `artifacts/engine-registry-tests.log` and
`artifacts/verification.log`, with repeated initial-breakpoint proof in
`artifacts/ast-clear-registry-separation-tests.log`. Available generated evidence
is copied into local development source packaging; a clean checkout does not
require it to build. Named regressions and reproduction commands are listed in
the [kernel readiness assessment](../docs/KERNEL_READINESS.md).

The installed datatypes package exposes explicit `string`, `table`, `datetime`,
`categorical`, `timetable` and `readtable` APIs after loading. Their availability
does not establish complete API compatibility, and this package is not loaded
automatically by the engine.

To add evidence, use a small deterministic fixture, identify the exact API variants, check meaningful outputs with appropriate numerical tolerances, and record provenance for any externally contributed test. Avoid asserting on the exact printed floating-point format when the mathematical result is what matters.

## Shared typed conformance routes

The separate `conformance` command executes original shared fixtures through raw
Octave and the actual `Engine::run_file` path. MATLAB execution is explicit and
optional; the command never discovers or starts MATLAB automatically.

```sh
cargo run -p mathlab-cli -- --json conformance compatibility/conformance/common.json
cargo run -p mathlab-cli -- --json conformance compatibility/conformance/common.json --matlab /absolute/path/to/matlab --output reference.json
cargo run -p mathlab-cli -- --json conformance compatibility/conformance/common.json --reference reference.json
cargo run -p mathlab-cli -- --json conformance compatibility/conformance/common.json --reference reference.json --strict
```

The requested profile pins Octave 11.3.0 and MATLAB R2026b. Each route records
runtime identity and typed outcomes; comparisons preserve class, shape, storage,
complex components, exact integer strings and declared floating tolerances.
Independent typed oracles check values even when two routes share the same record
writer. The driver stages the captured fixture/support bytes privately before
execution and records their SHA-256 hashes. Binary identity and captured build
source metadata are separate from the repository revision observed at runtime.
Enabled adapter files also record sampled hashes; these are not native build
provenance. Reports and saved-reference inputs are bounded to 16 MiB.

No reference means `unavailable` and an `incomplete` report, never a MATLAB match.
Saved replay validates profile, fixture/support hashes and successful reference
provenance; known synthetic test reports are rejected. Saved JSON alone does not
establish MAT-file interchange. The `mat_v7` route embeds the bounded, hashed
MATLAB-written MAT artifact and replays those actual bytes into both application
routes; without it, only independent self-round-trips are reported. Reports are provenance records rather than signed
attestations of a third party's claimed execution. Mismatches, invalid references
and unexpected execution errors return a nonzero exit; an explicitly incomplete
run with unavailable/unsupported routes can finish successfully as a report.
Use `--strict` for acceptance gates: it also fails an incomplete report after
publishing the route evidence. Unexpected execution deadlines always fail.
The independent research bundle's `process-report.json` can also be imported
with `--reference`. It retains its separate producer identity and validates
inventory, selected source/oracle bytes, records and MAT artifacts. Source bytes
come from the bundle's `snapshot/` or the supplied conformance directory, and must
match the original inventory. Importing and re-replaying it never becomes a new
MATLAB execution. Saved reports remain unsigned provenance inputs.
See [the original fixtures](conformance/README.md) for declared variants, independent
oracles, the forward-local raw-Octave boundary and the observed sparse-logical MAT gap.
