#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build four adapters by default, or the explicitly selected qualified core.
set -euo pipefail
native_usage() {
  printf '%s\n' 'Usage: build-debugger.sh [--profile qualified-core-v2]' >&2
  exit 2
}
native_profile=four-adapter-value-v1
if [[ "$#" -ne 0 ]]; then
  [[ "$#" -eq 2 ]] || native_usage
  [[ "$1" == --profile && "$2" == qualified-core-v2 ]] || native_usage
  native_profile=qualified-core-v2
fi
project_root="$(cd "$(dirname "$0")/.." && pwd)"
native_resolve_executable() {
  local native_resolved
  native_resolved="$(command -v -- "$1")" || return 1
  [[ -f "$native_resolved" && -x "$native_resolved" ]] || return 1
  printf '%s/%s\n' "$(cd -- "$(dirname -- "$native_resolved")" && pwd -P)" "$(basename -- "$native_resolved")"
}
native_octave="${MATHLAB_OCTAVE:-${OCTAVE_EXECUTABLE:-}}"
if [[ -z "$native_octave" ]]; then
  native_octave="$(command -v octave-cli || command -v octave || true)"
fi
if [[ -z "$native_octave" ]] || ! native_octave="$(native_resolve_executable "$native_octave")"; then
  printf '%s\n' 'An executable GNU Octave runtime is required. Set MATHLAB_OCTAVE.' >&2
  exit 1
fi
mkoctfile_bin="${MATHLAB_MKOCTFILE:-}"
if [[ -z "$mkoctfile_bin" ]]; then
  mkoctfile_bin="$(dirname -- "$native_octave")/mkoctfile"
  if [[ ! -f "$mkoctfile_bin" || ! -x "$mkoctfile_bin" ]]; then mkoctfile_bin=mkoctfile; fi
fi
if ! mkoctfile_bin="$(native_resolve_executable "$mkoctfile_bin")"; then
  printf '%s\n' 'Matching mkoctfile and Octave development headers are required. Set MATHLAB_MKOCTFILE.' >&2
  exit 1
fi
native_runtime_version="$("$native_octave" --version | sed -nE 's/^GNU Octave(,| \([^()[:space:]]+\))? version ([0-9]+\.[0-9]+\.[0-9]+([+~-][[:alnum:].+_~-]*)?)$/\2/p')"
native_sdk_version="$("$mkoctfile_bin" -p OCTAVE_VERSION)"
if [[ -z "$native_runtime_version" || "$native_sdk_version" != "$native_runtime_version" ]]; then
  printf 'Octave runtime/SDK mismatch: runtime=%s SDK=%s (%s).\n' "$native_runtime_version" "$native_sdk_version" "$mkoctfile_bin" >&2
  exit 1
fi
if [[ -n "${MATHLAB_EXPECT_OCTAVE_VERSION:-}" && "$native_runtime_version" != "$MATHLAB_EXPECT_OCTAVE_VERSION" ]]; then
  printf 'Expected Octave %s, found %s.\n' "$MATHLAB_EXPECT_OCTAVE_VERSION" "$native_runtime_version" >&2
  exit 1
fi
if [[ "$native_profile" == qualified-core-v2 ]]; then
  if [[ "$native_runtime_version" != 11.3.0 ]]; then
    printf 'Profile qualified-core-v2 requires Octave 11.3.0, found %s.\n' "$native_runtime_version" >&2
    exit 1
  fi
fi
native_compile_module() {
  if [[ "$native_profile" == qualified-core-v2 ]]; then
    # Match the accepted core build; V2 headers require api-v61 as well.
    "$mkoctfile_bin" -DMATHLAB_QUALIFIED_CORE_V2=1 -Wall -Wextra -Werror -std=c++17 "-I$project_root/native" "$@"
  else
    "$mkoctfile_bin" "$@"
  fi
}
native_build_dir="$(mktemp -d "$project_root/native/.build-XXXXXX")"
trap 'rm -rf -- "$native_build_dir"' EXIT
native_compile_module "$project_root/native/mathlab_debug_bridge.cc" -o "$native_build_dir/mathlab_debug_bridge.oct"
native_compile_module "$project_root/native/mathlab_scope_bridge.cc" -o "$native_build_dir/mathlab_scope_bridge.oct"
native_compile_module "$project_root/native/mathlab_input_bridge.cc" -o "$native_build_dir/mathlab_input_bridge.oct"
native_compile_module "$project_root/native/mathlab_value_bridge.cc" -o "$native_build_dir/mathlab_value_bridge.oct"
if [[ "$native_profile" == qualified-core-v2 ]]; then
  native_compile_module "$project_root/native/mathlab_intrinsic_bridge_v2.cc" -o "$native_build_dir/mathlab_intrinsic_bridge.oct"
fi
printf '%s\n' "$native_runtime_version" > "$native_build_dir/runtime-version.txt"
# A loaded module may still be memory-mapped by an existing interpreter. Build
# separately, then replace its inode instead of truncating that live module.
mv -f -- "$native_build_dir/mathlab_debug_bridge.oct" "$project_root/native/mathlab_debug_bridge.oct"
mv -f -- "$native_build_dir/mathlab_scope_bridge.oct" "$project_root/native/mathlab_scope_bridge.oct"
mv -f -- "$native_build_dir/mathlab_input_bridge.oct" "$project_root/native/mathlab_input_bridge.oct"
mv -f -- "$native_build_dir/mathlab_value_bridge.oct" "$project_root/native/mathlab_value_bridge.oct"
if [[ "$native_profile" == qualified-core-v2 ]]; then
  mv -f -- "$native_build_dir/mathlab_intrinsic_bridge.oct" "$project_root/native/mathlab_intrinsic_bridge.oct"
fi
mv -f -- "$native_build_dir/runtime-version.txt" "$project_root/native/runtime-version.txt"
printf 'Built %s\n' "$project_root/native/mathlab_debug_bridge.oct"
printf 'Built %s\n' "$project_root/native/mathlab_scope_bridge.oct"
printf 'Built %s\n' "$project_root/native/mathlab_input_bridge.oct"
printf 'Built %s\n' "$project_root/native/mathlab_value_bridge.oct"
if [[ "$native_profile" == qualified-core-v2 ]]; then
  printf 'Built %s\n' "$project_root/native/mathlab_intrinsic_bridge.oct"
fi
