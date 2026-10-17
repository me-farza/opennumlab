# Next candidate source readiness at 5c03e44

Static review is pinned after citation integration to
`5c03e442c21f59535d3e1c5ed0133db1008221fb`, tree
`9b8c2198f209cbfcf3d318b94f31ac76f86c48f5`. No missing selected production
dependency, product-document target or packaging field was identified. No
source-selector or production-code change is proposed by this review.

The unchanged policy selects 225 source files and 227 public-tree files at this
pin. Compared with the immutable `5fa9b5c` public set: four additions, 14 modified
files, 209 identical files, zero removals and zero mode changes. The additions
are the probe module, its integration tests and two release handoff documents.
These are static selection counts; no new archive, tree or package was produced.
This later handoff is outside the reviewed pin.

The package/source/public-tree producers, launcher, CI workflow, Cargo manifests
and lockfile are byte-identical to `5fa9b5c`. The new CLI probe and installed QA
consumer change the execution required of a future candidate; they do not change
the existing four-adapter manifest contract.

## Production and standalone scope

The [CLI probe](../../crates/mathlab-cli/src/value_probe.rs) is compiled into
`mathlab` through its normal module declaration. Its fixtures are inline Rust
strings and it owns a temporary workspace. The existing selector includes the
module, its [integration tests](../../crates/mathlab-cli/tests/value_api_probe.rs),
engine sources, embedded workers and helper inputs. Its dependencies already
exist in the unchanged Cargo manifests/lockfile. The CLI build script recursively
fingerprints the new module; its source identity must be refreshed when built.
The 36-input fingerprint is
`8b52ce6f8ec08c7ffcbc2bc620d138772537d10593e46ad15e2e5225b3f44593`,
using Rust path-component ordering. It identifies that CLI/worker source subset,
not the whole project, desktop, external native binaries or standalone prototype.

The [installed QA consumer](../../scripts/qa-installed.py) and its synthetic
fixture source are selected explicitly. For both declared profiles it requires
the packaged `probe-value-api` command, using `--expect-unavailable` for the
omitted profile. It binds CLI/runtime bytes and all four installed modules for
the required profile, and clears inherited adapter selections for omission.
Existing profile declarations, value-component metadata, protocols, module and
runtime hashes, and worker observation provide the needed packaging fields.

The `5fa9b5c` CLI source predates that command. A fresh post-probe build is needed
for the new QA gate; old app/source evidence cannot supply it. Legacy manifests
without an adapter-profile declaration retain their legacy scope. Packaging
continues to emit `not_qualified` and `installed_tests:not_run`; a later successful
API probe proves only its named CLI subset. Desktop, lifecycle and whole-release
qualification remain separate.

`prototypes/rn-prep-01/` and the RN research guide/roadmap JSON are standalone
development inputs, excluded from the sanitized selection and absent from Cargo
workspace membership. No selected production source or product-document link
requires them. Their local Rust 92-test result supplies no installed backend or
package qualification. The prototype README's contract hash is
`3086b3f06e6dfeb7fc3733dd497a99f3a73ce76b00f95e37c357dd9c7b936b59`.
There is no production reason to widen selection for these files. A future
separate prototype distribution would need its own reviewed source/contract set.

Citation integration replaces all 50 excluded evidence hyperlinks with explicit
local-evidence path citations. It preserves the historical paths/results and
does not distribute the raw records. Existing plain local path citations remain
outside that formatting change. All 199 active relative hyperlinks now resolve;
zero artifact hyperlinks remain in the selected Markdown. The 50 citations keep
the exact 32 original targets. All 12 literal Rust includes, two local quoted
native includes, 11 CI script references and 56 notice-file hash records close
within the selected set.

## Local fresh-candidate prerequisites

Use a clean checkout at an agreed full commit and a new, separately named,
absent `.app` destination. The [packager](../../scripts/release-package.py)
rejects dirty candidates by default; `--allow-dirty` produces an unqualified
local build. It freezes selected committed source and builds both CLI and
desktop with `cargo build --release --locked` in a new private Cargo target.
It does not reuse the shared `target/debug` or a supplied `CARGO_TARGET_DIR`.
Only Cargo registry/git cache directories are reused through symlinks.

