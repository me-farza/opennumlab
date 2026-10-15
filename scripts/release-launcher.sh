#!/bin/bash
# Installed entry point. Requires only system shell and file utilities.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
mathlab_contents="$(cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
mathlab_project="${MATHLAB_PROJECT_DIR:-${HOME:?HOME must name a writable user directory}/Library/Application Support/OpenNumLab/Projects/Getting Started}"
mathlab_custom_directory=false
mathlab_has_source=false
mathlab_expect=""
mathlab_state=""
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
  case "$mathlab_argument" in
    --cwd) mathlab_custom_directory=true; mathlab_expect=cwd; unset 'mathlab_arguments[${#mathlab_arguments[@]}-1]' ;;
    --cwd=*) mathlab_custom_directory=true; mathlab_project="${mathlab_argument#--cwd=}"; unset 'mathlab_arguments[${#mathlab_arguments[@]}-1]' ;;
    --open) mathlab_has_source=true; mathlab_expect=value ;;
    --open=*) mathlab_has_source=true ;;
    --state) mathlab_expect=state ;;
    --state=*) mathlab_state="${mathlab_argument#--state=}" ;;
    --octave|--inspect|--screenshot|--debugger|--break-at|--error-target) mathlab_expect=value ;;
    --*) ;;
    *) mathlab_has_source=true ;;
  esac
done
if [[ "$mathlab_expect" == cwd ]]; then
  printf '%s\n' '--cwd requires a project directory' >&2
  exit 2
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
mathlab_state="${mathlab_state:-.mathlab/session.json}"
if ! "$mathlab_custom_directory" && ! "$mathlab_has_source" && [[ ! -e "$mathlab_state" ]]; then
  mathlab_defaults+=(--open "$PWD/getting_started.m")
fi
for mathlab_adapter in DEBUG SCOPE INPUT; do
  case "$mathlab_adapter" in
    DEBUG) mathlab_module=mathlab_debug_bridge ;;
    SCOPE) mathlab_module=mathlab_scope_bridge ;;
    INPUT) mathlab_module=mathlab_input_bridge ;;
  esac
  mathlab_variable="MATHLAB_${mathlab_adapter}_BRIDGE"
  if [[ -f "$mathlab_contents/Resources/native/$mathlab_module.oct" && -z "${!mathlab_variable:-}" ]]; then
    export "$mathlab_variable=$mathlab_contents/Resources/native/$mathlab_module.oct"
  fi
done
# macOS Bash 3.2 considers an empty array unset under nounset.
if [[ ${#mathlab_arguments[@]} -gt 0 ]]; then
  exec "$mathlab_contents/MacOS/mathlab-desktop" "${mathlab_defaults[@]}" "${mathlab_arguments[@]}"
fi
exec "$mathlab_contents/MacOS/mathlab-desktop" "${mathlab_defaults[@]}"
