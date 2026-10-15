# Corresponding project-source archive

`scripts/release-source.py` creates an allowlisted OpenNumLab project-source set from one explicit committed Git ref. It does not compile, launch an application, publish anything, distribute Git history, or close external dependency/source obligations. Historical NumJinn candidate `2ef9d78` and its build outputs remain separate and unchanged.

Run the exporter and selection policy from a coherent committed checkout. Both exact script versions must be present at the selected ref; uncommitted or mismatched producer code is rejected.

```sh
python3 -B scripts/release-source.py create \
  --ref FULL_COMMIT_ID --output dist/source-FULL_COMMIT_ID
python3 -B scripts/release-source.py verify \
  --directory dist/source-FULL_COMMIT_ID
```

The output is a new directory with three files:

- `opennumlab-source-<full-commit>.tar.gz`: selected original project source and retained notices under one matching root directory.
- `source-manifest.json`: upstream commit/tree, exact per-file path/size/SHA-256/Git blob/mode, inventory digest, producer/policy hashes and normalization parameters.
- `checksums.sha256`: SHA-256 of the archive and manifest, using relative basenames only.

Existing output directories are refused. Assembly and verification finish in private staging before an atomic rename into a newly reserved output name. An injected failure removes only the exporter's empty reservation and private staging; existing source, apps, prior archives and user files are preserved.

New OpenNumLab sets use source schema version 2. Verification also accepts the exact legacy schema-1 identity `NumJinn` with `numjinn-source-<commit>` names; mixing a product, version or archive prefix is rejected. Logo admission is symmetric: schema 1 accepts only the legacy logo, and schema 2 accepts only the canonical OpenNumLab logo. Both-logo payloads are rejected. Historical `e1b0b2f` and `8a88b27` source/public sets retain their original bytes, manifests and names; neither becomes OpenNumLab runtime evidence.

The exporter reads raw blob objects, not the working tree or `git archive`. It resolves a ref once, disables Git replacements and ignores inherited Git redirection/configuration. Deleted or dirty local files, branch movement after resolution, `export-ignore`, `export-subst`, filesystem timestamps and destination names cannot change the fixed inputs. The reviewed `allowed_source` and `inventory` policy is shared with app packaging. Selected symlinks/gitlinks, unsafe names and case/Unicode-normalization extraction collisions fail before publication of the source set.

The exact optional paths `ROADMAP.md`, `roadmap.json`, `docs/WORKSPACE_PROTOCOL.md` and `docs/VALUE_PAGE_PROTOCOL.md` are included when committed. They do not enlarge the five mandatory minimal fixture roots or permit neighboring documentation by wildcard. The public roadmap remains planning data with unassessed implementation and unrun validation states. For a separately prepared repository file tree with exactly two reviewed committed configuration files, see [PUBLIC_TREE.md](PUBLIC_TREE.md).

Archive entries sort by complete relative POSIX names. Tar timestamps are zero, owner/group IDs are zero, owner/group names are empty, directories are `0755`, and files are `0644`/`0755` from committed Git modes. Gzip uses level 9, timestamp zero and no original filename. The manifest has canonical JSON and no wall clock, checkout/output path, user name or moving-ref label. It records Python/zlib versions because byte-identical compression is qualified for the recorded exporter/runtime implementation; arbitrary future compression-library versions are not promised identical bytes.

Verification uses only Python's standard library and needs neither Git nor build tools. It checks the three-file set, checksums, archive bytes/size/header, every member/type/owner/time/mode, complete tar end padding, exact inventory and data hashes, recomputed Git blob IDs, portable names and included exporter/selector identity. It rejects file/directory prefix conflicts and requires consistent repository object-ID lengths. It never extracts files. These are internal consistency checks, not a signature or independent proof of source authorship; the recorded commit/tree linkage comes from the committed export operation and cannot be reconstructed from an allowlist that omits repository files. Preserve the approved checksum/manifest through any later publication channel.

No `.git`, internal history, research/evidence/artifacts, generated MAT/SLX files, cached builds or runtime/vendor binaries are selected. Generic platform paths in authored source/build examples remain source text; the producer does not record a builder's local checkout or user-home path. Historical developer-local artifact links remain unresolved by design. The manifest describes the allowlisted project source, not every internal repository file or a complete dependency source distribution.

## Ordinary builds after extraction

Obtain all three files and verify the set before extraction, using a reviewed copy of the exporter. Extract into a new directory **outside every Git worktree**. Do not copy a `.git` directory into it or initialize a repository and label that new commit as the upstream revision.

```sh
python3 -B /path/to/reviewed/scripts/release-source.py verify --directory ./source-set
mkdir ./source-build
tar -xzf ./source-set/opennumlab-source-FULL_COMMIT_ID.tar.gz -C ./source-build
cd ./source-build/opennumlab-source-FULL_COMMIT_ID
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY
unset GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_NAMESPACE
cargo build --release --locked -p mathlab-cli
# Include the desktop when its platform build dependencies are prepared:
cargo build --release --locked -p mathlab-cli -p mathlab-desktop
```

No Cargo/native/application build is performed by export or verification. The ordinary build recipes above require a compatible Rust toolchain (the workspace currently declares minimum 1.88), a linker/platform SDK, and the locked Cargo registry/git dependencies in a prepared cache or available for download. `--locked` preserves dependency resolution; it does not vendor inputs or establish offline closure. Desktop system libraries/toolkit requirements are described by `scripts/setup.sh`. Target/toolchain and external dependency versions still need actual build evidence.

The CLI build script tolerates no Git: it computes a selected-source SHA-256 and reports binary Git revision as `unavailable`. That is the honest archive-build identity. `source-manifest.json` separately records the upstream archive commit; do not substitute that assertion for an observed binary Git revision. Extracting inside another Git repository or supplying Git redirection variables can make the current build script discover an unrelated checkout, so avoid both. A future export-aware binary identity contract can improve this without inventing history.

Running the scientific application additionally requires external GNU Octave, a working graphics toolkit/fonts (normally gnuplot for this alpha), and any selected scientific packages. An optional native adapter build uses `bash scripts/build-debugger.sh` with matching Octave headers, `mkoctfile` and compiler; it must be rebuilt/loaded for the actual OS, architecture, version and ABI. These prerequisites are not bundled by the source archive.

## App packaging remains a separate recipe

`scripts/package-macos.sh` currently requires an exact clean committed Git checkout and creates a separate frozen build export. It is **not supported from this Git-free archive**. Use the selected original checkout for that recipe, or implement and review an export-aware packaging contract later. A fresh synthetic Git repository would have a different local identity and must never masquerade as the upstream commit. This milestone deliberately separates a normal source build from app assembly.

The destination [me-farza/opennumlab](https://github.com/me-farza/opennumlab) remains private. Root completed the [reviewed initial standalone-history upload](OPENNUMLAB_SOURCE_8FC746A.md); these tools perform no remote action. The [first hosted CI run failed](PRIVATE_CI_38784271685.md). Public publication is not authorized. Final/offline dependency and license-source closure, actual archive builds, binary reproduction, new native/installed acceptance and signing remain pending. The developer checkout, detached historical candidate and internal history remain private working material.
