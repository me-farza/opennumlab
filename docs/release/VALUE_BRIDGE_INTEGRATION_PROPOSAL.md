# Value bridge release integration proposal

Status: source-only proposal; implementation and execution are held pending the
session/readiness owner handoff. This document changes no packaging, QA, runtime,
builder or workflow behavior. It authorizes no build, hosted job or publication.

Audited checkpoint: `1d21df82b248eccf7312161023cbfda366ae118d`, tree
`2b7f9bd5fd34269ce276fc17e40c7423d216eb11`. References and line numbers below
describe that committed checkpoint, excluding concurrent working-tree additions.

The checkpoint adds transactional edits and an optional exact dense-complex
adapter. Its source/export and installed-package integration is incomplete.
The smallest implementation should close the three source omissions, carry the
fourth adapter through the existing native package lane, and establish explicit
component declarations and installed evidence. Those changes would support a
bounded Development alpha; they would not complete CP03.

## Exact source closure

`scripts/release-package.py` owns the selector shared by package source,
corresponding-source archives and sanitized public trees. Add only these entries:

| Selector | Missing entry | Why needed |
| --- | --- | --- |
| `SCRIPT_FILES` | `build-value-bridge.sh` | Exported `verify.sh:85` invokes it in the ordinary runtime-enabled lane. |
| `SCRIPT_FILES` | `verify-value-provisioning-test.sh` | Synthetic provisioning contract; reads both builders and `verify.sh`. |
| `DOC_FILES` | `docs/VALUE_EDIT_PROTOCOL.md` | Edit wire, bounds, history and limitations; linked by the exported typed-page documentation. |

The pinned selector includes 208 source files. These three are excluded even
though tracked. Preserve script executable modes. Do not widen the selector to
all scripts/docs, generated `.oct` files, private reports or evidence roots.

Already selected: `native/mathlab_value_bridge.cc`, `native/VALUE_BRIDGE.md`,
`octave/mathlab_value_edit.m`, the new Rust modules and tests, desktop transaction
sources, and `tests/fixtures/class_support/MathLabEditLifetimePeer.m`. Inspected
literal Rust include edges and local C++ headers resolve to selected tracked
inputs. No additional Engine build-script change is needed for this closure.

After a policy change, commit a coherent new source reference before generating
new exports. The exporter must continue rejecting a helper/policy identity that
differs from the requested historical commit. Keep all prior exports unchanged.

## Package and capability contract

| Intended profile | Value component | Acceptance boundary |
| --- | --- | --- |
| Default Development alpha | Explicitly omitted | No exact dense-complex page/edit claim; retain explicit unsupported-operation errors. |
| Existing `--with-debugger` lane, after integration | All four freshly built adapters | Protocol/identity checks plus new installed tests for the declared capabilities; still Development alpha. |
| Complete CP03 | Requires integrated product behavior and broader evidence | A loaded adapter or headless pass alone cannot qualify this milestone. |

Keep the default isolated release assembly and its native omission. Avoid adding
a value-only package switch in this minimal patch. The existing all-native
builder already compiles the value module alongside debug/scope/input; its
`runtime-version.txt` covers that build. The standalone value builder's
`value-runtime-version.txt` exists only in its private temporary directory and is
not a durable package provenance input.

Proposed release-owner edits, after agreement:

1. In `release-package.py:188,225,363`, extend protocol probes, module hashes,
   linked-library inspection and final-byte copying to
   `native/mathlab_value_bridge.oct` →
   `Contents/Resources/native/mathlab_value_bridge.oct`. Update the three-adapter
   help text. Use the fresh frozen-source build output; never recover a module
   from the live checkout or a historical app.
2. Give the value adapter its own probe expression:
   `mathlab_value_bridge('protocol')`. The old adapters use `('protocol',0)`;
   copying that call would fail the new adapter's one-argument contract. Require
   `protocol:"mathlab-value"`, `version:1`, `dense_complex_components:true`,
   `max_cells:2000`, `max_rank:32`, and
   `max_replace_leaf_bytes:134217728`. Retain the complete returned object.
