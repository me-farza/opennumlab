#!/usr/bin/env bash
# Compile/test Rust and run original Octave assertions when a runtime exists.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

mathlab_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$mathlab_repo"

mathlab_require_octave=false
mathlab_include_control=false
mathlab_include_toolboxes=false
mathlab_include_debugger=false
for mathlab_argument in "$@"; do
  case "$mathlab_argument" in
    --require-octave) mathlab_require_octave=true ;;
    --with-control) mathlab_include_control=true ;;
    --with-toolboxes) mathlab_include_toolboxes=true ;;
    --with-debugger) mathlab_include_debugger=true ;;
    *) printf 'Unknown option: %s\n' "$mathlab_argument" >&2; exit 2 ;;
  esac
done

if "$mathlab_include_debugger"; then
  bash scripts/build-debugger.sh
  export MATHLAB_DEBUG_BRIDGE="$mathlab_repo/native/mathlab_debug_bridge.oct"
  export MATHLAB_SCOPE_BRIDGE="$mathlab_repo/native/mathlab_scope_bridge.oct"
  export MATHLAB_INPUT_BRIDGE="$mathlab_repo/native/mathlab_input_bridge.oct"
fi

cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked

if [[ -n "${MATHLAB_OCTAVE:-}" ]]; then
  mathlab_octave="$MATHLAB_OCTAVE"
elif command -v octave-cli >/dev/null 2>&1; then
  mathlab_octave="$(command -v octave-cli)"
elif command -v octave >/dev/null 2>&1; then
  mathlab_octave="$(command -v octave)"
else
  if "$mathlab_require_octave" || "$mathlab_include_control" || "$mathlab_include_toolboxes"; then
    printf '%s\n' 'GNU Octave is required but was not found.' >&2
    exit 1
  fi
  printf '%s\n' 'SKIP Octave fixtures: no runtime found. Rust checks completed.'
  exit 0
fi

"$mathlab_octave" --version
mathlab_graphics_setup="warning('off', 'Octave:gnuplot-graphics'); if any(strcmp(available_graphics_toolkits(), 'gnuplot')); graphics_toolkit('gnuplot'); mathlab_warmup=figure('visible','off','__graphics_toolkit__','gnuplot'); close(mathlab_warmup); clear mathlab_warmup; graphics_toolkit('gnuplot'); end;"
for mathlab_fixture in language_core core_types_functions linear_algebra fft_core mat_roundtrip graphics_core; do
  "$mathlab_octave" --no-gui --no-init-file --quiet --eval "${mathlab_graphics_setup} run('tests/fixtures/${mathlab_fixture}.m');"
done

# Exercise the Rust adapter as well as the raw-runtime fixtures.
cargo run --locked -p mathlab-cli -- doctor
cargo run --locked -p mathlab-cli -- check compatibility/core.json
if "$mathlab_include_control"; then
  "$mathlab_octave" --no-gui --no-init-file --quiet --eval "${mathlab_graphics_setup} run('tests/fixtures/control_optional.m');"
  cargo run --locked -p mathlab-cli -- check compatibility/control.json
fi
if "$mathlab_include_toolboxes"; then
  "$mathlab_octave" --no-gui --no-init-file --quiet --eval "${mathlab_graphics_setup} run('tests/fixtures/toolboxes_optional.m');"
  cargo run --locked -p mathlab-cli -- check compatibility/toolboxes.json
fi
printf '%s\n' 'Verification completed successfully.'
