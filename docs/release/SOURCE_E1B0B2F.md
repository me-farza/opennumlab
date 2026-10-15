# Local corresponding-source checkpoint e1b0b2f

The actual local source set was generated from explicit committed revision `e1b0b2fabced57a9e80790e0ea562a2c67dcdea3`, tree `fbfad7d8736edaf21ed3996f6bf01d73f0275c82`. This is a source-export tooling checkpoint, separate from frozen app candidate `2ef9d7806bac939114580fb634f95aad5b33d3f8`; that candidate was not changed.

The reviewable set is under `dist/source-e1b0b2fabced/`:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `numjinn-source-e1b0b2fabced57a9e80790e0ea562a2c67dcdea3.tar.gz` | 1,641,742 | `19f167d4578061af6b657817715f02ad5c91076fa3754346940494558d3d4084` |
| `source-manifest.json` | 48,030 | `1ea3df84b667df6cb875078afe2cd6c7a67475db64712b877152431746aa8fdc` |
| `checksums.sha256` | 216 | `36d3b97f4501b8dc2b4e12f5bdd373a9c7950b4ea3bcd04f18713c0b94e4161b` |

The manifest inventories 159 selected files with exact paths, byte lengths, SHA-256 hashes, Git blob identities and normalized Git executable modes. Its inventory digest is `b08692eadd0ffbeef472dacdf32ba561e31e9d7800305da988a07a1ae2ef443a`. Recorded production used Python 3.9.6 and zlib 1.2.12. The exact exporter and selector are included and identified by hash.

Actual commands:

```sh
python3 -B scripts/release-source-tests.py
python3 -B scripts/release-source.py create \
  --ref e1b0b2fabced57a9e80790e0ea562a2c67dcdea3 \
  --output dist/source-e1b0b2fabced
python3 -B scripts/release-source.py create \
  --ref e1b0b2fabced57a9e80790e0ea562a2c67dcdea3 \
  --output dist/source-e1b0b2fabced-reproduction
```

All 21 synthetic tests passed in 7.948 seconds. They exercise fixed committed inputs, dirty worktrees, moving refs, ignored Git replacement/attribute behavior, exact metadata, excluded payloads, portable path collisions, symlink/gitlink rejection, output preservation, injected write failures, false blob IDs, extra members, hidden member/end-padding data, missing tar termination, and verification without Git/build tools. No Cargo/native/application build is part of these tests.

Both independently assembled output directories contain byte-identical archive, manifest and checksum files. The primary set was additionally extracted into a new temporary directory outside Git ancestry. Its 159-file inventory matched the manifest exactly; the included exporter then verified the primary set with an empty tool PATH and Git redirection removed. This confirms extraction and standalone source-set verification, not an ordinary Cargo build or binary reproduction.

An independent read-only audit of the actual archive, using standard-library parsing and fixed Git objects rather than the producer's verifier, also passed. Its 159 regular files and 71 directories (230 tar members) match the commit, manifest and normalized metadata. All 56 retained notice references resolve and match their hashes; five literal Rust includes, 19 out-of-line modules and 31 explicit core build/QA/source inputs are present. No excluded payload path/suffix or actual workstation/account/chat marker was found. Generic temporary-directory references in authored operator/history documentation remain source text; raw proof payloads are absent. This audit ran no build or runtime.

The [source archive recipe](SOURCE_ARCHIVE.md) documents ordinary `cargo build --release --locked` with external dependencies. Archive binaries must report Git revision as unavailable rather than invent a checkout identity. Archive-only macOS packaging is unsupported; app assembly still requires the exact original committed checkout. Cargo dependency sources, compiler/linker/SDK, Octave, graphics/fonts and optional packages remain external. Offline/dependency/license-source closure, actual extracted-source builds and installed qualification remain pending. Nothing was published.
