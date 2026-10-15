#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
mkoctfile_bin="${MATHLAB_MKOCTFILE:-mkoctfile}"
if [[ -z "${MATHLAB_MKOCTFILE:-}" && -n "${MATHLAB_OCTAVE:-}" ]]; then
  selected_mkoctfile="$(dirname -- "$MATHLAB_OCTAVE")/mkoctfile"
  if [[ -x "$selected_mkoctfile" ]]; then
    mkoctfile_bin="$selected_mkoctfile"
  fi
fi
native_build_dir="$(mktemp -d "$project_root/native/.build-XXXXXX")"
trap 'rm -rf -- "$native_build_dir"' EXIT
"$mkoctfile_bin" "$project_root/native/mathlab_debug_bridge.cc" -o "$native_build_dir/mathlab_debug_bridge.oct"
"$mkoctfile_bin" "$project_root/native/mathlab_scope_bridge.cc" -o "$native_build_dir/mathlab_scope_bridge.oct"
"$mkoctfile_bin" "$project_root/native/mathlab_input_bridge.cc" -o "$native_build_dir/mathlab_input_bridge.oct"
"$mkoctfile_bin" -p OCTAVE_VERSION > "$native_build_dir/runtime-version.txt"
# A loaded module may still be memory-mapped by an existing interpreter. Build
# separately, then replace its inode instead of truncating that live module.
mv -f -- "$native_build_dir/mathlab_debug_bridge.oct" "$project_root/native/mathlab_debug_bridge.oct"
mv -f -- "$native_build_dir/mathlab_scope_bridge.oct" "$project_root/native/mathlab_scope_bridge.oct"
mv -f -- "$native_build_dir/mathlab_input_bridge.oct" "$project_root/native/mathlab_input_bridge.oct"
mv -f -- "$native_build_dir/runtime-version.txt" "$project_root/native/runtime-version.txt"
printf 'Built %s\n' "$project_root/native/mathlab_debug_bridge.oct"
printf 'Built %s\n' "$project_root/native/mathlab_scope_bridge.oct"
printf 'Built %s\n' "$project_root/native/mathlab_input_bridge.oct"
