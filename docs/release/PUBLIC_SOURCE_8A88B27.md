# Local public file-tree checkpoint 8a88b27

The prepared source snapshot is fixed at commit `8a88b27f403ecb811548764405dc909752f94c96`, tree `b84e0beb6faf1319d769beac7b31f9a45c958114`. It includes the committed public roadmap `5c5977a`, workspace protocol `6c575f5`, typed-read slice `3aa3db0`, QA finalization fix `c09a33a` and reviewed release tooling. Current uncommitted atomic-edit and graphics work was not selected or staged.

This is a new **source-only** checkpoint. It does not correspond byte-for-byte to the old `2ef9d7806bac939114580fb634f95aad5b33d3f8` app. That app and its retained `e1b0b2f` tooling archive remain unchanged, with their separately checked 155 binary-input correspondence. No binary was built or relabeled from this newer public source, no `.git` was initialized, no remote was created, and nothing was published.

## Actual local artifacts

The corresponding-source set is `dist/source-8a88b27f403e/` (176 selected files). The prepared repository-root tree is `dist/public-source-8a88b27f403e/tree/` (178 files: those same 176 plus exactly committed `.gitignore` and `.github/workflows/ci.yml`). Its sibling `FILE_TREE.txt`, `public-tree-manifest.json` and `checksums.sha256` describe the exact output.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `source-8a88b27f403e/numjinn-source-8a88b27f403ecb811548764405dc909752f94c96.tar.gz` | 1,887,973 | `0ab1f1fb00088536db8e2d77a005a4a39ef1c8c9972da29fa73197c16b5d7f5d` |
| `source-8a88b27f403e/source-manifest.json` | 52,794 | `bee1347864aad20e78cab747f983b7923b4460368e29b08a2ba730e6a68390e8` |
| `source-8a88b27f403e/checksums.sha256` | 216 | `56077cf6eff9e5a1742327ecf3e80220050464259726b374971f908552fdbbb9` |
| `public-source-8a88b27f403e/FILE_TREE.txt` | 12,427 | `88a7ec182368a4fa6490b89d7cad4332a7b518c644dc020c6497b95e31b78398` |
| `public-source-8a88b27f403e/public-tree-manifest.json` | 59,413 | `ab71fef188a139a43fef732e8b473e33404f046da8475a6dc67aab8244fb5be2` |
| `public-source-8a88b27f403e/checksums.sha256` | 21,019 | `fd169b6f1f498adc2fd377401aae34edb87e52848de0fdea57c817ce73ba3658` |

The source inventory digest is `2b5adba1e758bf9f01505d6f66d0e0b5de8b9c1c962b8ae39df87b3b33b3a1cb`; the public-tree inventory digest is `90c4e7fe18997dc4f78710417d7a0d277affce70d0c15d6b86de946a45e4c4ca`. Every file has size, SHA-256, Git blob identity and normalized committed mode; public-tree records distinguish source-archive files from the two configuration additions. There is no checkout path, user name, wall clock or moving-ref label in the generated manifests.

The exact optional inclusions are `ROADMAP.md`, `roadmap.json`, `docs/WORKSPACE_PROTOCOL.md` and `docs/VALUE_PAGE_PROTOCOL.md`. The five mandatory minimal fixture roots are unchanged. The root README links to the canonical public roadmap. Its current implementation states remain unassessed and validation states unrun; exporting the plan does not certify any task or release profile.

## Review and verification

All 18 synthetic packaging tests passed (6.468 seconds), all 21 source-archive tests passed (6.844 seconds), and all 13 public-tree tests passed (6.732 seconds). These exercise the optional paths/minimal roots, fixed committed source/configuration, exact metadata, helper/producer pinning, forbidden payloads, standalone verification and preservation of prior outputs. No Cargo, app, numerical runtime or GUI is invoked by these synthetic suites.

