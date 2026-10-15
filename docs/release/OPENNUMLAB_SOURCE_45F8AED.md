# OpenNumLab source follow-up 45f8aed

The fixed source pin is `45f8aed5ce12c7a0e7bd4e2c6fab2f03c22cba2a`, tree `c67ac60b7d12ec7e5a07336b33c7cc82c9dab44c`. It combines saved-script source-generation changes `80cb632`, native CI preflight/log retention `a239bc4`, and portable synthetic flood output `45f8aed`. This receipt postdates that pin. The artifacts were created once and were not regenerated to include this note.

The prior [8fc source/tree](OPENNUMLAB_SOURCE_8FC746A.md), original private commit `e5f5ce1b4024a30901ac830307c3ff563d11185f`, and [failed first CI run](PRIVATE_CI_38784271685.md) remain separate and preserved. No local Cargo/native build or real runtime/GUI was launched by this source-preparation lane.

## Immutable artifacts

`dist/opennumlab-source-45f8aed5ce12/` contains 180 source files. `dist/opennumlab-tree-45f8aed5ce12/tree/` contains those same files plus exactly `.gitignore` and `.github/workflows/ci.yml`: 182 repository-root files. Inventory/listing/checksum metadata stays outside the upload tree.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Source `.tar.gz` | 1,906,964 | `89f63408fd7c11913e71077f41203013e20d0484e5f9be54ebfd41ba23da92e1` |
| `source-manifest.json` | 53,939 | `c1a66f530fa3cd536938fd8b72ea1884dc1493ab68386272457567e03b58c889` |
| Source `checksums.sha256` | 219 | `24ba617a8ef96c40621f7c13600806de408cab7f7a5f0c9dfb8c1ef3f5746ea5` |
| `FILE_TREE.txt` | 12,607 | `dea1eb2c6126b98361416f26e350800c054cf1b1bb084afa538eede48c58348f` |
| `public-tree-manifest.json` | 60,694 | `533bc6fff19786dbad8fcb3ea6e9489512848a1f0839fb0e2025ba179c1115fb` |
| Tree `checksums.sha256` | 21,463 | `e38e8f27d669c3d2d66ec2eeaf248b727d1dfbcbf20b2b6d5976590e6740e5b3` |

The archive filename is `opennumlab-source-45f8aed5ce12c7a0e7bd4e2c6fab2f03c22cba2a.tar.gz`. Source inventory SHA-256 is `23e9f2547ad7e8933b89ac71fc0e11ac0a2187aa0f12c1f4f34e030dae6bac93`; repository-tree inventory SHA-256 is `eb388919d64dbff48d4cc882491ba9b7b1cb7ebbc8525f1c28eed445a8eb904f`. The 4,392-byte workflow SHA-256 is `c19c65aa5b5482a0d9ad5dea294029b1b99af33bf61d97bed15ac231301a4ef0`; `.gitignore`, logo, roadmap bytes and source-selection/verification tools remain unchanged from 8fc.

## Review and prepared private update

Independent actual-output/input review matched all source and repository paths, inspected raw committed code/QA/configuration blobs, and resolved the nine CI scripts, local imports/helpers, Rust includes/modules, fixtures and all 56 notice references. The newly included verifiers passed for both new sets with empty PATH, Python 3.9.6 `-B`, and no inherited Git redirection. They also verified both retained 8fc sets. All 374 files across those four sets retained hashes, sizes and modes; every one of the separate 697 historical guard entries also matched before and after. Concurrent atomic/graphics work, internal history and raw evidence are excluded.

Release prepared a clean standalone checkout with local commit `5fb7fba837ff42334d0e728a91ddbf6b59e081e4`, parent `e5f5ce1b4024a30901ac830307c3ff563d11185f`. Its 182 files match the reviewed tree, with 13 changed paths and exactly two commits in its private history. It imports no internal ancestors. Preparation did not push. Root independently reviews the payload and source-owner evidence before any push to private [me-farza/opennumlab](https://github.com/me-farza/opennumlab). Local checkout and change details are recorded outside the exported source in `dist/opennumlab-ci/38784271685/private-update-45f8aed-prepared.json`.

All 13 native-gate synthetic cases and the four isolated CI launch-block checks passed for the native workflow edit. PM reported all 15 QA synthetic tests passing under Python 3.14 after the explicit producer flush, with focused Python 3.9 regression coverage. The production QA Runner was unchanged. These checks do not replace hosted execution.

## Remaining boundaries

The general Ubuntu job's Octave 8.4 startup incompatibility (`33554432.0` versus Rust `usize`) is unresolved in this pin: worker/protocol bytes remain those from 8fc. The smoke test stays enabled. Root requested this reviewed follow-up payload before deciding its private push; no successful follow-up CI run is claimed.

Source-generation changes do not repair the immutable old NumJinn app. Its observed Save→Run stale-source GUI blocker remains open until a new candidate is built and exercised. This source set has no new installed/binary correspondence, clean-machine, lifecycle, signing, full numerical/MATLAB conformance or MATLAB muscle-memory/UI parity qualification. Repository visibility remains private; no public tag or release was prepared.
