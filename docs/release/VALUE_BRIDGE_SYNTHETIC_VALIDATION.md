# Value adapter release source validation

The release-owned source draft passed local synthetic validation on 2026-10-15
under installed `/usr/bin/python3` 3.9.6. No real Cargo, native compiler, Octave,
GUI, hosted CI, package build or source export ran. No installed qualification,
complete CP03, optimized build or new native ABI qualification is claimed.

The [version-1 contract](VALUE_BRIDGE_RELEASE_CONTRACT.md) is unchanged in its
fields/enums/API. The implementation adds the three exact source inclusions,
four-adapter package/protocol/hash/link inventories, explicit native omission,
strict manifest validation and a newly named source native gate. It does not
edit launcher, builders, verify scripts, CLI or PM-owned QA. Those owner surfaces
and identified packaged checked read/edit/history evidence remain integration
prerequisites.

## Actual local results

All five synthetic commands ran serially; no unexpected test failure occurred:

| Command | Actual result |
| --- | --- |
| `python3 -B scripts/release-package-tests.py` | 31 passed |
| `python3 -B scripts/release-source-tests.py` | 26 passed |
| `python3 -B scripts/release-public-tree-tests.py` | 18 passed |
| `python3 -B scripts/release-native-gate-test.py` | 20 passed |
| `bash scripts/verify-value-provisioning-test.sh` | Synthetic provisioning contracts passed |

The 95 Python tests use temporary fixture repositories and fake executables.
The native-gate fixture simulates its new `native-four-adapter-value-v1`
schema-3 contract of 105 named proofs across 21 suites. **Those 105 real native
proofs were not executed here.** Historical 27-contract native evidence retains
its old identity; neither it nor these synthetic results qualify the new gate.

Final read-only syntax checks ran separately for each of
`scripts/package-macos.sh`, `scripts/release-launcher.sh` and
`scripts/release-native-gate.sh`, with one `bash -n` invocation per file; all
exited 0. The earlier batch's multi-file `bash -n` command checked only its
first file because later filenames are shell positional arguments. Its raw
result is preserved and is not attributed to three checks. Final static records
are in `static-checks-final.json`; their four handles (`95208`–`95211`, including
the workflow YAML parser) are terminal. Workflow YAML syntax, Python and
embedded-Python syntax compilation without execution, and scoped whitespace
checks also passed. No cold dependency installation or additional toolchain was
used.

## Preserved resource and identity evidence

Initial raw batch: `dist/opennumlab-synthetic-value/20261015-lease-01/`.
It passed the same commands and recorded RSS and terminal handles, but the
600 MiB guard was initially interpreted as RAM. It did not record the requested
minimum free disk. Its report/logs/input snapshots are unchanged; a separate
`disk-guard-correction.json` explicitly records this evidence gap.

The corrected repeat is retained at
`dist/opennumlab-synthetic-value/20261015-lease-02-disk/`:

- Required free-disk floor: **629,145,600 bytes** (600 MiB).
- Actual minimum sampled available bytes across checkout and `/private/tmp`:
  **2,050,363,392 bytes**. The monitor sampled during every suite and would stop
  owned processes if the floor crossed. This is a sampled minimum, not a claim
  of continuous filesystem telemetry.
- Peak measured aggregate owned-process RSS: **45,344 KiB**, recorded separately
  from the disk policy.
- Before/after source input records matched exactly. Each batch retains exact
  input bytes/modes/hashes, command logs, guard source and process identities.
  Post-validation documentation changes and final source hashes are separately
  retained in `final-source.json`; they do not change tested executable logic.
- Both controlling execution sessions (`59693`, `19916`) exited 0. Corrected
  workload roots were `88743`, `89514`, `90247`, `91007`, `94154`, `94743`; every
  recorded workload handle is terminal, with empty live-owned-handle lists.
  The synthetic lease has been returned; no further workload is active here.

Corrected `report.json` SHA-256:
`b2a77d3e1162d8b935813c8d90f89e1a9a4182aca95c70b52ee1987cd0cc46fa`.
Retained guard SHA-256:
`8e46707ce446c99b2507473fd074107e760d05e4fb0ed6da87fb8d840b0d5ef9`.
Disk samples SHA-256:
`85fb83428f61ca5eff0762e29bc2be99689c4ad3bcd17190a1f2242d0663cc32`.

The report binds every log hash and the input snapshots enumerate every tested
release/helper input. These private logs are intentionally excluded from source
exports. Existing apps, archives, private-upload history and hosted evidence were
preserved. Further runtime/build/installed work requires its own coordinated
lease and actual artifact identities.

Release-owned implementation checkpoint:
`55b2e0ad98b7ab563cc3ef747dfebe8fa3ba0f9a` (11 exact owned paths; 219 selected
raw committed source files under its policy, without an export). The saved test
inputs also include the session owner's working launcher draft, SHA-256
`48a417b9b2a5fed5562f37b1b76411b2d200e3086bb242ce4709f89c352e883f`;
that uncommitted owner change was deliberately excluded from this release
commit. Its committed launcher remains SHA-256
`7aa393702084e43a8b155fcd1409debd62460e4422bfd1ce264d87fcca8739ba`.
`final-source.json` records this dependency difference explicitly. The passing
results identify the saved working-source inputs and owned functional bytes;
they do not establish a clean full-checkpoint application build. Owner
integration must be committed and re-pinned before packaging qualification.

## Fingerprint ordering correction

PM found that the audit helper had sorted complete POSIX strings while CLI
`build.rs` uses Rust `PathBuf::sort`, which compares components. A raw-blob
recomputation confirms the same input bytes produce these distinct digests:

| Pinned source | Inputs | Correct component order | Earlier incorrect string order |
| --- | ---: | --- | --- |
| `1d21df82b248` | 34 | `035611dba7f0ff9b2151e385c92d955a68cb0a96a028f6d287d27828f4d185a0` | `e6e8b0bbff3762bc1439c3890a91477b9b57d11e907e34030e793759405ecb12` |
| `904fe17b252f` | 35 | `cb7073ed1e45cdad085432e5769d5daf45e3ac674764f595a3034f77463983ee` | `0dd7b29b367ec035563600e95ee777530fb592f159016ae4c4bb28fd09d953d8` |

For these repository paths, sorting the tuples of path components reproduces
the Rust ordering. Source inventory sorting remains its separately specified
complete POSIX-name order. The current contract and original proposal are
annotated with the corrected CLI attribution. Raw test records are unchanged;
no stale-CLI/source mismatch or runtime failure is inferred from this audit
mistake. No rebuild or test replay was needed for these documentation changes.
