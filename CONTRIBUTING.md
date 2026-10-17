# Contributing to OpenNumLab

OpenNumLab is preparing its first `v0.1.0-alpha` development preview. We welcome
small, reproducible improvements to scientific workflows, Rust execution,
documentation and installation. Read the [README](README.md) and
[alpha notes](docs/release/V0.1.0_ALPHA.md) for the implemented scope, then use the
[roadmap](ROADMAP.md) to connect larger proposals to the long-term product.

## Set up a development checkout

Use a Rust toolchain compatible with `Cargo.lock` (workspace minimum: Rust 1.88),
your platform compiler/linker and the desktop libraries listed by
`scripts/setup.sh`. Start in the repository root:

```sh
./scripts/setup.sh
cargo build --locked -j1 -p mathlab-cli -p mathlab-desktop
cargo run --locked -j1 -p mathlab-cli -- doctor
cargo run --locked -j1 -p mathlab-desktop
```

The setup script prints guidance and reports tools; it does not install them.
Running the main application requires a separately installed GNU Octave. Figure
export needs a working graphics toolkit and fonts. Native adapters additionally
need matching Octave headers, `mkoctfile` and a C++ compiler. Use
`MATHLAB_OCTAVE=/absolute/path/to/octave-cli` to select a runtime explicitly.
Archive builds have a separate [source-build recipe](docs/release/SOURCE_ARCHIVE.md#ordinary-builds-after-extraction).

## Find the code

| Area | Responsibility |
| --- | --- |
| `crates/mathlab-desktop` | Rust desktop panels, editor, workspace and session UI |
| `crates/mathlab-cli` | Command parsing, batch execution, REPL, diagnostics and conformance commands |
| `crates/mathlab-engine` | Octave process/protocol, source and value lifetimes; experimental Rust session owner |
| `crates/mathlab-language` | Source preparation/mapping and bounded Rust expression parsing/evaluation |
| `crates/mathlab-array` | Rust array representation and arithmetic |
| `crates/mathlab-simulation` | Experimental bounded discrete-accumulator library |
| `crates/mathlab-debugger`, `native/`, `octave/` | Debugger transport, optional Octave adapters and worker source |
| `compatibility/`, `tests/fixtures/` | Original behavior cases, mathematical oracles and declared compatibility subsets |
| `scripts/`, `docs/release/` | Build, packaging, installed QA and source-distribution contracts |

The default desktop and CLI session run through Octave. Rust arithmetic is an
explicit experimental route. Preserve backend identity and unsupported-input
errors; changes to backend defaults require separate conformance and performance
evidence. A working button, dependency or isolated fixture does not establish
whole-feature compatibility.

## Choose a first contribution

- Reproduce a small bug with an original script, expected result and actual
  result. A case covering edit → save → run, error → recovery, or Stop → next
  command is especially useful when it exposes a specific failure.
- Improve setup or diagnostics for one named platform/runtime. Report the actual
  missing prerequisite and preserve existing project files on failure.
- Add a focused numerical or file-format case with an independent expectation.
  Record class, shape, storage order and tolerance or exact bits as appropriate.
- Improve documentation or an example by checking it against the implemented
  command/API. Keep toolbox, simulation and MATLAB-interface goals distinct from
  tested capabilities.

For a larger API, UI workflow, dependency or language extension, open a proposal
before implementation so its scope, ownership and acceptance cases can be agreed.
There is no requirement to rewrite an existing component just to contribute.

## Check a change

Run the smallest relevant checks first. These examples exercise Rust code without
starting Octave:

```sh
cargo test --locked -j1 -p mathlab-array --lib
cargo test --locked -j1 -p mathlab-language --test session_expression_v1
cargo test --locked -j1 -p mathlab-simulation --lib
cargo test --locked -j1 -p mathlab-engine --lib rust_session::tests
```

For the workspace and runtime profile:

```sh
CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1 ./scripts/verify.sh
CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1 ./scripts/verify.sh --require-octave
```

The verifier checks formatting, compilation and tests. With Octave available it
requires a matching SDK and builds the value adapter before runtime tests.
Without Octave the default run explicitly skips the runtime fixtures;
`--require-octave` makes absence fail. Optional `--with-control`,
`--with-toolboxes` and `--with-debugger` profiles require their respective
prerequisites. A skipped test is not evidence of runtime support. See the
[native guide](native/README.md) before changing an adapter or protocol.

Keep `Cargo.lock` changes intentional. Use `cargo fmt --all -- --check` to check
formatting; format only files you own while another contributor is working in the
same checkout. Run compiler/native/package work serially on constrained machines.
When a check cannot run, report the reason and the unverified behavior.

## Report and submit

Use repository issues for bug reports and pull requests for changes. Include the
source revision, OS/architecture, Rust and Octave versions, selected backend,
optional adapters/packages, minimal input, expected result, actual result and
exact command. Remove secrets, personal paths and private data from logs. Screenshots
help UI reports, but also describe the action sequence and final state.

Keep each pull request focused. Explain what users can do after the change, why
it is needed, and which checks actually ran. Include failure and recovery cases
when state or data can change. Changes to numerical, protocol, native, packaging
or release behavior need independent review of both the source and relevant
results. Preserve existing source and user data during failed operations.

The project source and original examples/fixtures use **GPL-3.0-or-later**; see
[LICENSE](LICENSE). Keep applicable source notices and record dependency/license
changes in [THIRD_PARTY.md](THIRD_PARTY.md). Contribute code and fixtures you have
permission to share. Use public interfaces, documentation and original tests for
interoperability work; do not submit proprietary implementation code, restricted
datasets or unreviewed third-party payloads.
