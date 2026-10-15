#!/usr/bin/env bash
# Print local setup guidance. This script does not install or change anything.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

cat <<'TEXT'
Open MathLab development setup

Required: a stable Rust toolchain and GNU Octave on PATH.
The Rust desktop and CLI start a local Octave process. Octave is installed
separately during this alpha; no interpreter is bundled in this repository.

Rust installation documentation: https://rustup.rs/
Octave downloads: https://octave.org/download
TEXT

case "$(uname -s)" in
  Darwin)
    cat <<'TEXT'

macOS with an existing Homebrew installation:
  brew install octave gnuplot

Formula details: https://formulae.brew.sh/formula/octave
TEXT
    ;;
  Linux)
    cat <<'TEXT'

Ubuntu/Debian example (run these commands yourself):
  sudo apt-get update
  sudo apt-get install octave gnuplot-nox fonts-freefont-otf \
    build-essential pkg-config libx11-dev libxi-dev libxcursor-dev \
    libxrandr-dev libxinerama-dev libgl1-mesa-dev libwayland-dev libxkbcommon-dev

Optional packaged toolboxes, when available in your distribution:
  sudo apt-get install octave-control octave-signal octave-image octave-statistics octave-optim octave-struct

Other Linux distributions: use the Octave package from your distribution.
TEXT
    ;;
  *)
    cat <<'TEXT'

Other platforms: install Rust and a GNU Octave command-line executable using
the official platform guidance. This alpha's desktop packaging has not been
validated on every platform.
TEXT
    ;;
esac

cat <<'TEXT'

Optional scientific package profile from the Octave command prompt:
  pkg install -forge struct datatypes
  pkg install -forge control
  pkg install -forge signal image statistics optim
  pkg list

Some packages compile native extensions and require compiler/build tools.
Packages remain separate upstream projects. Check the exact installed versions
with pkg list; listing a package does not establish MATLAB toolbox parity.
The control example loads control itself. Other packages can be loaded for a
session with pkg load NAME. Avoid loading every package by default because
package functions can shadow core functions.

From this repository:
  cargo run -p mathlab-cli -- doctor
  cargo run -p mathlab-cli -- check compatibility/core.json
  cargo run -p mathlab-desktop
  ./scripts/verify.sh
TEXT

if command -v cargo >/dev/null 2>&1; then
  cargo --version
else
  printf '%s\n' 'Rust cargo: not found on PATH'
fi
if command -v octave-cli >/dev/null 2>&1; then
  octave-cli --version | sed -n '1p'
elif command -v octave >/dev/null 2>&1; then
  octave --version | sed -n '1p'
else
  printf '%s\n' 'GNU Octave: not found on PATH'
fi