Independent read-only tooling review accepted the final exporter/helper identity checks and focused regressions. The two hidden-file exceptions were reviewed separately. `.gitignore` is the exact committed 135-byte file, SHA-256 `6cbf5aafee77b3d7eb66d2ea39b36a1c459e78389eee351e2eab039ba0193683`. CI is the exact committed 3,947-byte workflow, SHA-256 `06bcd229328971ab6670d441e3d9965221c342de8c064a2e27a05b2f207cefda`; the only new workflow step runs the public-tree synthetic suite. No publishing permissions, credentials or workstation artifacts were added. Actions, images, toolchains, external dependencies and hosted workflow execution remain separate/unrun.

Actual commands:

```sh
python3 -B scripts/release-source.py create \
  --ref 8a88b27f403ecb811548764405dc909752f94c96 \
  --output dist/source-8a88b27f403e
python3 -B scripts/release-public-tree.py create \
  --source-set dist/source-8a88b27f403e --repo . \
  --output dist/public-source-8a88b27f403e
python3 -B dist/public-source-8a88b27f403e/tree/scripts/release-public-tree.py verify \
  --directory dist/public-source-8a88b27f403e
```

Creation verified both outputs. The included public-tree tool additionally verified the actual tree with Git redirection removed and an empty tool PATH, using no Git or build executable. The old archive and app checksum-list identities were rechecked and remain intact. The [public-tree contract](PUBLIC_TREE.md) and [ordinary source-build recipe](SOURCE_ARCHIVE.md) state the external prerequisites and Git-free identity limits; archive-only macOS packaging remains unsupported.

An independent audit of the actual outputs used standard-library parsing and fixed Git objects rather than the producer's verifier. It passed all 176 source files/72 archive directories (248 tar members) and all 178 public files/74 directories, exact modes/hashes/blob IDs, origin identities, listing and checksums. The two configuration bytes match their reviewed hashes. All 56 retained notice references, nine CI script references, eight literal Rust includes and 22 out-of-line modules resolve. No additional hidden files, `.git`/history, excluded research/evidence/artifacts/vendor/runtime/MAT paths or tested workstation/account/chat/credential markers were found. Dirty atomic-edit/graphics sources and fixtures are absent; the selected committed engine/desktop bytes remain distinct from concurrent live edits. The audit ran no tests/builds, changed nothing and published nothing.

A separate actual visibility/input review confirmed roadmap bytes unchanged from accepted `5c5977a` and all 193 assessment records retaining their unassessed/unrun states. The canonical README roadmap link and both engine protocol links resolve. The nine CI scripts have their inspected two local imports, five dynamic helpers, generated console-helper input, 27 fixture/oracle references and parsed JSON inputs present. Both committed QA files match `c09a33a`; all 16 typed-read source records match `3aa3db0`. This closes the inspected local workflow/input visibility review, not hosted CI execution. Historical artifact links remain intentionally unavailable rather than being copied into the tree.

## Separate frozen-candidate headless results

The later frozen language/Rustfmt and installed saved-reference replay are recorded in [the candidate run](CANDIDATE_2EF9D78_RUN.md). All 25 language cases and Rustfmt passed with unchanged frozen source and explicit compile-slot handback. Independent retained-evidence review accepted their exact scope.

Installed saved-reference replay used the unchanged old packaged CLI in an external project, retaining original packet layout, exact executable/fixture/report hashes and separate common/known-gap reports. Independent review accepted the recorded limits: common incomplete with 13 pairwise matches and the two raw forward-local unsupported comparisons; sparse-logical mismatch with two oracle failures, not a reference-file interchange pass. Both strict commands correctly exit one. The declared MAT-v7 reference bytes loaded and matched in the two actual routes for that bounded fixture. No fresh MATLAB process was launched.

GUI, Stop/debugger and real desktop state journeys are explicitly user-deferred. A2 and full-release acceptance remain open. The independent runner-cleanup follow-up is PM-owned and was not repeated here. Public source preparation, roadmap checks and selected headless evidence do not establish offline/dependency-source closure, a self-contained installer, binary reproduction, signing or public delivery.
