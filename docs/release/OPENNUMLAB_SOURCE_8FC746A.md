# OpenNumLab source handoff 8fc746a

The coherent source pin is `8fc746a84f4db181178963c3724e7df1dac16330`, tree `bbd2deb47281c26c74971e044ac75e741565da32`. It includes OpenNumLab roadmap `0a39db3`, byte-preserved logo/PM branding `81635a8`, prospective release identity `301d428`, seven display-only desktop changes `66a72b2` and the symmetric legacy-logo verification fix. Current uncommitted atomic-edit/graphics code was not selected or staged.

The designated repository is **private** [me-farza/opennumlab](https://github.com/me-farza/opennumlab). Root owns the reviewed standalone-history upload. Release prepared the files below without initializing Git, copying internal history, setting a remote or uploading anything. Public publication is not authorized. This audit note postdates the fixed source pin; it does not change the prepared tree.

## Actual artifacts

`dist/opennumlab-source-8fc746a84f4d/` contains the schema-2 OpenNumLab corresponding-source set with 178 selected files. `dist/opennumlab-tree-8fc746a84f4d/tree/` contains the 180-file repository-root tree: those same 178 files plus exactly committed `.gitignore` and `.github/workflows/ci.yml`. Its three sibling inventory/listing/checksum files stay outside the upload tree.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `opennumlab-source-8fc746a84f4d/opennumlab-source-8fc746a84f4db181178963c3724e7df1dac16330.tar.gz` | 1,896,070 | `8e5dd3143da0cef6c17d92f0b21f5f29feface505c859e42235b6246941cb6e0` |
| `opennumlab-source-8fc746a84f4d/source-manifest.json` | 53,370 | `4de063d246f023253aa2a7934f27fd83fc750a728a842c16e596917c8181e83f` |
| `opennumlab-source-8fc746a84f4d/checksums.sha256` | 219 | `01343b8c552ee8ebcd118f04c29f601af887e0ed13ce9bfa28022ae88dc5a4ab` |
| `opennumlab-tree-8fc746a84f4d/FILE_TREE.txt` | 12,516 | `db4e7a22e1b92bcc2b49a513838244bd07a9ae5ed9428aa039f49129c241032d` |
| `opennumlab-tree-8fc746a84f4d/public-tree-manifest.json` | 60,057 | `1bb7cc9d9083bfab746a5d648ee034b1b4b9f8023c4a8c42419731dff1a37f6c` |
| `opennumlab-tree-8fc746a84f4d/checksums.sha256` | 21,240 | `d38b6eb1d92496553ba42bc23545e820422dec499b121510d713db3d38898812` |

Source inventory SHA-256: `cd307070da115c56c8e66316b4329a18bbd49cec88ad7b9c10bda718d197d035`. Repository-tree inventory SHA-256: `c068f30a3fc7d294a1766386a5e5c557a812b0916c88c3647a527654d6c3c744`. Manifests bind every path, byte length, SHA-256, Git blob, normalized mode, source/configuration origin and exact producer/helper identity. No builder path, account name, wall clock or moving-ref label is generated into them.

The only logo path is `assets/opennumlab-logo.png`; its original PNG bytes remain SHA-256 `157621469bfae6f190bfc02fd823f61ee23bef365e43632a2950ade537e75aac`. Prospective README, desktop display/help, app/release product and writable-project defaults use OpenNumLab. Internal `mathlab` interfaces and the stable bundle identifier remain unchanged. The two configuration hashes remain `.gitignore` `6cbf5aafee77b3d7eb66d2ea39b36a1c459e78389eee351e2eab039ba0193683` and workflow `06bcd229328971ab6670d441e3d9965221c342de8c064a2e27a05b2f207cefda`.

## Source-only verification and legacy preservation

All 21 packaging/launcher tests passed in 9.412 seconds. After the schema/logo symmetry follow-up, all 25 source tests passed in 10.998 seconds and all 17 repository-tree tests in 11.190 seconds: 63 synthetic cases total. Shell syntax and whitespace checks passed. Independent review accepted the new names, schema separation, exact selector/producer pins and negative v1/current-logo cases, including both-logo rejection. App/QA schema 1 remains distinct from source/public-tree schema 2.

The actual outputs were verified on creation. Their included tools additionally verified both new sets with an empty tool PATH and Git redirection removed. A separate read-only check used those same included tools for all three retained legacy sets (`e1` source 159 files, `8a` source 176 files, `8a` public tree 178 files) and both new schema-2 sets. All five subprocesses exited zero with empty stderr, using Python 3.9.6 and no Git/build executable. Every one of the 373 actual files across those source/public sets retained identical hashes, lengths and modes before/after.

An independent inspection compared the actual 178 source files and 180 repository-tree files against raw committed Git bytes, hashes, modes and exact path sets. It checked archive metadata/padding, all inventory and listing entries, and both configuration origins without using the included verifiers. A separate input-closure review resolved the nine CI scripts, their local imports and helper scripts, Rust modules/includes, fixture/oracle inputs and all 56 third-party notice references. Both reviews passed. Concurrent atomic/graphics paths, internal history and raw evidence remained excluded; roadmap statuses were unchanged.

Included verifier SHA-256 identities are `release-source.py` `b83fcb8373d77b8a6fb07e673a1092d2a63ea61b27219fc6eeefd93b12b054c2`, `release-public-tree.py` `2af45e94236b1c91af04a3f1dff4ca02130297763ae1a811594578792ff33055` and shared selector `release-package.py` `df4e0972030e76998481d0117b4865cc23ca9ad95f8ed7b75dd0bfe554bba501`. Schema 1 admits the legacy logo and rejects the new logo; schema 2 does the reverse. No existing archive, manifest, source tree, report or installed app was renamed or rewritten. The separate 697-file historical guard also remained unchanged.

Actual preparation commands:

```sh
python3 -B scripts/release-source.py create \
  --ref 8fc746a84f4db181178963c3724e7df1dac16330 \
  --output dist/opennumlab-source-8fc746a84f4d
python3 -B scripts/release-public-tree.py create \
  --source-set dist/opennumlab-source-8fc746a84f4d --repo . \
  --output dist/opennumlab-tree-8fc746a84f4d
python3 -B dist/opennumlab-tree-8fc746a84f4d/tree/scripts/release-public-tree.py verify \
  --directory dist/opennumlab-tree-8fc746a84f4d
```

## Private upload receipt and first hosted run

Root reported the standalone upload completed at private `me-farza/opennumlab`, with exactly one initial commit on `main`: `e5f5ce1b4024a30901ac830307c3ff563d11185f`. Root independently compared all 180 uploaded files, including modes, with the reviewed tree above. The remote initial commit is a new history identity; `8fc746a84f4db181178963c3724e7df1dac16330` remains the internal source-selection pin. The internal checkout has no remote and its history was not uploaded. Release independently confirmed repository privacy and the hosted run's exact head.

The first [hosted CI run, 38784271685](https://github.com/me-farza/opennumlab/actions/runs/38784271685), completed with failure at `2026-10-15T18:21:08Z`: Ubuntu synthetic contracts passed; macOS synthetic QA, native preflight and the broader Ubuntu runtime job failed. [The retained CI receipt](PRIVATE_CI_38784271685.md) records each failure and the bounded native workflow correction. No rerun or successful hosted qualification is claimed by this note. Follow-up edits postdate the fixed artifacts above.

## Qualification and handoff limits

No Cargo/native build, numerical runtime or GUI was launched by this source/branding lane. The new source is not a byte-for-byte counterpart of the old NumJinn binary and has no new binary qualification. Frozen `2ef9d78`, its completed reports and original `e1`/`8a` sets retain their historical identities and bounded results.

The actual old-candidate Save→Run stale-source **release blocker remains open**. [Prospective alpha notes](DEVELOPMENT_ALPHA.md#current-installed-release-blocker) retain its exact observed sequence, report hash and the successful basic GUI/numerics/plot/edit/input/Stop/debugger/Save journeys separately. This rename claims no fix. Source preparation does not close A2, full qualification, hosted CI, installed/lifecycle acceptance, binary reproduction, dependency-source/offline closure, signing or public delivery. Root's private upload contains the reviewed standalone file tree; internal Git history and raw evidence remain outside it.
