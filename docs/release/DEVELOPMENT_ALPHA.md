# OpenNumLab Development alpha packaging

This lane hardens the currently implemented application. It does not qualify Core Desktop, Full, exact MATLAB compatibility, a self-contained runtime distribution, or trusted public installation. Internal executable, crate, protocol and environment names remain `mathlab` in this brand iteration.

The first public preview is being prepared as **v0.1.0-alpha**. [Release notes and known gaps](V0.1.0_ALPHA.md) describe its advertised subset. Source regression results, package assembly, installed checks and GUI journeys remain separate evidence. The current candidate source checkpoint includes the Rust arithmetic/session libraries, selected operator regressions and the bounded discrete-accumulator library; the proposed persistent Rust CLI route is outside that checkpoint.

The [versioned native package contract](VALUE_BRIDGE_RELEASE_CONTRACT.md) defines explicit adapter omission and required native support. Local package and installed-CLI results exist for an earlier candidate; a freshly rebuilt candidate still needs its own installed, relocation and final payload review. These notes do not turn those earlier results into qualification of a new app.

From a clean committed checkout on macOS, in a prepared Rust/Apple SDK environment:

```sh
./scripts/package-macos.sh
./scripts/package-macos.sh --with-debugger --output dist/OpenNumLab-native.app
open dist/OpenNumLab.app
```

Run only one real release build at a time. The source export and Cargo target are private temporary directories outside the checkout. Cargo reuses only registry/git caches through an isolated Cargo home, excluding live checkout and user Cargo configuration. An inherited `CARGO_TARGET_DIR` does not select the release target: private outputs prevent another build replacing a binary before it is hashed/copied. Ensure sufficient disk for a complete release target before starting. No skip-build/prebuilt packaging option is exposed. `--allow-dirty` is solely an unqualified local-development escape hatch; it sets `clean_candidate:false` and cannot satisfy the accepted clean-source gate.

The prospective default output is `dist/OpenNumLab.app`; existing MathLab and NumJinn apps remain preserved. The product display name is OpenNumLab and the byte-preserved original `assets/opennumlab-logo.png` is included when present; `.icns` generation and internal identifier migration remain separate. The stable bundle identifier remains `org.openmathlab.desktop` for this iteration. App `release.json` retains schema version 1; source/public-tree schema version 2 is a separate format change.

## Retained installed GUI limitation

The immutable historical NumJinn candidate `2ef9d7806bac939114580fb634f95aad5b33d3f8` is **not qualified**. A later saved-source repair passed local source/native regressions, but the edited Save→Run journey still needs verification in the release app. PM's retained actual desktop observations report a stale-source failure: after breakpoint/pause/evaluation/step/continue, removing the breakpoint, editing and saving the original script writes the new file, but Run executes the previous four-line source. The appended marker remains absent in an independent console query. Those observations describe the old binary and are preserved as a failed installed result.

The same observations retain bounded passes for desktop numerics and a visible plot, real grid inspection/scalar editing, Unicode input, Stop preserving the engine/workspace, basic debugger breakpoint/evaluation/step/continue, and editor Save itself. Those passes do not erase the failed Save→Run freshness journey or qualify full source-snapshot/debugger variants. Desktop state recovery, desktop relocation/read-only Resources, clean-machine installation and installer lifecycle remain pending. All 501 installed bundle files were rechecked unchanged after the GUI pass.

Internal evidence: `dist/candidate-2ef9d7806bac/desktop-qa/observations-20261015.json`, SHA-256 `6869b628593cfeeb6b390311b701009242f86ccff5ed3c91a872f157eb6c34be`. The report identifies the original candidate/checksums and root-owned CUA observations; raw screenshots and interaction outputs remain with PM. OpenNumLab branding and newer source regressions do not relabel this historical result.

## Assembly and provenance

Assembly uses an empty private staging directory. A deliberate tracked-source allowlist includes application/build source, original fixtures and conformance source, selected product docs, reviewed Cargo notices and the one original logo. It excludes all research, evidence, generated artifacts, vendor directories, local/private directories, caches, native binaries and MATLAB-generated `.mat`/`.slx` payloads, even if tracked. Untracked intended source must be committed first. Symlinks and unsafe filenames are rejected. Clean candidates export the fixed Git commit and verify it against the captured SHA-256 inventory; they never compile from the moving checkout. An isolated detached Git identity ensures build scripts observe that fixed commit.

