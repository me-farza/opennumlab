#!/usr/bin/env bash
# Compile/test Rust; an available runtime requires its exact-value native adapter.
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

# Produce an absolute executable path so Rust tests using another working
# directory and raw fixture commands use exactly the same selected runtime.
mathlab_resolve_executable() {
  local mathlab_resolved
  mathlab_resolved="$(command -v -- "$1")" || return 1
  [[ -f "$mathlab_resolved" && -x "$mathlab_resolved" ]] || return 1
  printf '%s/%s\n' "$(cd -- "$(dirname -- "$mathlab_resolved")" && pwd -P)" "$(basename -- "$mathlab_resolved")"
}

mathlab_octave=""
if [[ -n "${MATHLAB_OCTAVE:-${OCTAVE_EXECUTABLE:-}}" ]]; then
  if ! mathlab_octave="$(mathlab_resolve_executable "${MATHLAB_OCTAVE:-$OCTAVE_EXECUTABLE}")"; then
    printf '%s\n' 'Configured Octave executable is missing or not executable.' >&2
    exit 1
  fi
else
  for mathlab_candidate in octave-cli octave /opt/homebrew/bin/octave-cli /opt/homebrew/bin/octave /usr/local/bin/octave-cli /usr/local/bin/octave /usr/bin/octave-cli /usr/bin/octave; do
    if mathlab_octave="$(mathlab_resolve_executable "$mathlab_candidate")"; then break; fi
  done
fi

# Required test profiles cannot turn a missing runtime into successful early returns.
for mathlab_required in MATHLAB_REQUIRE_TYPED_PAGE_TESTS MATHLAB_REQUIRE_VALUE_EDIT_TESTS MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS MATHLAB_REQUIRE_GRAPHICS_TESTS MATHLAB_REQUIRE_READY_JSON_TESTS; do
  if [[ "${!mathlab_required:-}" == 1 ]]; then mathlab_require_octave=true; fi
done
if [[ -z "$mathlab_octave" ]]; then
  if "$mathlab_require_octave" || "$mathlab_include_control" || "$mathlab_include_toolboxes" || "$mathlab_include_debugger"; then
    printf '%s\n' 'GNU Octave is required but was not found.' >&2
    exit 1
  fi
else
  export MATHLAB_OCTAVE="$mathlab_octave"
  mathlab_banner="$("$mathlab_octave" --version)"
  mathlab_runtime_version="$(printf '%s\n' "$mathlab_banner" | sed -nE 's/^GNU Octave(,| \([^()[:space:]]+\))? version ([0-9]+\.[0-9]+\.[0-9]+([+~-][[:alnum:].+_~-]*)?)$/\2/p')"
  mathlab_compiler="${MATHLAB_MKOCTFILE:-}"
  if [[ -z "$mathlab_compiler" ]]; then
    mathlab_compiler="$(dirname -- "$mathlab_octave")/mkoctfile"
    if [[ ! -f "$mathlab_compiler" || ! -x "$mathlab_compiler" ]]; then mathlab_compiler=mkoctfile; fi
  fi
  if ! mathlab_compiler="$(mathlab_resolve_executable "$mathlab_compiler")"; then
    printf '%s\n' 'Matching mkoctfile and Octave development headers are required for exact-value verification. Set MATHLAB_MKOCTFILE.' >&2
    exit 1
  fi
  mathlab_sdk_version="$("$mathlab_compiler" -p OCTAVE_VERSION)"
  if [[ -z "$mathlab_runtime_version" || "$mathlab_sdk_version" != "$mathlab_runtime_version" ]]; then
    printf 'Octave runtime/SDK mismatch: runtime=%s SDK=%s (%s).\n' "$mathlab_runtime_version" "$mathlab_sdk_version" "$mathlab_compiler" >&2
    exit 1
  fi
  if [[ -n "${MATHLAB_EXPECT_OCTAVE_VERSION:-}" && "$mathlab_runtime_version" != "$MATHLAB_EXPECT_OCTAVE_VERSION" ]]; then
    printf 'Expected Octave %s, found %s.\n' "$MATHLAB_EXPECT_OCTAVE_VERSION" "$mathlab_runtime_version" >&2
    exit 1
  fi
  export MATHLAB_MKOCTFILE="$mathlab_compiler"
  printf '%s\n' "$mathlab_banner"
  # Core public Engine tests include dense-complex pages/edits. Provision their
  # matching adapter before ANY workspace tests rather than narrowing fixtures.
  if "$mathlab_include_debugger"; then
    bash scripts/build-debugger.sh
    export MATHLAB_DEBUG_BRIDGE="$mathlab_repo/native/mathlab_debug_bridge.oct"
    export MATHLAB_SCOPE_BRIDGE="$mathlab_repo/native/mathlab_scope_bridge.oct"
    export MATHLAB_INPUT_BRIDGE="$mathlab_repo/native/mathlab_input_bridge.oct"
  else
    # The default profile provisions only the value adapter. Engine startup
    # consumes scope/input paths automatically, so stale inherited paths would
    # silently mix unrelated native builds into this profile.
    unset MATHLAB_DEBUG_BRIDGE MATHLAB_SCOPE_BRIDGE MATHLAB_INPUT_BRIDGE
    bash scripts/build-value-bridge.sh
  fi
  export MATHLAB_VALUE_BRIDGE="$mathlab_repo/native/mathlab_value_bridge.oct"
  [[ -s "$MATHLAB_VALUE_BRIDGE" ]] || { printf '%s\n' 'Value adapter build did not produce a nonempty module.' >&2; exit 1; }
  export MATHLAB_REQUIRE_TYPED_PAGE_TESTS=1
  export MATHLAB_REQUIRE_VALUE_EDIT_TESTS=1
  export MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS=1
  export MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS=1
  export MATHLAB_REQUIRE_READY_JSON_TESTS=1
fi

# Native interpreter sessions are resource-heavy. Keep the default bounded;
# callers may explicitly choose a different test-thread count.
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}"
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked

if [[ -z "$mathlab_octave" ]]; then
  printf '%s\n' 'SKIP Octave fixtures/native adapter: no runtime found. Rust checks completed.'
  exit 0
fi

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