3. Add VALUE → `mathlab_value_bridge` to `release-launcher.sh:59` using its
   existing explicit environment-override semantics. Production provisioning is
   `MATHLAB_VALUE_BRIDGE` or `Engine::enable_value_bridge`; test-only
   `MATHLAB_REQUIRE_*` flags do not enable an adapter.
4. Add an explicit versioned adapter-profile/component declaration to newly
   assembled manifests. Record value inclusion or omission/reason, required
   capability `dense_complex_native_components.v1`, relative installed module
   path, module hash, actual protocol result and runtime/SDK identity. Prefer an
   additive schema-1 extension with focused validation. Missing fields in old
   three-adapter manifests retain their legacy meaning; they must never imply
   enabled value support. If a breaking contract becomes necessary, version it
   explicitly instead of reinterpreting historical manifests.

The package validator and installed acceptance must enforce a declared component:
an absent file, wrong hash, incompatible protocol or missing loaded capability
fails that profile. A path alone is insufficient. `build.native.status` currently
summarizes only the original three probes and cannot establish value coverage.
`atomic_range_edit.v1` describes edit support separately from the optional native
dense-complex precision capability; do not conflate either with CP03 completion.

The existing source inventory already covers the native C++ and embedded worker
sources. CLI `build.rs:24–45` now includes `mathlab_value_edit.m`; its pinned
34-input SHA-256, corrected for Rust `PathBuf` component ordering, is
`035611dba7f0ff9b2151e385c92d955a68cb0a96a028f6d287d27828f4d185a0`.
The original `e6e8b0bb...` audit calculation used the same inputs in incorrect
whole-string order. This annotation corrects that attribution; the original Git
version and retained test evidence remain preserved. Current post-readiness
identity is documented in [the release contract](VALUE_BRIDGE_RELEASE_CONTRACT.md).
That CLI digest does not identify an external native module. Preserve separate
source inventory, executable, module, selected-runtime executable, SDK/compiler
and linked-library identities. Record or reject unreviewed effective Cargo
profile overrides before making an optimized-build claim; the current recorded
configuration omits `CARGO_PROFILE_RELEASE_*` and `CARGO_INCREMENTAL`.

Cargo manifests/lock are unchanged from the preceding `ee67bcca` source snapshot.
The 56 retained notice-file hashes across 29 supplement records also match,
with no unresolved index entries. This establishes notice-input continuity,
not new native linked-library/runtime closure. Keep complete notice generation
and inspect actual fourth-module dependencies when a build is authorized.

## Installed acceptance handoff

These changes require the CLI/session and PM-owned QA surfaces; this proposal
does not edit them:

- `qa-installed.py:633` clears DEBUG/SCOPE/INPUT but leaves an inherited
  `MATHLAB_VALUE_BRIDGE`. Engine startup consumes it at `lib.rs:478`. Clear it
  for every profile, extend the existing environment-leak regression, and set
  only the validated bundled value path when declared. The CLI harness invokes
  the packaged CLI directly, so launcher wiring alone will not provision it.
- CLI doctor currently reports scope/input paths but no value path or negotiated
  capabilities (`main.rs:270`). The session/CLI owner should expose the observed
  value path and current worker capabilities. QA must require the canonical
  installed path and capability after loading; retain the exact runtime
  executable-hash check at `qa-installed.py:498–500`.
- Doctor/protocol success is only a prerequisite. The current CLI has no checked
  page/edit acceptance entry point. Agree an installed test surface with its
  owner that invokes the public checked read/write/undo/redo APIs from identified
  final artifacts. Exercise real dense complex single/double values, component
  bits and declared complex state, stale identity, history, restart, same-process
  Stop recovery, and missing/incompatible-module errors. Do not substitute a
  newly compiled checkout harness or raw Octave calls for installed API proof.
