# Actual local candidate run, 2026-10-15

Frozen candidate `2ef9d7806bac939114580fb634f95aad5b33d3f8` completed the authorized serial sequence: source-native gate, private release/app assembly, then the reviewed installed CLI harness. All three exited zero. The exact detached checkout remained clean; its 155-file selected-source digest remains `44907d20a58f2359613bc1365bb885776561fa6064d9eb0933fde14e56fcd21b`. No new candidate revision was substituted.

The new unsigned app is `dist/candidate-2ef9d7806bac/NumJinn-alpha-2ef9d7806bac-native.app`. Internal records, logs and reports are beside it. Existing apps, source, working developer binaries and user projects were preserved; nothing was published. These results cover the declared local macOS arm64 source and CLI scope. Whole-release qualification remains pending.

| Stage | UTC interval | Seconds | Exec session / child PID |
| --- | --- | ---: | --- |
| Native gate | 16:53:44–16:56:01 | 136.235 | `33719` / `80429` |
| Private packaging | 16:56:26–16:58:36 | 130.371 | `62124` / `84427` |
| Installed CLI QA | 16:59:16–16:59:24 | 8.478 | `3285` / `85961` |

The exact argv, working directories, selected environment, launch/process-group IDs, timestamps, exit codes, free-space readings and log hashes are in `native-run.json`, `package-run.json` and `qa-run.json`; matching `*-session.json` records retain the execution session handles. All handles are terminal. `serial-slot-handback.json` records explicit handback after completion; a contemporaneous process check found no compiler processes. Later work requires its own slot/capacity check.

## Source-native gate

The commands followed [CANDIDATE_RUN.md](CANDIDATE_RUN.md), using explicit `/opt/homebrew/bin/octave-cli` and matching `mkoctfile`, a private `native-target`, two Cargo build jobs, no incremental compilation and zero development/test debug information. Runtime and SDK both reported 11.3.0. The three newly built adapters loaded with debugger/scope/input protocols 4/2/1 before Cargo compiled the test executables.

The gate passed 27 exact named contracts: 17 native debugger/scope/source-preparation/input/desktop-worker tests and 10 CLI conformance tests across nine suites. No tests failed, were ignored, skipped or timed out. The gate source inventory includes all 459 tracked inputs, separate from the 155-file selected distribution inventory; before/after source inventories matched. Independent read-only review rehashed all source inputs, 16 command logs, 11 Cargo-captured executable artifacts and three native modules. Eleven captured executables does not mean eleven executables were tested; the report identifies the actual suite binaries.

`native-report/report.json` SHA-256: `7b6f28852e33c677bfef3d0c75a0a36fd6d384d4868793f613bf7cad328aa48b`. Outer `native-gate.log` SHA-256: `e5574aec25589d2c780d023d76f4a75eb664099620fa1547edc745d25d904609`.

### Additional frozen language targets and formatting

After a separately authorized implementation handoff, release used the existing frozen checkout/private `native-target` for only `cargo test --locked -p mathlab-language --test preparation --test octave_local_functions --no-run --message-format=json`, then executed its two captured binaries directly and serially. All native adapter environment variables were unset; the selected Octave 11.3.0 executable retained its known hash. The 17 exact preparation and eight exact real-Octave language cases passed with zero failures, ignored cases or early-return skips. `cargo fmt --all -- --check` passed without source changes.

Evidence is in `language-source-qa/report.json`, SHA-256 `eb3203cf0120fb18b242b189b03059cdd717359feeddf1ea0750048cebcf42a9`; `session.json` records terminal execution session `56290`. The report retains both executable identities, exact named results, five bounded command/process-group records, ten stdout/stderr log hashes and unchanged 459-file source inventories. Independent read-only review rehashed the executables/logs/source and found no inconsistency. Actual runtime/compile/preparation/Octave/formatting durations were 0.551/0.902/0.383/4.300/0.332 seconds, within respective 30/180/60/180/120-second limits. The compile slot was explicitly handed back after completion. The accepted 27-test gate and PM's separate supplemental run were not repeated. This adds source-only language/formatting evidence; it does not mark A2 or full release accepted, close the separately recorded runner cleanup failure, or change the installed app/archive identity.

## Private release assembly

The frozen candidate's own `package-macos.sh --with-debugger` built CLI and desktop from a second exact source export in a private target outside checkout ancestry. Its isolated Cargo home reused registry/git caches without inheriting user Cargo configuration. Two build jobs and disabled incremental compilation were recorded. The private build/staging directories were cleaned by the packaging operation after assembly; the frozen source, native-gate target, app and reports remain retained.

