# Third-party provenance

Open MathLab's application source is GPL-3.0-or-later. The explicit Rust program profiles implement their own bounded language and numerical execution and do not discover, start or require GNU Octave or MATLAB. Ordinary/default execution routes retain the separately installed GNU Octave backend; they are distinct from the explicit Rust routes. The application does not translate the Octave codebase into Rust, and this repository does not redistribute an Octave or MATLAB binary. Exact Rust dependency versions are recorded in `Cargo.lock`.

The optional native debugger adapter is project-owned GPL-3.0-or-later C++ code using GNU Octave's public interpreter/event/breakpoint interfaces. `--with-debugger` builds an `.oct` module against the selected local Octave development installation. The macOS bundle includes that module, its recorded compiler runtime version, and the corresponding C++ source. It still requires a compatible separately installed Octave runtime. Those optional adapters forward execution to Octave; they do not supply the independent Rust profile algorithms or interpreter.

| Component | Role | Upstream license / source |
| --- | --- | --- |
| GNU Octave | External legacy/default execution backend; not required by explicit Rust profiles | GPL-3.0-or-later; [project](https://octave.org/about), [COPYING](https://github.com/gnu-octave/octave/blob/default/COPYING), [Homebrew runtime metadata](https://formulae.brew.sh/formula/octave) |
| egui / eframe | Native Rust desktop | MIT OR Apache-2.0; [upstream](https://github.com/emilk/egui), [MIT](https://github.com/emilk/egui/blob/0.33.3/LICENSE-MIT), [Apache](https://github.com/emilk/egui/blob/0.33.3/LICENSE-APACHE) |
| rfd | Native file dialogs | MIT; [upstream](https://github.com/PolyMeilex/rfd) |
| image | Decode figure PNG previews | MIT OR Apache-2.0; [upstream](https://github.com/image-rs/image) |
| serde / serde_json | Typed JSON session messages | MIT OR Apache-2.0; [serde](https://github.com/serde-rs/serde), [serde_json](https://github.com/serde-rs/json) |
| tempfile | Session files and lifecycle | MIT OR Apache-2.0; [upstream](https://github.com/Stebalien/tempfile) |
| clap | Command-line parsing | MIT OR Apache-2.0; [upstream](https://github.com/clap-rs/clap) |
| anyhow | CLI error context | MIT OR Apache-2.0; [upstream](https://github.com/dtolnay/anyhow) |

The Rust license expressions above should be checked against the downloaded crate metadata for the exact locked release, together with any `license-file` and notices. The `scripts/generate-notices.py` script reads Cargo metadata offline and copies available license/notice texts for the entire metadata dependency inventory into a destination you specify:

```sh
python3 scripts/generate-notices.py dist/licenses
```

By default the script inventories the actual Cargo **normal/build dependency tree** for `mathlab-desktop` and `mathlab-cli` on the configured target or Rust host. The local `aarch64-apple-darwin` build selects **160 external crates**. Inactive optional dependencies, dependencies for other platforms, and dev-only dependencies are excluded from that default tree. `--target TRIPLE`, `--package NAME`, and `--all-platforms` allow explicit inventories of other configurations.

A headless CLI distribution must select its own target-specific closure with `--package mathlab-cli --target TARGET --require-complete`. It must not reuse the desktop dependency count or claim that optional GUI/Octave components are included or required. The exact selected inventory and original license texts remain release gates; the Rust profile scope does not imply support for every MATLAB language or numerical feature.

The generated `index.json` records package names, versions, license expressions, repositories, copied texts, immutable upstream source URLs, SHA-256 hashes, and any missing texts. It also scans nested notices, including egui's bundled-font OFL, Ubuntu-font, and emoji font MIT terms. Where published crates omit their shared-workspace notices, original texts are retained under `third-party/licenses/` at the exact git commits in the crates' `.cargo_vcs_info.json`; the generator verifies the recorded file hashes before copying them. `scripts/fetch-upstream-licenses.py` is an explicit network operation for updating those retained notices after dependency changes. It does not run automatically during packaging.

The macOS package includes the original notices and corresponding project source, including `third-party/`. `--require-complete` rejects a selected dependency with no available license text/declaration supplement. These Cargo records exclude the separate Octave installation and operating-system libraries; a self-contained runtime distribution requires their own inventory and source/notices.

**Upstream notice gap: `dispatch` 0.2.0.** Its exact upstream commit `82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748` explicitly declares `license = "MIT"` and the author in Cargo metadata, but provides no copyright/license notice file. The upstream repository's current tree and license-file history also contain no such file. We retain its original [Cargo declaration](https://github.com/SSheldon/rust-dispatch/blob/82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748/Cargo.toml) and README, plus standard [SPDX MIT permission terms](https://spdx.org/licenses/MIT.html). The unassigned template copyright line is omitted from the accompanying permission text; no year or copyright holder is invented. The provenance index explicitly records this gap and distinguishes the standard terms from an original upstream notice.

## Optional scientific components

The original `examples/control_response.m` and `tests/fixtures/control_optional.m` call an optional existing Octave `control` package. No package source or binaries are copied into the repository. The package has GPLv3 licensing material; its included SLICOT routines have BSD-3-Clause notices according to the [official control repository](https://github.com/gnu-octave/pkg-control) and [COPYING](https://github.com/gnu-octave/pkg-control/blob/main/COPYING). Keep the exact selected package's file-level licenses, including SLICOT, when packaging it.

The local development installation has the separately installed Octave profile `control` 4.2.3, `signal` 1.4.8, `image` 2.20.1, `statistics` 2.0.0, `optim` 1.6.3, `datatypes` 1.5.0, and `struct` 1.0.18. The original package fixtures assess a few named APIs in the first five packages; `datatypes`/`struct` are recorded as installed dependencies. Their binaries are **not bundled** in `MathLab.app`, and the Cargo license inventory does not replace their own upstream notices. Exact executed cases appear in `artifacts/VALIDATION.md`.

Other packages from the [Octave package catalog](https://packages.octave.org/) are candidates for later curated profiles. Scilab/Xcos, OpenModelica, Nelson, SymPy, GPU runtimes, and specialized domain systems appear in the roadmap as candidates; the current product does not integrate them.

The full GPLv3 license text in this repository's `LICENSE` was copied from the installed GNU Octave 11.3.0 distribution's `COPYING`; the application is offered under GPL version 3 or any later version as stated in the workspace metadata and original source notices. Original example/fixture source is project-owned; numerical formulas and tests were written for this repository, not copied from MATLAB or Octave suites.