For the current native candidate, select `--with-debugger` and identify executable
Octave and matching `mkoctfile` through `MATHLAB_OCTAVE` and
`MATHLAB_MKOCTFILE`. Runtime/SDK versions must agree, including any declared
expected version. The invocation needs Rust/Cargo, macOS compiler/SDK tools,
`otool`, Python/Git, complete cached Cargo/license inputs or separately authorized
dependency access, and writable owned temporary/output directories. The normal
Cargo build uses `--locked`, which does not itself prohibit network access.

The frozen native directory must initially contain no prebuilt modules/version
marker. The existing builder compiles all four adapters, requires debugger 4,
scope 2, input 1 and exact value 1 protocol loads, hashes the modules/runtime,
and observes capabilities through that invocation's CLI doctor. Do not reuse
old `.oct` files or infer installed API execution from these protocol checks.

Disk must accommodate the complete private optimized Cargo target, frozen source,
native/link scratch, staged app including a second source copy and notices,
and retained evidence. The staged app becomes the final app by rename; those
are one new-app allocation. Build temporary files use Python's temporary
directory; app staging uses the destination parent. If replacing an output, the
previous app is retained as well. A separately retained source-native-gate target
would add to the packager's private target. The scripts contain no disk-floor
guard, reservation or measured cold-build peak.

A read-only observation found 2,231,431,168 available bytes on the shared APFS
device for the checkout and `/private/tmp`. Tool-path presence was recorded
without executing version/runtime probes. These observations establish neither
sufficient build capacity nor tool/SDK/ABI compatibility. Maintain the existing
operator floor of 629,145,600 bytes (600 MiB); it is a stop boundary, not a build
budget. No local release-build capacity claim follows from this review.

Before execution, reserve the serial heavy-job slot, establish a fresh capacity
budget, and arrange a bounded space/time supervisor that tracks and drains all
owned nested sessions. The packager starts child commands in separate sessions,
so stopping only its outer process group is insufficient. Actual installed QA
then needs its own approved execution against the final candidate bytes.

The held [APFS development proposal](DEVELOPMENT_GUI_CACHE_PROPOSAL.md) still
describes three adapters and a 512 MiB floor. Before reviving it, the minimal
documentation refresh is four adapters/exact value protocol plus at least the
current 600 MiB operator floor, followed by a new budget and reviewed supervisor.
Its development-cache estimate is not a cold optimized-build estimate. The
[hosted proposal](HOSTED_MACOS_PACKAGING_PROPOSAL.md) has separate proposed
8 GiB starting/1 GiB stopping gates; neither proposal is an active local builder.

## Static receipts

Retained under `dist/opennumlab-next-candidate-readiness-20261015/`:

- `source-readiness-5c03e442.json` records raw committed selection, changes from
  `5fa9b5c`, include/CI/notice/link closure, exact citation preservation and the
  RN boundary. SHA-256:
  `80691f00188eb339e7fdeff265f7f323b2548592cfbb29d429fbe066e3241c4e`.
- `qa-packaging-contract-compatibility-5c03e442.json` binds 25 committed inputs
  and compares shared profile/protocol/component/capability contracts. SHA-256:
  `927754587890ef2ea053fc8beea4cf77c734a4bc70b055a4509439b96d5deab9`.
- `local-environment-observation.json` records the dated disk/tool-path
  observations and their limits. SHA-256:
  `1d45ed941d155bd5e1f190056220df7d388bb90a0dd05e59591e9740c352ffa6`.

The CLI fingerprint was independently recomputed from raw pinned blobs using
path-component ordering. The packaging/QA conclusions and build-prerequisite
wording received independent static review.

No build, test suite, runtime, package/export generation, network/CI/push or
cleanup was performed. The immutable `5fa9b5c` artifacts and prepared `ccb4a01`
private update remain outside this review. PRIVATE remote writes remain held
pending direct human authorization.

The optional controller prototype at `6d55e94` was withdrawn from distributed
source because host ownership support remained unresolved. Its exact source
remains in Git history; the source and manifest remain in the local
`dist/opennumlab-resource-supervisor-preparation-20261015/` bundle. Production
packaging, workload controls and existing bounded-test eligibility are unchanged.