`Contents/Resources/release.json` records the product/profile/version, source commit and dirty status, frozen source inventory/digest, actual Cargo command/toolchain/SDK/platform configuration, built executable hashes and linked-library inspection. The new required native profile records selected Octave/SDK/compiler configuration, runtime-executable and four module hashes, actual protocol load results (debugger 4, scope 2, input 1, value 1), and an identified worker capability observation. Its value probe uses `mathlab_value_bridge('protocol')`; the older adapters use two arguments. Protocol/doctor success is narrower than checked read/edit/history or installed acceptance. Without adapters, their omission is explicit; legacy manifests retain their original three-adapter scope and cannot carry a value module. `checksums.sha256` covers every final file except itself; packaging prints its SHA-256 as the digest of that complete checksum list. Executable permission flags are validated separately. No archive, signing or notarization processing is claimed by these hashes.

Assembly leaves `qualification.status:not_qualified` and installed/native-regression/public-review evidence at `not_run`. `clean_candidate:true` identifies clean build input only. Installed acceptance must examine its actual reports and conformance summary/per-route/oracle statuses; conformance can intentionally exit zero with an `incomplete` summary. Process exit zero and source CI entrypoint counts cannot qualify a final candidate.

Before publication, the candidate is checked against source, payload, version, permissions and final checksums. Existing apps are exchanged atomically using macOS `renamex_np(RENAME_SWAP)` and retained beside the destination under a unique `.previous-*` suffix. If the filesystem does not support exchange, replacement fails with the old app intact. An interruption leaves the installed destination usable and the other bundle outside temporary staging; interrupted staging may require manual housekeeping. This establishes local candidate preservation, not a qualified updater/uninstaller or power-loss durability claim. Never delete retained bundles without inspecting them.

The launcher copies original `.m` examples once into `~/Library/Application Support/OpenNumLab/Projects/Getting Started`. It preserves edited copies and opens the starter only when recovery state is absent. Process cwd, Current Folder, default `.mathlab/session.json` recovery and generated figures live in the project, outside app Resources. `MATHLAB_PROJECT_DIR` selects another default project; `--cwd` selects an existing project without copying examples. Relative directories are normalized once; `--open`, positional source and `--state` remain available. Existing MathLab/NumJinn project data is preserved at its old location; no automatic migration is performed or qualified.

External runtime prerequisites remain explicit:

- GNU Octave, located through `MATHLAB_OCTAVE`, `PATH` or common Homebrew locations. Non-native builds do not pin a runtime and require per-runtime validation.
- A working Octave graphics toolkit, fonts and normally gnuplot for figure export. Runtime discovery alone is not graphics qualification.
- `control` for `control_response.m`; scientific package profiles remain separately installed, optional and individually qualified.
- For `--with-debugger`, matching Octave headers/compiler and the selected runtime must build and load all four adapters on the same OS, architecture and ABI. A different external runtime needs a new adapter build. This lane does not provide portable Octave modules.

Cargo lockfile and license texts are included; Cargo dependency source archives, Apple SDK, compiler, Octave/toolkit and scientific package sources remain external build prerequisites. Full offline source/dependency closure and license-distribution acceptance are later release gates.

Repository and source-tree preparation are described in [PUBLIC_TREE.md](PUBLIC_TREE.md) and [SOURCE_ARCHIVE.md](SOURCE_ARCHIVE.md). Export tools select committed allowlisted files without copying internal history or raw research evidence. Preparing a source tree does not publish it, demonstrate hosted CI success, or qualify its corresponding app. Retained historical source receipts describe their own revisions.

Selected original conformance fixtures, mathematical oracle JSON and `compatibility/conformance/build_oracles.py` are included for reproduction. Some source READMEs retain links to developer-only `artifacts/` evidence and broader roadmap/research documents. Those references are unavailable in this deliberately bounded source export; they neither require nor authorize distributing raw evidence. Runtime/build source and selected native/interface docs remain included. PM/implementation should mark those historical evidence links as checkout-only when reconciling product docs.

The three original root product plans linked by README are optional exact source inclusions after PM review: `DEVELOPMENT_PLAN.md`, `FUNCTIONALITY_CATALOG.md` and `INTEGRATION_STRATEGY.md`. They are not required inputs for minimal packaging fixtures. Historical artifact links remain developer-local; no raw log/evidence directory is admitted to make those links resolve.

The exact reviewed installed acceptance harness paths `scripts/qa-installed.py`, `scripts/qa-installed-tests.py` and `docs/qa/INSTALLED_ACCEPTANCE.md` are included. CI exercises its synthetic contracts on Ubuntu and macOS; that synthetic job does not run a packaged application or qualify the installed workflows.

The README uses the OpenNumLab display name, logo, app and writable-project defaults. Historical NumJinn candidate/source/archive reports retain their original names, revisions and hashes. New OpenNumLab package assembly has separate evidence; the first public candidate still requires its final advertised-scope and installed acceptance.
