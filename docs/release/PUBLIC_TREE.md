# Sanitized repository file-tree preparation

`scripts/release-public-tree.py` prepares a local reviewable repository file tree from one verified corresponding-source archive plus exactly committed `.gitignore` and `.github/workflows/ci.yml`. It never initializes Git, copies history, creates a remote or publishes anything. Review those two configuration files at the selected source commit before running it; no other hidden files or workflow directory contents are admitted.

The prospective product is OpenNumLab, with private destination [me-farza/opennumlab](https://github.com/me-farza/opennumlab). Root owns the reviewed standalone-history upload. The tool's historical `public-tree` filename does not change repository visibility or authorize public publication. New source/public-tree formats use schema 2 and OpenNumLab product/archive identities; legacy schema-1 NumJinn source/public sets remain verifiable under their original identities.

First finish and commit the intended source, QA, producer/policy and documentation checkpoints. Run the scripts from that coherent committed version. Create the source set only after its inputs are ready, then use its immutable commit for the configuration files:

```sh
python3 -B scripts/release-source.py create \
  --ref FULL_SOURCE_COMMIT --output dist/source-FULL_SOURCE_COMMIT
python3 -B scripts/release-public-tree.py create \
  --source-set dist/source-FULL_SOURCE_COMMIT --repo . \
  --output dist/public-source-FULL_SOURCE_COMMIT
python3 -B scripts/release-public-tree.py verify \
  --directory dist/public-source-FULL_SOURCE_COMMIT
```

Creation obtains the commit/tree from the verified source manifest and reads raw configuration blobs from that same commit, with inherited Git redirection/replacements disabled. It does not use configuration bytes from a dirty worktree or follow a moving ref. The exact public-tree producer must be included in the selected source archive. Existing output names are refused; private staging is verified before an atomic rename into a newly reserved empty directory. Failure cleanup never recursively removes a prior output.

The output has four items:

- `tree/`: repository-root files from the source archive, plus the two exact configuration files with committed executable modes normalized to `0644`/`0755`. No `.git` metadata is present.
- `FILE_TREE.txt`: a deterministic sorted directory/file listing for the prepared tree.
- `public-tree-manifest.json`: canonical external inventory with per-file size, SHA-256, Git blob, mode and origin; upstream commit/tree; exact source archive/manifest identities; and producer identity.
- `checksums.sha256`: relative-path SHA-256 records for the tree and external inventory/listing, excluding itself.

Standalone verification needs Python's standard library and the included release helpers. It checks exact files/directories, modes, hashes, portable extraction names, producer identity, canonical metadata/checksums and the narrow hidden-file exceptions. These internal consistency checks do not authenticate a signature or independently prove the omitted repository history. Retain the approved manifest/checksums with any separately authorized later delivery.

## Public documentation and workflow visibility

`ROADMAP.md` and `roadmap.json` are exact optional source inclusions; their task states remain unassessed and their source/installed validation states remain `not_run`. `docs/WORKSPACE_PROTOCOL.md` and `docs/VALUE_PAGE_PROTOCOL.md` are exact optional inclusions resolving the engine README's published protocol links. None enlarges the five mandatory minimal fixture roots, and no general `docs/` or JSON wildcard is added. Historical artifact links do not authorize copying raw logs or research.

Root `CONTRIBUTING.md` is another exact optional inclusion. The single data path `crates/mathlab-language/tests/fixtures/operator_native_reference_v1.provenance.json` is included to preserve the portable provenance cited by its Rust fixture. This admits neither other crate JSON nor research/evidence directories. Existing path, hidden-file and binary denials still take precedence. Both files use committed bytes from the selected source revision; their absence does not enlarge the mandatory fixture roots. Source inclusion records provenance and contributor guidance, not new runtime or release acceptance.

The reviewed CI workflow is read-only with respect to repository permissions and does not publish a release. Its release scripts/tests, QA scripts, helper modules and checked fixtures must be present in the selected source tree. The native job's report output remains ignored by the exact `.gitignore`. Actions, image pulls, toolchains and OS dependencies are external inputs; preparing the workflow is not evidence it passed. The [first private hosted run failed](PRIVATE_CI_38784271685.md), with its original evidence retained separately from follow-up fixes. The public-tree producer/test filenames use the existing `release-*` source rule, and schema version 2 is defined by their committed code and this document. App and QA report schema 1 contracts remain separate.

## Source and binary identities stay separate

Frozen binary candidate `2ef9d7806bac939114580fb634f95aad5b33d3f8` keeps its 155 frozen source inputs identified separately. [Checkpoint e1b0b2f](SOURCE_E1B0B2F.md) records their checked correspondence within that retained 159-file tooling archive. A newer public source tree can contain CP02/typed-read changes and newer QA/release tooling. It does **not** correspond byte-for-byte to that older app, and the app is not relabeled or rebuilt by this preparation.

Ordinary Git-free source builds retain the prerequisites and honest revision handling in [SOURCE_ARCHIVE.md](SOURCE_ARCHIVE.md). Archive-only macOS app packaging remains unsupported. Dependency-source/offline closure, installed/GUI acceptance, signing and publication require separate evidence or authorization; full qualification remains open.