- Report declared profile, actual paths/capabilities, all artifact identities,
  executed cases, unsupported cases and failures. A missing advertised component
  must fail; an omitted optional component must be explicit in the report.

Without an optional helper, keep `MathLab:ValuePagePrecisionUnavailable` and
`MathLab:ValueEditPrecisionUnavailable` for dense-complex operations. A configured
missing module, canonical-identity mismatch or wrong protocol fails startup or
enablement, rather than silently downgrading. Explain the selected path, expected
matching ABI and remedy in prerequisite diagnostics. Retain the canonical-file/protocol loader
checks and their recovery tests; a generic Octave metadata type of `function`
does not prove native identity or close transitive dispatcher/startup issues.

## Verification and execution boundary

The implementation review should cover these source-only changes first:

| Owner/surface | Proposed verification after scope agreement |
| --- | --- |
| Release selector tests | Three exact positive inclusions, nearby negative exclusions, modes and inherited archive/public-tree closure. |
| Release package/launcher tests | Four-module hash/copy/link/protocol records; distinct value probe arity; omission and failure cases; bundled VALUE discovery and explicit override semantics. |
| PM QA tests | Inherited value-path isolation, declared omission versus required inclusion, path/hash/capability mismatch and final runtime identity. |
| Session provisioning | Run the new synthetic provisioning script explicitly; retain its missing-tool, banner/SDK mismatch, build-failure, path-with-spaces and private-output cleanup checks. |
| Native gate | A newly declared required-value suite, with four modules exported/probed/hashed and actual expected test names executed without skips. |

For the native gate, provision `MATHLAB_VALUE_BRIDGE` and set
`MATHLAB_REQUIRE_TYPED_PAGE_TESTS`, `MATHLAB_REQUIRE_VALUE_EDIT_TESTS`,
`MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS` and `MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS` to
`1`. Retain the original session/binding/page suites and add bridge lifecycle,
precision, edit-session and Engine/desktop transaction cases. In particular keep
the original mixed-class tests
`original_complex_single_double_and_dense_logical_values_keep_their_types` and
`complex_single_and_double_preserve_component_class_and_zero_imaginary_state`.
Record a new suite identity and observed count; the historical 27-contract
three-adapter gate is not this evidence. Test deliberately absent adapters with
child-command environment removal, without process-global environment mutation
or weakening the existing cases.

`verify.sh` now checks the runtime/SDK and builds the value adapter before Cargo
whenever a runtime is found. The general Ubuntu workflow currently installs
Octave without its matching `mkoctfile`/development headers. Add the matching
development prerequisite in a separately reviewed workflow change; the pinned
11.3 native container already supplies it. Missing runtime, compiler, SDK match
or nonempty module in a required profile must fail as a prerequisite before
tests, never count as a skipped success. For a requested native package, add the
same inexpensive runtime/SDK preflight before costly Cargo compilation.

Keep the separately observed Octave 8.4 readiness JSON failure open until the
readiness owner integrates and validates its fix. The CLI spawn guard in this
checkpoint makes an executable spawn error explicit; it does not eliminate the
existing version-profile skips or prove required 8.4 startup compatibility.
Update setup/release guidance and PM-owned installed-acceptance documentation
with the agreed optional/required scope. Do not dispatch CI to test speculative
integration during the owner's serial build lease.

Complete CP03 additionally needs wired range selection/Paste/Undo/Redo actions,
real clipboard/keyboard/stale-state journeys, installed/native GUI acceptance,
broader complex/cancellation/receipt-loss variants, resource evidence and the
shared dispatcher/startup/recovery boundary. The committed protocol document
explicitly says those desktop actions are unwired. Its reported headless results
must retain their original source identity and scope.

This audit used committed-source reads only: no tests, native/runtime calls,
builds, exports, GUI actions or hosted jobs. Historical artifacts and evidence
remain unchanged. APFS/cache and hosted macOS packaging proposals remain held
under their existing runtime-identity, nested-session cleanup and quota gates.
