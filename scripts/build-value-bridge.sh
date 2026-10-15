#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the exact-value adapter for the selected matching Octave runtime.
set -euo pipefail
mathlab_value_repo="$(cd "$(dirname "$0")/.." && pwd)"
mathlab_value_resolve_executable() {
  local mathlab_value_resolved
  mathlab_value_resolved="$(command -v -- "$1")" || return 1
  [[ -f "$mathlab_value_resolved" && -x "$mathlab_value_resolved" ]] || return 1
  printf '%s/%s\n' "$(cd -- "$(dirname -- "$mathlab_value_resolved")" && pwd -P)" "$(basename -- "$mathlab_value_resolved")"
}
mathlab_value_octave="${MATHLAB_OCTAVE:-${OCTAVE_EXECUTABLE:-}}"
if [[ -z "$mathlab_value_octave" ]]; then
  mathlab_value_octave="$(command -v octave-cli || command -v octave || true)"
fi
if [[ -z "$mathlab_value_octave" ]] || ! mathlab_value_octave="$(mathlab_value_resolve_executable "$mathlab_value_octave")"; then
  printf '%s\n' 'An executable GNU Octave runtime is required. Set MATHLAB_OCTAVE.' >&2
  exit 1
fi
mathlab_value_compiler="${MATHLAB_MKOCTFILE:-}"
if [[ -z "$mathlab_value_compiler" ]]; then
  mathlab_value_compiler="$(dirname -- "$mathlab_value_octave")/mkoctfile"
  if [[ ! -f "$mathlab_value_compiler" || ! -x "$mathlab_value_compiler" ]]; then mathlab_value_compiler=mkoctfile; fi
fi
if ! mathlab_value_compiler="$(mathlab_value_resolve_executable "$mathlab_value_compiler")"; then
  printf '%s\n' 'Matching mkoctfile and Octave development headers are required. Set MATHLAB_MKOCTFILE.' >&2
  exit 1
fi
mathlab_value_runtime_version="$("$mathlab_value_octave" --version | sed -nE 's/^GNU Octave(,| \([^()[:space:]]+\))? version ([0-9]+\.[0-9]+\.[0-9]+([+~-][[:alnum:].+_~-]*)?)$/\2/p')"
mathlab_value_sdk_version="$("$mathlab_value_compiler" -p OCTAVE_VERSION)"
if [[ -z "$mathlab_value_runtime_version" || "$mathlab_value_sdk_version" != "$mathlab_value_runtime_version" ]]; then
  printf 'Octave runtime/SDK mismatch: runtime=%s SDK=%s (%s).\n' "$mathlab_value_runtime_version" "$mathlab_value_sdk_version" "$mathlab_value_compiler" >&2
  exit 1
fi
if [[ -n "${MATHLAB_EXPECT_OCTAVE_VERSION:-}" && "$mathlab_value_runtime_version" != "$MATHLAB_EXPECT_OCTAVE_VERSION" ]]; then
  printf 'Expected Octave %s, found %s.\n' "$MATHLAB_EXPECT_OCTAVE_VERSION" "$mathlab_value_runtime_version" >&2
  exit 1
fi

mathlab_value_build_dir="$(mktemp -d "$mathlab_value_repo/native/.value-build-XXXXXX")"
trap 'rm -rf -- "$mathlab_value_build_dir"' EXIT
"$mathlab_value_compiler" "$mathlab_value_repo/native/mathlab_value_bridge.cc" \
  -o "$mathlab_value_build_dir/mathlab_value_bridge.oct"
printf '%s\n' "$mathlab_value_runtime_version" > "$mathlab_value_build_dir/value-runtime-version.txt"

# Existing sessions can retain the old library inode through a captured handle.
# Replace the finished module, never truncate a module mapped by an interpreter.
mv -f -- "$mathlab_value_build_dir/mathlab_value_bridge.oct" \
  "$mathlab_value_repo/native/mathlab_value_bridge.oct"
printf 'Built %s\n' "$mathlab_value_repo/native/mathlab_value_bridge.oct"
printf 'Octave version: %s\n' "$(< "$mathlab_value_build_dir/value-runtime-version.txt")"