Build identity: Rust/Cargo 1.94.0, macOS 26.6.1 arm64, Apple SDK 26.5, Apple clang 21.0.0 and Octave/SDK 11.3.0 (`aarch64-apple-darwin25.4.0`). Packaging rebuilt its own native modules and load-probed them; the preceding native regression suite is separate source evidence, not a claim that the installed adapter bytes ran that entire suite.

| Final identity | SHA-256 |
| --- | --- |
| `Contents/Resources/checksums.sha256` | `05357daa9af8b462a8a2f2bd1a9fc52e5a001dd96b46511f66fe8c8ecc344ba6` |
| `Contents/Resources/release.json` | `eb4caddcfd21c63157e236aaa4437fbef5bbf3bb742164f784aa4ae12cb5248c` |
| `Contents/MacOS/mathlab` | `4d292aa85367fc2901ae115e0917639b1fe85bd0d6901238d404789526c0a355` |
| `Contents/MacOS/mathlab-desktop` | `ca7aceaf52d9e7ff157d7d479d28383a83275295405c7ea7848ec953247c421d` |
| `package-build.log` | `a9d1d3cfd33be0d8a94a0d32b48f6c8b1ff51c6a74702e67943a6e3952b8e099` |

The immutable build manifest retains `qualification.status: not_qualified` and its build-time `not_run` fields. Subsequent acceptance lives in separate reports bound to these exact bytes; the app manifest was not rewritten after tests. The app is unsigned, unnotarized and requires external Octave, graphics/toolkit/fonts and optional scientific packages. Build success does not establish clean-machine installation, binary reproducibility or offline/dependency/license-source closure.

Independent read-only bundle review passed all 501 final files, 499 payload records, final checksums and 155 exact committed/frozen source records/modes. It verified 167 Cargo notice packages, 325 listed license texts and 56 retained upstream notice hashes with no missing text; the explicit `dispatch` 0.2.0 copyright gap remains recorded. The CLI embeds the frozen revision and its separately computed build-script source fingerprint `4570d284a16cebd8e3f1f9e1a246eca27d41b7c4c69d29597d253ce022571bb8` (a different inventory algorithm from the release export digest). Executable/native/runtime hashes, recorded linked dependencies and protocol identities match; signature inspection confirms unsigned status. The new destination reports no previous bundle, and the existing `MathLab.app` entries predate package launch. The audit launched no app or numerical runtime.

## Installed CLI subset

The reviewed candidate `scripts/qa-installed.py` hash, `0c1627309fb987a0c47e1244964517e21cffbe711f9a865a9a229bdb30deffcb`, matched the packaged corresponding-source record before launch. It ran with explicit external Octave from owned temporary projects outside the app/checkout. The retained report is `installed-qa/installed-cli-20261015T165916Z-f1af02506e/report.json`, SHA-256 `e3c99a13b787b37c05aaa8c83e7aa2739e7610b462d84b58ec0c5508fda831d2`. Outer `qa-installed.log` SHA-256 is `9bb0739769d689016b696e40ddd0a6eef0b6b9a2e3fbcb04e98a8b9bc15ec955`.

All 12 required checks passed: bundle identity, CLI version, doctor, explicit missing-runtime diagnostic, original numerical script, original-source error location, persistent REPL, graphics/PNG, bundled getting-started example, same-runtime MAT self-roundtrip, authentic native input and unchanged bundle. The report contains nine actual CLI commands and retained source/log/artifact evidence. Temporary execution files were removed. Runtime identity and final app source/executable/checksum identities match the packaging record.

Independent read-only QA review rehashed all nine commands' 18 stdout/stderr logs, eight retained artifacts, all 501 final bundle files, the 155 corresponding-source records and packaged/runtime identities. Report and copied pre-QA manifest/checksum identities match the package record; the actual numerical, source-line, persistent-state and Unicode/empty/caller-scope input logs support their declared outcomes. Two nonzero command exits are the expected missing-runtime and original-source-error negative cases. No audit inconsistency was found; the audit itself executed no tests or application.

This harness's MAT coverage is an Octave self-roundtrip; native input coverage is not a debugger or desktop journey. The six required pending journeys remain desktop UI, Stop/recovery, debugger, read-only Resources/relocation, clean-machine installation and lifecycle. Complete installed conformance acceptance, Linux hosted CI, full native regression against packaged module bytes, signing and public delivery are also not established. `release_qualified` remains false.

### Installed saved-reference replay

