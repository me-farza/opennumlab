# Development GUI validation with a private APFS cache seed

This is a source-only design and patch proposal, inspected on 2026-10-15. No cache clone, deletion, Cargo/native job or GUI launch was performed. Existing release assembly, app evidence, source archives and prepared private commits are unchanged by this proposal.

Implementation is on hold while the [hosted default macOS packaging proposal](HOSTED_MACOS_PACKAGING_PROPOSAL.md) is assessed. Neither proposal has started a packaging job.

A separately labeled development app is a sound **conditional experiment**, using an APFS dependency-cache seed and a fresh compilation of every workspace crate. Completion with the current free space is not established: the exact settings of the cached GUI family are unproven. The experiment must stop on incompatible cache reuse or its space limits. It must not become an implicit cold build.

## Measured space and prerequisites

The checkout and `/private/tmp` are on APFS device 16777234. A read-only inspection found 4,482 regular files and 752 directories in `target/debug`, with no symlinks or special files. These measurements are observations, not a reservation or proof that writers are idle.

| Item | Observed size |
| --- | ---: |
| Available Data-volume space | about 1.263 GiB |
| Existing debug cache | 1.425 GiB |
| Dependency artifacts identified from depfiles | about 954 MiB |
| Workspace artifacts, including historical/test variants | about 437 MiB |
| Build-script output directories | 39.3 MiB |
| Incremental cache | 0 |
| Sum of observed per-target workspace maxima across historical families | about 65.5 MiB |
| Frozen 45f selected source | 4.19 MiB |

The root `target/debug/mathlab-desktop` executable is absent. Historical hashed desktop images are at most about 17 MiB; they are not a current source-qualified GUI executable. The 65.5 MiB subtotal is a sizing proxy, not an established compatible artifact family. Cache records identify rustc 1.94.0 (`4a4ef493e`, LLVM 21.1.8, aarch64-apple-darwin); the installed SDK settings file identifies macOS SDK 26.5. Both must be checked against the actual future invocation. Empty rustflags in inspected fingerprints do not establish all effective profile/configuration values. Retained records contain both debuginfo 2 and separate debuginfo 0 runs; none binds a complete numeric profile to this GUI family. Do not set `CARGO_PROFILE_DEV_DEBUG=0` merely to reduce space.

Planning allowance for a matching warm dependency family:

| Added allocation | Allowance |
| --- | ---: |
| Clone metadata, frozen source, fingerprints and logs | 32–64 MiB |
| Replacement workspace outputs/helpers | 100 MiB |
| Codegen/link/native scratch | 350–500 MiB |
| Separate development app/source/native copy | 50–64 MiB |
| Total planning envelope | about 0.52–0.71 GiB |

This is an allocation estimate, not an observed upper bound. A first trial should cap net volume consumption at 640 MiB and maintain at least 512 MiB free, stopping the whole process group at either threshold. Its upper planning range could exceed that trial cap; an abort is an expected bounded outcome. Serial execution and checking before each phase reduce exposure, but monitoring cannot reserve filesystem space or prevent an individual large write. A normal cache copy already exceeds available space; a broad dependency rebuild also breaks the plan.

Before execution, root must reserve the heavy-job slot, establish a quiescent cache snapshot, approve a committed source pin containing the intended fixes and proposed builder, and either establish the matching GUI dependency settings or accept a fail-closed cache-miss trial. More free space and a new estimate are prerequisites for a cold or broadly invalidated build. A free Cargo lock at one instant is not ownership of the slot.

## Concrete patch shape

Add a separate `scripts/release-development-gui.py` entrypoint, its synthetic contract tests, and this guide. Keep `package-macos.sh`, the existing release builder/CLI and its default release acceptance unchanged. Do not add a generic prebuilt, skip-build, shared-target or dirty-build option.

Proposed interface (not implemented):

```text
release-development-gui.py
  --source-checkout CLEAN_REVIEWED_CHECKOUT
  --expected-commit FULL_COMMIT
  --tree-set VERIFIED_REPOSITORY_TREE_SET
  --seed-target EXISTING_TARGET
  --with-debugger
  --output NEW_DIRECTORY/OpenNumLab-Development-GUI.app
```

The checkout must be clean at the expected commit and match the verified tree set in bytes/modes. Record both the standalone commit and upstream source-selection pin; do not infer either from moving HEAD. The new entrypoint can reuse `release-package.py` as an imported library: call its existing `package` with an internally defined development builder and a destination **inside owned temporary staging**, never the user-visible destination. That retains its fixed-commit export, isolated Git identity, corresponding-source checks, notice generation, version agreement, executable copying, native probes and payload validation. Its test injection hook is not exposed as a new generic user option.

After that private assembly, convert its manifest before final publication. The final development app must contain `development-validation.json` with a distinct kind, and **no `release.json`**. Recompute the complete payload inventory/checksums and validate with a development-only validator before an atomic absent-only publication. This avoids a default installed-release harness accepting a development profile merely because its source is clean; the current harness requires `release.json` and remains unchanged. Preserve common byte/mode/source/native integrity checks. A transient manifest inside private staging is not a delivered result.

Use visible app/display text `OpenNumLab (Development GUI validation)`, a distinct development bundle identifier, and an explicitly separate writable test project/state directory. The new output must be absent and must never exchange with an existing app. The development manifest must identify `cargo_profile:dev`, `target_policy:private_apfs_clone`, `optimized_release:false`, `release_lane_eligible:false` and qualification `not_qualified`. GUI observation reports bind this manifest and final checksum-list hash. They cannot be attached as release-candidate acceptance.

