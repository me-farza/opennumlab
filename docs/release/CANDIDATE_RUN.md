# Detached local candidate run

The assembly and native-gate source milestones are `ccb1f89` and `19ad89e`; the CP01 runner/strict acceptance implementation is committed as `28eb1d7`, the report-directory fix as `3289222`, the reviewed installed QA harness as `2893be6`, and its invocation/tool-identity follow-up as `c0c96bc`. After this source-allowlist/CI milestone and the pending authentic-reference importer checkpoint, select one exact combined full commit. The shared developer checkout can continue CP02 without changing candidate inputs.

Use one exact full commit ID, not a moving branch name. Check that it contains the intended runner/build identity, conformance fixtures, release scripts and reviewed QA files. Create a new detached local clone under `/private/tmp`; preserve both the existing developer branch and existing bundles. A local shared-object clone is adequate for this short-lived internal candidate and copies no Cargo target. Its Git history is internal working material, not a public source distribution.

The following is an operator recipe, **not execution evidence**. Set `numjinn_revision` to the agreed full commit before running it:

```sh
numjinn_repo="$PWD"
numjinn_revision="FULL_COMMIT_ID"
git cat-file -e "$numjinn_revision:crates/mathlab-cli/build.rs"
git cat-file -e "$numjinn_revision:scripts/qa-installed.py"
git cat-file -e "$numjinn_revision:scripts/qa-installed-tests.py"
numjinn_candidate="$(mktemp -d /private/tmp/numjinn-candidate.XXXXXX)"
git clone --shared --no-checkout "$numjinn_repo" "$numjinn_candidate/source"
git -C "$numjinn_candidate/source" checkout --detach "$numjinn_revision"
git -C "$numjinn_candidate/source" rev-parse HEAD
git -C "$numjinn_candidate/source" status --porcelain
```

The detached clone must report the exact selected commit and empty status. Do not copy the live checkout's modified/untracked source into it. Do not reset or switch the shared developer branch to obtain a clean build.

Before compilation, coordinate the heavy-job slot with implementation, recheck free disk and ensure no native/debug build is running. A momentarily free Cargo lock is only an observation, not ownership of that shared slot. Packaging itself builds a second fixed commit export into a private target outside checkout ancestry; plan disk for a complete target, source export, candidate, and retained predecessor. The private target is removed afterward. Earlier free-space readings fluctuated enough to cause `ENOSPC` during tiny checks, so a prior reading alone does not establish capacity.

Run the candidate's own packaging script with the agreed external Octave 11.3.0 and matching `mkoctfile`, using a separately named destination such as `dist/NumJinn-alpha-<commit>-native.app`. Existing `MathLab.app`, `NumJinn.app` and user projects are preserved. Keep the build log, manifest, final checksum-list digest, runtime/SDK identity and actual command with the candidate record. A successful build and protocol probe still leave `qualification.status:not_qualified`.

After the exact revision and serial heavy-job slot are agreed, prepare these commands in the detached checkout. The native gate and packaging build run sequentially. Record the real source-native gate result before assigning it any passing status; debug information is bounded to conserve disk, and that configuration belongs in its evidence.

```sh
numjinn_short="$(git -C "$numjinn_candidate/source" rev-parse --short=12 HEAD)"
numjinn_output="$numjinn_repo/dist/candidate-$numjinn_short"
mkdir -p "$numjinn_output"
cd "$numjinn_candidate/source"
MATHLAB_OCTAVE=/opt/homebrew/bin/octave-cli \
MATHLAB_MKOCTFILE=/opt/homebrew/bin/mkoctfile \
CARGO_TARGET_DIR="$numjinn_candidate/native-target" \
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
bash scripts/release-native-gate.sh \
  --report-dir "$numjinn_output/native-report" \
  --build-timeout-seconds 900 --test-timeout-seconds 180

MATHLAB_OCTAVE=/opt/homebrew/bin/octave-cli \
MATHLAB_MKOCTFILE=/opt/homebrew/bin/mkoctfile \
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
bash scripts/package-macos.sh --with-debugger \
  --output "$numjinn_output/NumJinn-alpha-$numjinn_short-native.app"
```

These commands declare this host's Homebrew runtime locations. Other hosts must select and identify their actual matching runtime/SDK rather than copying those paths blindly. If either build fails, retain its failure record, stop the serial sequence and resolve the specific failure before continuing.

Run the candidate's reviewed `qa-installed.py --bundle ... --report-dir ... --octave ...` against the newly packaged CLI, from its owned temporary external projects. Retain its complete report and command/log hashes. The harness identifies a CLI subset and leaves desktop editing/figures, Stop/recovery, debugger, read-only Resources/relocation, clean-machine installation and lifecycle explicitly pending. Run those selected journeys separately against the same final bytes. Native source regression evidence is also separate from installed protocol loads.

QA integration review found compatible manifest/hash/native states and CLI schemas; PM reviewed and froze the original suite at `2893be6`, then accepted the follow-up `c0c96bc`. Its 14 synthetic checks passed in release's local rerun, including supplied-runtime alias handling. Reports now identify the executed script and Python runtime. Corresponding source includes only `scripts/qa-installed.py`, `scripts/qa-installed-tests.py` and `docs/qa/INSTALLED_ACCEPTANCE.md`; no general `qa-*` wildcard is allowed. Release has not modified those files. Check the executed harness SHA-256 against its packaged source record when retaining actual candidate reports.

For installed CP01 evidence, use the packaged executable and packaged original conformance manifest with explicit reference provenance. Parse every required route, oracle comparison and summary. Default `incomplete` may exit zero; `--strict` makes incomplete acceptance fail after writing its report. Do not turn absent licensed-reference evidence into a match or launch MATLAB implicitly. A known, retained compatibility mismatch remains a release limitation, not a reason to alter the oracle.

Summarize actual results in `docs/release/` with the candidate commit, final file identities, external prerequisites and pending gates. Generated logs/reports belong in a new named candidate output directory and are not automatically admitted to the public tree or source bundle. Keep the detached clone until rebuild/installed debugging is finished; any later cleanup must target that owned clone and generated target only, preserving projects, evidence and working bundles.
