# OpenNumLab private-update source 5fa9b5c

The fixed source pin is `5fa9b5c36fda8709268408b1c7deee7b2091449d`, tree
`b11be685333a1499040f04540e416dde662198a4`. It includes accepted CLI/launcher
checkpoint `846365d`, installed-QA declaration checkpoint `6d9a43a`, separate CI
syntax correction `9e5bfc2`, and the four-file launcher-CI/sentinel/README slice.
The archive and sanitized tree read only this committed pin. Later shared source
work and this post-pin receipt are absent; no artifact was regenerated to add
them.

The slice adds the launcher harness to the existing Ubuntu/macOS synthetic CI
matrix with a one-minute timeout, protects its exact bytes/executable mode in
the two existing export-fixture maps, and documents all four native adapters.
`scripts/release-launcher-value-test.sh` was already admitted by the existing
`release-*.sh` rule. No selector was widened and no new export dependency is
missing. The script is 7,489 bytes, Git mode `100755`, SHA-256
`134ad167d7b80cff51ec0a1b7953d0ac813121d5a4eb53e8ed68bb9929275002`.

## Immutable local artifacts

`dist/opennumlab-source-5fa9b5c36fda/` contains 221 corresponding-project-source
files. `dist/opennumlab-tree-5fa9b5c36fda/tree/` contains those same files plus
exactly `.gitignore` and `.github/workflows/ci.yml`: 223 repository-root files.
The external manifests, listings and checksums are outside the upload tree.
Both formats remain schema 2/OpenNumLab; legacy schema-1 verification is
unchanged.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Source `.tar.gz` | 2,105,203 | `cd766e8bc83eb1f0ae25ac6704fc8d000e5d362e12546877dd76ce1682e48200` |
| `source-manifest.json` | 65,791 | `69e6c3030a3cb39f8fa4fccb3a991987485b1a69765b07b41810d4817bab461f` |
| Source `checksums.sha256` | 219 | `da451f3f0ba88d39ffe26154228449ee8dd70e0f8bbc27bc7d805722472825d0` |
| `FILE_TREE.txt` | 14,732 | `93e39f84358682f60c935a89c9296c83392fa828c7c3ed42b64b5560fb51a323` |
| `public-tree-manifest.json` | 73,940 | `b4490a9da3e5f406d917f29a6d436904c49d65f35ca4e20f9998da6cdd886cf1` |
| Tree `checksums.sha256` | 26,199 | `4c972e5464b702f6c05a3d26cd7c7b4a43e25ec8bb9d9dc2bbf52a5d22da4195` |

Archive filename:
`opennumlab-source-5fa9b5c36fda8709268408b1c7deee7b2091449d.tar.gz`.
Source inventory SHA-256:
`640845276c072d4a3d197e2a15677db3a669459be09f5dfd7e6431322ce8c932`.
Public-tree inventory SHA-256:
`f1d622d6cca8e0a2cb0e8d8ddcf1dcaaf8329dadbb9ff22b0eec8271cc1dfbd4`.

Generation used frozen matching exporter/policy bytes and an explicit full Git
commit. Both artifacts then passed their own included verifiers with empty
PATH, installed Python 3.9.6 and no available Git command. No build/runtime or
remote operation was part of generation or verification. Existing archives,
applications, private-upload history and retained evidence were not replaced.

Independent inspection matched every source/public file's bytes, modes and Git
blob ID to the raw fixed commit, plus exact archive headers/padding/gzip,
manifests, inventories and checksum/listing records. All 229 artifact files
remained unchanged during review. The private audit receipt is
`dist/opennumlab-source-preparation-5fa9b5c36fda/independent-artifact-audit.json`,
SHA-256 `888443ee20b8edcbf5d57974a13c7a2eb4fa705424e8fe6def7b65bd05e9b41a`.

## Source, test and documentation closure

All 12 inspected literal Rust include paths, the generated console helper,
local native header and all 11 script paths referenced by the proposed CI
workflow resolve inside the selected set. The launcher's new harness reads only
the selected launcher script; it creates owned shell desktop stubs and inert
text `.oct` fixtures. Its seven cases cover bundled discovery, existing and
missing explicit overrides, empty overrides, omitted support, omitted/empty
state, and preservation of the other adapter overrides. Relocated Unicode/space
paths and read-only bundle preservation are asserted without loading a runtime.

The CLI fingerprint uses 35 committed inputs sorted by Rust `PathBuf`
components, SHA-256
`c9869a61a7aabc4605d9bad79d53b8a9bfce58b9cba0468228ad6b2b60ba83cf`.
It does not identify the complete source tree or a native binary. Cargo
manifests/lock and third-party inputs remain unchanged from the previous ee67
source. All 56 retained notice-file references across 29 supplement records
resolve with matching hashes; no unresolved index entries were found.

