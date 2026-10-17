# Dense complex component adapter

The development source adds `mathlab_value_bridge.cc` for exact access to dense
complex single/double components in an Octave-backed session. This interim C++
adapter does not implement the required Rust execution engine.

Octave can narrow a selected complex singleton or all-real region and lose an
imaginary negative zero. The adapter reads native storage before that selection
and returns separate real arrays. Replacement constructs a private native leaf;
the worker still owns binding validation, history and the single base assignment.

## Development status

The four-adapter build script compiled the owned adapter with Octave 11.3 on
macOS arm64. The combined source passed 404 workspace test entrypoints, including
exact dense-complex read/write/undo/redo, startup/restart/Stop, failed-loader
recovery and desktop transaction routing. The follow-up report (local evidence unavailable in source exports: `../artifacts/cp03-followup-validation/RESULTS.json`)
retains exact inputs, executables and results. Earlier isolated loader probes
reject foreign M-file and native-module paths before protocol entry and include
ordinary-call and configured-module positive controls.
No installed application, full CP03, performance or general memory claim follows.

The retained native probe report (local evidence unavailable in source exports: `../artifacts/cp03-value-bridge-extensions/report.json`)
identifies the exact private module, runtime, commands and results. Its baseline,
original probe and extension probe all exited successfully; all three retained
the same shutdown diagnostic, including the baseline without the module.

## Build and configuration

For development, build against the exact runtime's `mkoctfile`:

```sh
MATHLAB_OCTAVE=/absolute/path/to/octave-cli bash scripts/build-value-bridge.sh
```

`MATHLAB_MKOCTFILE` can explicitly select the compiler. The script compiles into
a fresh private build directory and atomically replaces the finished module,
preserving any older inode mapped by an existing interpreter. Its actual selected
build is retained in `artifacts/cp03-current-validation/native-value-build-01.json`.
The updated all-four builder's actual run is
`artifacts/cp03-followup-validation/native-build-all-four-02.json`.

`scripts/verify.sh` resolves one runtime and requires its matching development
SDK before running Cargo. It builds and exports the value adapter, and enables
the required typed-page, edit, precision and desktop actor test profiles.
`--with-debugger` builds and exports all four adapters. Runtime/SDK versions must
match exactly; explicit missing tools, unknown banners and version mismatches
fail before Cargo. Both observed architecture-bearing and legacy version banners
have synthetic coverage. The matching 11.3 all-four build ran successfully;
an end-to-end `verify.sh` invocation was not part of this follow-up.

The development session interface uses `MATHLAB_VALUE_BRIDGE=/absolute/path/to/mathlab_value_bridge.oct`
or `Engine::enable_value_bridge`. Installation checks the exact resolved canonical
`.oct` path and protocol before advertising
`dense_complex_native_components.v1`. Restart and same-process Stop recovery must
reestablish that capability. A retained configuration path alone is insufficient.
Changing the module path requires a new engine session.

An actual probe corrected the loader's metadata expectation: genuine `__which__`
reports type `function` on Octave 11.3 before and after native loading, rather than
the human-readable `dynamically-linked function` label. The generic label alone
does not prove native identity. The transitive dispatcher/startup defect remains
open as described in [the workspace qualification](../docs/WORKSPACE_PROTOCOL.md#dispatcher-identity-qualification);
the isolated path-gate probes do not close that separate boundary.

## Native operations

| Operation | Result and bounds |
| --- | --- |
| `protocol` | `mathlab-value`, version 1, exact limits and the dense component capability |
| `read_components` | Fresh `{real,imag}` arrays of the original precision and selected rows × columns; no full-source component decomposition |
| `replace_components` | Private full leaf plus fresh bounded before/after component pairs; no binding, revision or history mutation |

Selections contain one native subscript per actual axis, rank 2–32, at most
2,000 selected cells and indices below 2^53. Replacement admits at most 128 MiB
of logical complex leaf data before native copy-on-write. Genuine primitive
native type IDs are checked before dimensions, extraction or conversion.
Objects, sparse/diagonal leaves, implicit casting and shape expansion are outside
this native API. Sparse pages/edits retain their separate existing route.

Dense complex history keeps the separate selected real/imaginary arrays and
serializes whole bounded real arrays through integer bit representations.
It must not recreate selected complex values just to capture undo data. The
worker's existing encoded history charge and transaction ceilings still apply;
they are not allocator or peak-RSS measurements.

## Required qualification

Actual checks must cover native metadata and shadow rejection before callbacks,
clear-functions/all lifetimes, startup/restart/Stop, failed readiness after a
committed edit, exact singleton/all-real and nested container write/undo/redo,
untouched quiet-NaN payloads and aliases, missing-adapter rejection, empty sources,
admission/cancellation, and the existing real/sparse/debugger/input regressions.
Required profiles must build and explicitly enable the matching module rather
than silently skip existing mixed-class tests. The follow-up ran the original
public typed-page/binding/session/input targets and current workspace check,
Clippy and all compiled test targets with the matching four-module profile.
Setup guidance, CI development-SDK dependencies, native release gates, source
allowlists, package/launcher provisioning, broader resource variants and installed
qualification remain open. See the release-owner handoff (local evidence unavailable in source exports: `../artifacts/cp03-value-provisioning-release-handoff.json`).
Octave 8.4 and 11.3 have separate ABI/runtime gates; the separate 8.4 readiness
producer correction remains pending. These 11.3 results do not qualify another
runtime or complete every case listed above.
