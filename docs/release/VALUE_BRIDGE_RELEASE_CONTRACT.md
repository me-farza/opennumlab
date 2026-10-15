# Value adapter package contract, version 1

The source draft passed 95 local synthetic Python tests, provisioning contracts
and shell syntax; see the [validation receipt](VALUE_BRIDGE_SYNTHETIC_VALIDATION.md).
No real build, runtime invocation or hosted job validated these changes.
Installed acceptance and complete CP03 remain pending.
The earlier proposal's `1d21df8` inventory and CLI digest are
historical. The readiness baseline was re-audited below; no new export was made.

The immutable post-readiness baseline is
`904fe17b252fa270200416ef5d5b0c0d32ae145e`, tree
`06b18887010b60a006fe2885da4bbd688d05a5ea`, including readiness checkpoint
`e1ae304`. Applying the prospective selector to its raw committed blobs selects
217 files, versus 214 with its old committed selector. The exact additions are
the two provisioning scripts (0755) and `docs/VALUE_EDIT_PROTOCOL.md` (0644).
This calculation predates this contract document and is not a policy-matched
export receipt. All 12 inspected literal Rust include paths close, including the
new readiness helper and producer fixtures. The CLI's actual build-script digest
now has 35 inputs:
`cb7073ed1e45cdad085432e5769d5daf45e3ac674764f595a3034f77463983ee`.
The algorithm sorts Rust `PathBuf` components: for example,
`conformance/reference_bundle.rs` precedes `conformance.rs`. The earlier
`0dd7b29b...` calculation used the same bytes in incorrect whole-string order;
it was not the CLI build-script digest. The correction is documentary only.
That digest identifies this pinned CLI input set, not the whole source inventory
or an external native module. Re-pin again if owner integration changes those
inputs before a package/export is produced.

This is an additive contract inside schema-1 `release.json`. Only an absent
`build.native.adapter_profile` means legacy. Null, malformed, extra-key or
unknown-version/name declarations fail validation. JSON duplicate keys fail.

New default builds declare:

```json
{
  "status": "not_included",
  "adapter_profile": {"schema_version": 1, "name": "native-omitted-v1"},
  "value_component": {"status": "not_included", "reason": "native_profile_not_requested"},
  "reason": "--with-debugger was not selected; exact dense-complex native support is unavailable",
  "artifact_sha256": {}
}
```

An omitted profile has no module payload, protocol records or worker observation.
Legacy manifests have no value component, value protocol/hash/observation or
bundled `mathlab_value_bridge.oct`. Both reject that basename anywhere in the
bundle, including relocated or case-variant copies. Legacy three-adapter records
retain their historical scope; missing declarations never imply value support.

New `--with-debugger` builds require:

```json
{
  "status": "enabled_protocol_load_passed",
  "adapter_profile": {"schema_version": 1, "name": "four-adapter-value-v1"},
  "value_component": {
    "status": "required",
    "path": "Contents/Resources/native/mathlab_value_bridge.oct",
    "capability": "dense_complex_native_components.v1",
    "sha256": "<64 lowercase hexadecimal characters>"
  },
  "runtime_version": "<selected exact version>",
  "runtime_executable_sha256": "<64 lowercase hexadecimal characters>",
  "sdk": {"OCTAVE_VERSION": "<same exact version>"},
  "artifact_sha256": {
    "mathlab_debug_bridge.oct": "<sha256>",
    "mathlab_scope_bridge.oct": "<sha256>",
    "mathlab_input_bridge.oct": "<sha256>",
    "mathlab_value_bridge.oct": "<sha256>",
    "runtime-version.txt": "<sha256>"
  },
  "protocols": {
    "debug": {"protocol": "mathlab-debugger", "version": 4},
    "scope": {"protocol": "mathlab-scope", "version": 2},
    "input": {"protocol": "mathlab-input", "version": 1},
    "value": {
      "protocol": "mathlab-value", "version": 1,
      "dense_complex_components": true,
      "max_cells": 2000, "max_rank": 32,
      "max_replace_leaf_bytes": 134217728
    }
  },
  "worker_observation": {
    "method": "built-cli-doctor", "ready": true,
    "cli_executable_sha256": "<same as build.executable_sha256.mathlab>",
    "value_adapter": "<canonical absolute fresh-build module path>",
    "runtime": "<absolute observed runtime path>",
    "worker_capabilities": [
      "typed_nd_page.v1", "atomic_range_edit.v1",
      "dense_complex_native_components.v1"
    ],
    "module_sha256": "<same as value_component.sha256>",
    "runtime_executable_sha256": "<same as native runtime executable hash>"
  }
}
```

The example abbreviates SDK/compiler data and old protocol feature fields;
assembly retains their complete results. Value protocol fields/keys/types are
exact. Versions and limits must decode as JSON integers, not booleans or floats.
`adapter_profile` has exactly `schema_version` and `name`. `value_component` and
`worker_observation` have exactly their displayed keys. Worker capabilities are
unique strings containing the three displayed names; other negotiated names are
allowed. All four module hashes and the version-marker hash are mandatory and
must match final bundle bytes. The required profile permits exactly one value
module, at the declared path. `build.linked_libraries` must retain nonempty
inspection output for each of the four module basenames.

The live probe uses this invocation's freshly built CLI:

```text
mathlab --json --octave <selected-runtime> --directory <owned-empty-project> doctor
```

The CLI/session-owner API prerequisite is doctor JSON with `ready:true`, absolute
`runtime`, canonical absolute `value_adapter`, and `worker_capabilities` as an
array of actual negotiated capability strings from the current Engine worker.
The package and new native gate both use this field contract. A direct native
protocol probe, configured path or expected capability literal is insufficient.
Missing doctor fields fail the required profile until its owner integrates them.
No CLI/launcher/verify/builder or PM QA changes are included in this release patch.

The packager compares the observed canonical module/runtime paths to its fresh
inputs, hashes CLI/runtime/module before and after the probe, and binds the
receipt to those identities. Static validation checks the receipt's consistency
with the final CLI/module bytes and recorded runtime hash. It does not rerun
doctor, authenticate an unsigned manifest, require vanished temporary build
paths to exist after relocation, or qualify installed API behavior.

PM QA should scrub inherited adapter variables, select only declared bundled
components, compare the current installed canonical value path and external
runtime executable hash, and require actual observed worker capabilities. Doctor
is only a prerequisite: identified packaged checked read/edit/history acceptance
and actual desktop journeys remain separate owner work. The source native gate
has a separate new schema/suite identity; prior 27-contract reports are preserved.

The completed first slot ran package, source, public-tree and native-gate Python
tests serially, followed by `bash scripts/verify-value-provisioning-test.sh` and
shell syntax. These used owned fake executables without Cargo/native/Octave
workloads. A corrected repeat recorded the required minimum free-disk floor;
both raw batches are preserved in the receipt. The lease and all recorded child
handles are terminal. Further execution requires a new PM-coordinated slot;
PM-owned QA and session-owner runtime integration remain separate. No CI
dispatch or publication is authorized. These status updates do not change the
version-1 field/enum/API contract.
