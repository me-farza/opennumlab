# OpenNumLab startup-correction source ee67bcc

The fixed source pin is `ee67bcca4c095210517207e9498d4ff743e5440a`, tree `acd7d31ca9cd6694b6c38cf06f89f87efa5d8159`. It retains accepted saved-source, native CI and synthetic QA corrections, then adds the separately scoped three-file startup correction. The worker emits its three fixed capability integers through builtin `uint32`; Rust protocol parsing and limits remain strict. Actual Octave 8.4 execution is pending hosted CI.

This receipt and the [development GUI cache proposal](DEVELOPMENT_GUI_CACHE_PROPOSAL.md) postdate the fixed pin and are absent from this prepared payload. No source artifact was regenerated to include them. The proposal is unimplemented and does not authorize a build. No Cargo/native job, runtime or GUI was launched by this source-preparation lane.

## Immutable artifacts

`dist/opennumlab-source-ee67bcca4c09/` contains 181 selected source files. `dist/opennumlab-tree-ee67bcca4c09/tree/` contains those same files plus exactly `.gitignore` and `.github/workflows/ci.yml`: 183 repository-root files. External manifests/listings/checksums stay outside the upload tree.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Source `.tar.gz` | 1,909,384 | `c058b25ac1a03f8313b76a3718950e21ac9a48240eabfa0023e61a8a011e087b` |
| `source-manifest.json` | 54,226 | `00bbededf4d28c95156708c9299e97ce94164cd8b600e0c581c3df4999d9eed9` |
| Source `checksums.sha256` | 219 | `4073073d76af4ed3b3fd983cdc3ae01a41285968045572dcf7db395364049839` |
| `FILE_TREE.txt` | 12,654 | `7bdd56af5fb31d9fa7b6f9a3d44198174b26bc0c5991f448805b7d161db12861` |
| `public-tree-manifest.json` | 61,015 | `02c165f21d6b5b93edbb4ff2780269fa37470bbe582d49c500972a026bc94c81` |
| Tree `checksums.sha256` | 21,576 | `75ca6331de83384e2c03d91f2b579072b91375a6efa812b7e0d94613aa10736c` |

The archive filename is `opennumlab-source-ee67bcca4c095210517207e9498d4ff743e5440a.tar.gz`. Source inventory SHA-256 is `b849a58a776d602411e936be9bf9fc2adcd6cacb9017d2101a213157a1273330`; repository-tree inventory SHA-256 is `0b74fe4c9ca1549535fc7192e86be68165395c3125337d165c41c1402770294d`. The selector/verifiers, workflow, `.gitignore`, logo and roadmap bytes are unchanged from the [45f set](OPENNUMLAB_SOURCE_45F8AED.md).

## Review and standalone history

Independent actual-output/input review matched the source/tree path sets and inspected raw committed code/QA/configuration blobs, CI scripts/imports/helpers, Rust modules/includes, fixture/oracle inputs and all 56 notice references. Both new sets passed their included verifiers with empty PATH, Python 3.9.6 `-B`, no inherited Git redirection and empty stderr. The retained 45f sets passed too; all 377 actual files across those four sets retained hashes/sizes/modes. All 697 historical guard entries still match. Pending atomic/graphics work, PM-private records, raw evidence, Git metadata and binaries are excluded.

The prepared private checkout now has third standalone commit `566f75b413eaa328a29c9849d86e4dfa570e5ffb`, parent `5fb7fba837ff42334d0e728a91ddbf6b59e081e4`, then original `e5f5ce1b4024a30901ac830307c3ff563d11185f`. It preserves both prior commits without rewriting or importing internal history. The new committed/worktree payload matches all 183 reviewed files and modes and is clean. Exactly four paths changed from 5fb: the three startup files and the new 45f receipt. No push was performed. Root independently verifies before pushing to private [me-farza/opennumlab](https://github.com/me-farza/opennumlab).

The external local-preparation receipt is `dist/opennumlab-ci/38784271685/private-update-ee67bcc-prepared.json`, SHA-256 `ba97b200d8987bd27ddf580b8241f314022920100058667444f171b9c4b5308c`. Earlier source/tree artifacts, the original failed hosted run and historical apps/reports remain immutable.

## Validation scope

PM independently accepted the source owner's local correction proof: 98 frozen inputs, three preserved executables and 18 output logs matched final report SHA-256 `28b5cb628607f131a608bce356740dd7bb154a49e195b07f14ac3fafe87f19e4`. Observed local coverage was 62 engine unit entrypoints, one selected CLI smoke test with nine filtered tests, and a separate direct preserved-CLI run passing seven core fixtures/nine features under Octave 11.3. Formatting and strict Clippy passed. The selected smoke's `CARGO_BIN_EXE` child path was mutable; its entire child closure was not preserved. The separate direct CLI fixture run is bound to preserved executable bytes. Prior failed/aborted evidence remains retained.

This is source-level acceptance with bounded local proof. It does not establish actual Octave 8.4 compatibility, hosted CI success, a new installed GUI candidate, optimized-release reproduction, full numerical/MATLAB conformance, MATLAB interaction parity, clean-machine/lifecycle acceptance, signing or public delivery. The old app's Save→Run failure remains historical and unresolved for that immutable app.