## Clone and rebuild algorithm

1. Under root's exclusive reservation, hold the original Cargo target/build-directory lock only during the snapshot, while excluding non-Cargo writers. Do not hold that lock when starting Cargo on the private target. Inventory the selected cache and capture before/after identities under the reservation. A per-file clone walk is not an atomic directory snapshot.
2. Create an owned, new target on the same APFS volume outside checkout ancestry. Create directories separately and call `clonefile`/`fclonefileat` for each regular file, rejecting symlinks, nonregular entries, existing destinations and clone errors. Never fall back to normal copying or hardlinks. Verify bytes/modes and distinct source/destination inodes; retain the seed inventory digest. Release the original cache reservation after the verified snapshot. Subsequent writes are confined to the private target. Apple's `cp -c` permits full-copy fallback, and cloned files do not reserve later write space. [Apple clonefile](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2), [Apple cp](https://github.com/apple-oss-distributions/file_cmds/blob/main/cp/cp.1).
3. Invalidate all five workspace packages **inside the private clone only**, including prior hashed variants, final binaries, fingerprints, incremental state and build-script/helper outputs. A scoped `cargo clean -p` is one component; inspect afterward and reject any reusable workspace remnants. Never execute a copied final GUI/CLI binary. Fresh frozen source may have old archive timestamps, so Cargo freshness alone does not prove it compiled the intended bytes. [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html), [Cargo fingerprint model](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html).
4. Inspect cached third-party build-script output for original target/OUT_DIR paths. Keep valid relocated outputs only when their active references resolve privately; otherwise invalidate and regenerate their run outputs/fingerprints. Do not edit fingerprints to force freshness. Native link-search directives or an active console-helper path must not resolve back to the original target. Mere historical paths in debug symbols are a separate matter. The engine build script must regenerate/embed its console helper; the CLI build script must rerun its source/Git identity calculation. [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
5. Use the exact compatible installed toolchain, ordinary Cargo `dev` profile, host-target mode and feature graph. Set both target and build directories to the owned clone where supported; isolate Cargo home with registry/git caches only, excluding user/live ancestor config. Record all effective profile overrides, wrappers, rustflags, codegen/debug/incremental values, linker/SDK/deployment settings, dependency features and command argv. Do not change global dependency flags to save space. Disable incremental for newly rebuilt workspace units and serialize jobs. [Cargo cache layout](https://doc.rust-lang.org/cargo/reference/build-cache.html), [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).
6. A proposed transparent `RUSTC_WRAPPER` should allow only precisely recognized compiler version/target-info probes or Cargo-dispatched compilations of inventoried files under the five canonical frozen workspace roots, with private output paths. Reject a registry/git crate compile before executing rustc. `--emit=metadata` is a compilation, not a probe. Unknown invocation forms or cache misses abort; do not widen the allowance automatically. Use the general wrapper rather than a workspace wrapper, whose artifact namespace changes. This is a cache-miss tripwire, not a compiler sandbox: cached build scripts can invoke rustc/native compilers directly, so an independent space/time supervisor of all owned compiler sessions remains necessary. The existing packager creates nested sessions; killing only its outer process group is insufficient. Session registration/cleanup must be implemented and synthetically proven before any trial. [Cargo wrapper/environment contract](https://doc.rust-lang.org/cargo/reference/environment-variables.html).
7. Build GUI and CLI with the actual dev command, for example `cargo build --frozen --message-format=json-render-diagnostics -p mathlab-desktop -p mathlab-cli`. `--frozen` controls dependency/network behavior; source freezing is established separately. Require successful completion, wrapper allow records, `fresh:false` for requested workspace targets/build scripts, regenerated helper identity, and all frozen input hashes unchanged. Cargo-reported executables must be regular files beneath the private target, then be hashed and copied into private staging before any lease ends. Recheck copied hashes and inspect linked libraries for original-target dependencies.
8. Build all three native adapters afresh from the same frozen source with the selected matching Octave/mkoctfile/compiler and require debugger/scope/input protocol loads, or explicitly omit native capabilities. Never copy modules from the old app or source cache. Retain exact tool/SDK/runtime/module identities, logs, native status and scope limits. No full test/native-regression or release build is included in this small space plan.

Proposed helper boundaries are `clone_seed_exclusive`, `invalidate_private_workspace`, `build_development_gui`, `convert_development_manifest`, `validate_development_bundle` and `publish_new_development_bundle`. None should mutate the original cache, default release app or a retained artifact. On failure retain the bounded failure receipt; any future cleanup is limited to explicitly owned temporary paths.

## Validation before use

Synthetic tests should cover clone failure with no copy fallback, source-cache mutation detection, wrong device/profile/commit, symlink rejection, unremoved stale workspace output, dependency-compile rejection, helper provenance, old-target link leakage, space-floor/process-group termination, mismatched executable hashes, missing development label, accidental `release.json`, existing-output refusal and preservation of old apps/cache. Default release packaging tests must remain unchanged and pass. No such implementation tests were run for this design document.

Actual acceptance would be limited to named GUI journeys against the new development app, especially breakpoint/pause/evaluate/step/continue, clear breakpoints, edit/save, Run a new marker, and verify same-session state. Keep prior failure observations immutable. Optimized-release behavior, reproduction, performance, full installed/native/clean-machine/lifecycle acceptance, signing and MATLAB UI parity remain separate pending work.