After build-slot handback, an explicitly authorized headless replay used the unchanged packaged CLI from an owned external project. It read the packaged `common.json` and `known-gaps.json` separately, with explicit Octave 11.3.0 and the packaged scope/input adapters. The original independent CP01 reference packet was copied without flattening, verified against its original index/file hashes, and retained alongside the new reports. No MATLAB executable or GUI was launched. Both commands used `--strict`, bounded process groups, retained stdout/stderr and complete JSON reports, and correctly exited one.

| Retained replay evidence | SHA-256 |
| --- | --- |
| `installed-reference-replay/replay-run.json` | `41c2fbcafff1cb041beb0b551ca5d17c9d75cae72f4fc80f154607dce14d6974` |
| `installed-reference-replay/common.json` | `a7558b9ed0ea0438babf02018ba2dde79e56c00e29dbb859c1b16e18b156399a` |
| `installed-reference-replay/known-gap.json` | `32595c06140170aff8525644d1baf1a57a11ca3cf4fe228d89ef66e39db94c5d` |

The five-case common report is **incomplete**: 13 pairwise matches, zero pairwise/oracle mismatches and two unsupported comparisons caused by raw Octave's intended forward-local `make_offsetter` call. All five prepared application results match the saved MATLAB records and independent oracles, including the expected-error partial state. The raw route's limitation remains unsupported; it is not counted as a pass. The MAT-v7 case supplies the identified 876-byte MATLAB-written file, SHA-256 `52098ccf210ebd13a4af3c75d547964f736f7cfc6c83ca6d228a35b7754afdff`, to both actual routes; their declared typed field/oracle matches support that bounded file-loading result.

The separate sparse-logical case is a **mismatch**: saved MATLAB reference matches its oracle, while raw Octave and the application each produce an error and oracle mismatch after their own MAT roundtrip loses sparse storage. Its pairwise comparisons remain `not_run`; agreeing route failures are not matches. `mat_file_policy` is `not_applicable` for this case, so retaining the reference's 206-byte sparse-logical MAT file does not establish that the application loaded those reference bytes.

The run record binds the exact CLI/build revision, all packaged fixture/oracle/support bytes, packet identities, selected adapter/runtime hashes, actual commands/PIDs/bounds and separate report/log hashes. `session.json` retains terminal session `64817`. All 501 app files and the original/external reference copies remained unchanged. The external project is retained for review; raw replay/reference data stays in ignored output space. This execution leaves complete conformance and whole-release acceptance open. GUI/Stop/debugger/state journeys remain user-deferred.

## PM desktop handoff recipe

PM owns the next interactive desktop, Stop/debugger/state and relocation journeys. Release has not launched a GUI. The final manifest and checksum paths are the app's `Contents/Resources/release.json` and `Contents/Resources/checksums.sha256`, with hashes listed above. Use a fresh writable project outside the app/checkout and a separate recovery file. Run this prepared recipe from the original developer checkout root; it was not executed as part of this CLI run:

```sh
numjinn_bundle="$PWD/dist/candidate-2ef9d7806bac/NumJinn-alpha-2ef9d7806bac-native.app"
numjinn_project="$(mktemp -d /private/tmp/numjinn-installed-desktop.XXXXXX)"
mkdir "$numjinn_project/.mathlab"
cp "$numjinn_bundle"/Contents/Resources/examples/*.m "$numjinn_project/"
env -u MATHLAB_DEBUG_BRIDGE -u MATHLAB_SCOPE_BRIDGE -u MATHLAB_INPUT_BRIDGE \
  "$numjinn_bundle/Contents/MacOS/mathlab-launcher" \
  --cwd "$numjinn_project" --octave /opt/homebrew/bin/octave-cli \
  --state "$numjinn_project/.mathlab/session.json" \
  --open "$numjinn_project/getting_started.m"
```

The launcher selects the three bundled adapters after inherited bridge overrides are removed. Retain the owned project path, actual launch log/process handle, recovery artifacts and screenshots with the resulting journey evidence. Runtime remains external Octave 11.3.0. Keep those GUI results separate from the completed CLI subset and bind them to the same final checksums.

## Separate corresponding-source archive

The source-only set at [checkpoint e1b0b2f](SOURCE_E1B0B2F.md) uses a newer tooling commit and must retain that identity. PM independently compared its 159-file inventory against the candidate freeze: all 155 selected candidate inputs match by bytes and executable modes; its four additions are two release documents and the exporter/tests. Internal evidence is `artifacts/candidate-2ef9d78-source-correspondence.json`. That correspondence does not change either commit, qualify archive-only app packaging or establish full external dependency closure. No Git history or raw research packet is part of the allowlisted archive.
