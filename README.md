<p align="center">
  <img src="assets/opennumlab-logo.png" alt="OpenNumLab logo" width="160">
</p>

<h1 align="center">OpenNumLab</h1>

<p align="center">Open-source scientific computing for MATLAB-style scripts.</p>

<p align="center">
  <a href="https://github.com/me-farza/opennumlab/stargazers"><img src="https://img.shields.io/github/stars/me-farza/opennumlab?style=flat&amp;logo=github" alt="GitHub stars"></a>
  <a href="https://github.com/me-farza/opennumlab/forks"><img src="https://img.shields.io/github/forks/me-farza/opennumlab?style=flat" alt="GitHub forks"></a>
  <a href="https://github.com/me-farza/opennumlab/issues"><img src="https://img.shields.io/github/issues/me-farza/opennumlab?style=flat" alt="Open issues"></a>
  <a href="https://github.com/me-farza/opennumlab/blob/main/LICENSING.md"><img src="https://img.shields.io/badge/license-MIT%20%2F%20GPL-blue?style=flat" alt="MIT and GPL license options"></a>
</p>

OpenNumLab is building an independent scientific computing environment in Rust. You can try the headless Rust preview or run the desktop and CLI with GNU Octave.

## Try the Rust preview

The preview runs a subset of MATLAB-style scripts, functions, and classes. It supports complex arrays, sparse matrix operations, and linear least squares without MATLAB or Octave.

With Rust 1.88 or newer and a system linker:

```sh
git clone --branch codex/headless-preview-be4e5cd https://github.com/me-farza/opennumlab.git
cd opennumlab
cargo build --locked --release -j1 -p mathlab-cli --bin mathlab
./target/release/mathlab --json run --rust-program --profile class-v1 --workspace examples/rust-least-squares.m
```

[Supported features and limits](https://github.com/me-farza/opennumlab/blob/codex/headless-preview-be4e5cd/docs/release/HEADLESS_CLI.md)

## Explore the desktop

The desktop on `main` uses a separately installed GNU Octave. It includes a script editor, command window, workspace, and figure previews.

[Desktop setup and usage](https://github.com/me-farza/opennumlab/blob/86776bb3dbdb95ed2828db373cf87883d55f5dce/README.md#start-it) · [Desktop alpha notes](docs/release/V0.1.0_ALPHA.md)

## Contribute

Try an example, report a reproducible issue, or pick a task from the roadmap.

[Contributing](CONTRIBUTING.md) · [Roadmap](ROADMAP.md) · [Licensing](LICENSING.md)
