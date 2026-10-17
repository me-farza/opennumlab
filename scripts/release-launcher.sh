#!/bin/bash
# Installed entry point. Requires only system shell and file utilities.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
# The producer changes this literal only for an explicitly qualified package.
mathlab_packaged_startup=false
mathlab_contents="$(cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
mathlab_project="${MATHLAB_PROJECT_DIR:-${HOME:?HOME must name a writable user directory}/Library/Application Support/OpenNumLab/Projects/Getting Started}"
mathlab_custom_directory=false
mathlab_has_source=false
mathlab_expect=""
mathlab_state=""
mathlab_literal_startup=0
mathlab_packaged_argument=0
mathlab_runtime_override=false
mathlab_positional_only=false
mathlab_arguments=()
for mathlab_argument in "$@"; do
  if [[ "$mathlab_expect" == cwd ]]; then
    mathlab_project="$mathlab_argument"
    mathlab_expect=""
    continue
  fi
  mathlab_arguments+=("$mathlab_argument")
  if [[ -n "$mathlab_expect" ]]; then
    if [[ "$mathlab_expect" == state ]]; then mathlab_state="$mathlab_argument"; fi
    mathlab_expect=""
    continue
  fi
  if "$mathlab_positional_only"; then
    mathlab_has_source=true
    continue
  fi
  case "$mathlab_argument" in
    --) mathlab_positional_only=true ;;
    --cwd) mathlab_custom_directory=true; mathlab_expect=cwd; unset 'mathlab_arguments[${#mathlab_arguments[@]}-1]' ;;
    --cwd=*) mathlab_custom_directory=true; mathlab_project="${mathlab_argument#--cwd=}"; unset 'mathlab_arguments[${#mathlab_arguments[@]}-1]' ;;
    --open) mathlab_has_source=true; mathlab_expect=value ;;
    --open=*) mathlab_has_source=true ;;
    --state) mathlab_expect=state ;;
    --state=*) mathlab_state="${mathlab_argument#--state=}" ;;
    --startup-config) mathlab_literal_startup=$((mathlab_literal_startup + 1)); mathlab_expect=value ;;
    --startup-config=*) mathlab_literal_startup=$((mathlab_literal_startup + 1)) ;;
    --packaged-startup) mathlab_packaged_argument=$((mathlab_packaged_argument + 1)) ;;
    --octave|--debugger) mathlab_runtime_override=true; mathlab_expect=value ;;
    --octave=*|--debugger=*) mathlab_runtime_override=true ;;
    --inspect|--screenshot|--break-at|--error-target) mathlab_expect=value ;;
    --*) ;;
    *) mathlab_has_source=true ;;
  esac
done
if [[ "$mathlab_expect" == cwd ]]; then
  printf '%s\n' '--cwd requires a project directory' >&2
  exit 2
fi
if "$mathlab_packaged_startup" || [[ "$mathlab_packaged_argument" -gt 0 ]]; then
  if [[ "$mathlab_packaged_argument" -gt 1 || "$mathlab_literal_startup" -gt 1 ]] \
      || [[ "$mathlab_packaged_argument" -gt 0 && "$mathlab_literal_startup" -gt 0 ]] \
      || "$mathlab_runtime_override"; then
    printf '%s\n' 'Qualified startup requires one selector and cannot use --octave or --debugger overrides' >&2
    exit 2
  fi
  # Qualified selection never inherits legacy runtime or adapter choices.
  unset MATHLAB_OCTAVE OCTAVE_EXECUTABLE MATHLAB_DEBUG_BRIDGE MATHLAB_SCOPE_BRIDGE \
    MATHLAB_INPUT_BRIDGE MATHLAB_VALUE_BRIDGE
fi
if ! "$mathlab_custom_directory"; then
  (umask 077; mkdir -p "$mathlab_project")
  # Copy once, preserving edited examples on later launches and upgrades.
  for mathlab_example in "$mathlab_contents"/Resources/examples/*.m; do
    [[ -f "$mathlab_example" ]] || continue
    mathlab_copy="$mathlab_project/$(basename -- "$mathlab_example")"
    if [[ ! -e "$mathlab_copy" && ! -L "$mathlab_copy" ]]; then
      (umask 077; set -o noclobber; cat "$mathlab_example" > "$mathlab_copy") 2>/dev/null || [[ -e "$mathlab_copy" ]]
    fi
  done
fi
# Resolve relative --cwd once, then give the desktop the absolute path.
cd -- "$mathlab_project"
mathlab_defaults=(--cwd "$PWD")
if "$mathlab_packaged_startup" && [[ "$mathlab_literal_startup" -eq 0 && "$mathlab_packaged_argument" -eq 0 ]]; then
  mathlab_defaults+=(--packaged-startup)
fi
mathlab_state="${mathlab_state:-.mathlab/session.json}"
if ! "$mathlab_custom_directory" && ! "$mathlab_has_source" && [[ ! -e "$mathlab_state" ]]; then
  mathlab_defaults+=(--open "$PWD/getting_started.m")
fi
if ! "$mathlab_packaged_startup" && [[ "$mathlab_packaged_argument" -eq 0 ]]; then
  for mathlab_adapter in DEBUG SCOPE INPUT VALUE; do
    case "$mathlab_adapter" in
      DEBUG) mathlab_module=mathlab_debug_bridge ;;
      SCOPE) mathlab_module=mathlab_scope_bridge ;;
      INPUT) mathlab_module=mathlab_input_bridge ;;
      VALUE) mathlab_module=mathlab_value_bridge ;;
    esac
    mathlab_variable="MATHLAB_${mathlab_adapter}_BRIDGE"
    if [[ -f "$mathlab_contents/Resources/native/$mathlab_module.oct" && -z "${!mathlab_variable:-}" ]]; then
      export "$mathlab_variable=$mathlab_contents/Resources/native/$mathlab_module.oct"
    fi
  done
fi
# macOS Bash 3.2 considers an empty array unset under nounset.
if [[ ${#mathlab_arguments[@]} -gt 0 ]]; then
  exec "$mathlab_contents/MacOS/mathlab-desktop" "${mathlab_defaults[@]}" "${mathlab_arguments[@]}"
fi
exec "$mathlab_contents/MacOS/mathlab-desktop" "${mathlab_defaults[@]}"