The Markdown target audit found 239 relative references: 189 resolve to 75
selected file/directory targets. The remaining 50 references name 32 deliberately
excluded `artifacts/` paths across ten source documents. No code or product-doc
file target is missing. These developer-local evidence hyperlinks remain an
explicit documentation exception; their source references do not distribute the
evidence. An owner-approved conversion to labeled unavailable-evidence path
citations can remove those broken hyperlinks later. Do not admit raw evidence
or rewrite historical reports to close them.

The selected source includes the committed transactional edit, desktop
controller/file-action and graphics inputs. The older ee67 receipt's exclusion
of then-pending source must not be applied to this new set. Inclusion is source
distribution, not acceptance of all those features or a complete CP03 claim.

## Focused execution receipt and terminal handback

Only the authorized focused slice ran; the 95-test suite was not replayed:

- Seven launcher scenarios completed.
- One changed exact-mode source fixture and one changed exact-mode public-tree
  fixture passed.
- YAML/matrix/exact-step checks and four separate `bash -n` invocations passed.
- All eleven frozen input records matched before/after and the committed bytes
  and original modes. Minimum sampled free disk was 1,862,451,200 bytes, above
  the 629,145,600-byte floor.

Private receipt: `dist/opennumlab-private-slice-846365d/report.json`, SHA-256
`87d2f5496b6033cc4725d5ed6a6cd56f3c57bc8c3073afd800de292e488f7b3b`.
Execution session `46318` exited 0; roots `9554`, `9827`, `9871`, `9931`, `9937`,
`9941`, `9945`, `9950` have empty recorded live-owned-handle lists.

Source preparation receipt:
`dist/opennumlab-source-preparation-5fa9b5c36fda/report.json`, SHA-256
`249d5e2ef52f2b3c1a349f7cb5fd042e406d16fa6f16a882e74ca367b7c71ec0`.
Execution session `10276` exited 0; roots `11482`, `11737`, `11751`, `11757` are
terminal. Minimum sampled free disk was 1,853,300,736 bytes. All command logs,
guard source, disk samples and frozen inputs are retained. The synthetic/source
preparation lease is returned; no further workload is active here.

## Private-update plan awaiting payload review

The existing standalone checkout remains untouched and clean at
`566f75b413eaa328a29c9849d86e4dfa570e5ffb`, with exactly three commit objects and
183 tracked files. Its origin is private `me-farza/opennumlab`; PM rechecked
PRIVATE visibility and remote main at the same tip. No fetch, push, visibility
change, new standalone commit or checkout copying was performed here.

The reviewed flat payload changes 183 to 223 files: 40 additions, 31 modified,
152 identical, zero deletions and zero mode changes. The proposed next standalone
commit must have sole parent `566f75b413eaa328a29c9849d86e4dfa570e5ffb` and exact
sanitized root tree `ebaf448f52189fab9a69efab14a6d2ff1926e7e5`. No future commit
SHA exists yet.

After PM accepts the payload, copy only the verified `tree/` contents and append
one standalone child with explicit reviewed paths. Independently compare its
committed/worktree bytes and modes to the manifest. Do not copy external
manifests/listings/checksums, the source tar or internal `.git`, and do not
cherry-pick/merge internal history. A later push would start the existing CI
workflow; remote mutation and CI execution remain separate, ungranted actions.
No historical hosted result, installed app, optimized build or full-product
qualification transfers to this source set.

## Current-state supplement, 2026-10-15

The preceding preparation section remains a capture-time historical record.
After payload acceptance, the standalone checkout was updated and verified clean
with exactly 223 files and four standalone commit objects. Its prepared local
commit is `ccb4a01b98d8fb824320e482e77aa85979008b92`, with sole parent
`566f75b413eaa328a29c9849d86e4dfa570e5ffb` and exact reviewed tree
`ebaf448f52189fab9a69efab14a6d2ff1926e7e5`. No internal history was imported.

Automatic approval review blocked the default-main push because delegation
messages did not satisfy its requirement for direct human authorization to
update the remote and trigger CI. The push command was not executed, and no new
CI run was started by this operation. The prepared commit and failed-attempt
record are preserved at
`dist/opennumlab-private-update-5fa9b5c36fda/private-update-receipt.json`, SHA-256
`bb6fc94bd34f534d63296c6aed06af3cf97dbb3dd04f0b16f1312d8779d32c25`.
Remote writes and dispatch remain held. PM owns the human-approval follow-up and
any later push, with fresh PRIVATE/head checks. No alternative push path was
attempted. The immutable `5fa9b5c` artifacts remain unchanged.
