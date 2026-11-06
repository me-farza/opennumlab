#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Synthetic launcher contracts only: the desktop is a shell stub; no runtime,
# native module, Cargo, installed application or user project is executed.
set -euo pipefail

test_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
test_tmp="$(mktemp -d "${TMPDIR:-/tmp}/mathlab launcher-value.XXXXXX")"
# macOS TMPDIR commonly ends in '/'; mktemp may retain the resulting '//'.
# The launcher deliberately resolves cwd before forwarding it. Canonicalize the
# owned fixture root too, so default-project expectations use that same spelling.
test_tmp="$(cd -- "$test_tmp" && pwd -P)"
cleanup() {
  # Only this harness's new tree is writable/removed, including read-only bundles.
  chmod -R u+w "$test_tmp" 2>/dev/null || true
  rm -rf -- "$test_tmp"
}
trap cleanup EXIT

mkdir -p "$test_tmp/project with spaces/工作区" "$test_tmp/launch directory" \
  "$test_tmp/relocated 名称" "$test_tmp/overrides"
test_project="$(cd -- "$test_tmp/project with spaces/工作区" && pwd -P)"
test_launch_directory="$(cd -- "$test_tmp/launch directory" && pwd -P)"
test_override="$test_tmp/overrides/explicit value Ω.oct"
test_missing_override='../missing adapter Ω.oct'
printf '%s\n' 'synthetic explicit override; never load' > "$test_override"
[[ ! -e "$test_project/$test_missing_override" ]]

make_bundle() {
  local test_bundle="$1" test_with_value="$2" test_module test_profile="${3:-legacy}"
  mkdir -p "$test_bundle/Contents/MacOS" "$test_bundle/Contents/Resources/native" \
    "$test_bundle/Contents/Resources/examples"
  cp "$test_repo/scripts/release-launcher.sh" "$test_bundle/Contents/MacOS/mathlab-launcher"
  if [[ "$test_profile" == qualified ]]; then
    [[ "$(grep -c '^mathlab_packaged_startup=false$' "$test_bundle/Contents/MacOS/mathlab-launcher")" == 1 ]]
    sed 's/^mathlab_packaged_startup=false$/mathlab_packaged_startup=true/' \
      "$test_bundle/Contents/MacOS/mathlab-launcher" > "$test_bundle/Contents/MacOS/stamped-launcher"
    mv "$test_bundle/Contents/MacOS/stamped-launcher" "$test_bundle/Contents/MacOS/mathlab-launcher"
    printf '%s\n' 'synthetic intrinsic provider; never load' \
      > "$test_bundle/Contents/Resources/native/mathlab_intrinsic_bridge.oct"
  fi
  for test_module in debug scope input; do
    printf '%s\n' "synthetic $test_module module; never load" \
      > "$test_bundle/Contents/Resources/native/mathlab_${test_module}_bridge.oct"
  done
  if [[ "$test_with_value" == yes ]]; then
    printf '%s\n' 'synthetic value module; never load' \
      > "$test_bundle/Contents/Resources/native/mathlab_value_bridge.oct"
  fi
  printf '%s\n' '% This must not be copied into a default project.' \
    > "$test_bundle/Contents/Resources/examples/getting_started.m"
  cat > "$test_bundle/Contents/MacOS/mathlab-desktop" <<'STUB'
#!/bin/bash
set -euo pipefail
: "${MATHLAB_LAUNCHER_VALUE_CAPTURE:?the synthetic capture directory is required}"
printf '%s\0' "$PWD" "$@" > "$MATHLAB_LAUNCHER_VALUE_CAPTURE/arguments"
for test_adapter in DEBUG SCOPE INPUT VALUE; do
  test_variable="MATHLAB_${test_adapter}_BRIDGE"
  if declare -p "$test_variable" >/dev/null 2>&1; then
    printf '%s' "${!test_variable}" > "$MATHLAB_LAUNCHER_VALUE_CAPTURE/$test_adapter.value"
  fi
done
for test_variable in MATHLAB_OCTAVE OCTAVE_EXECUTABLE; do
  if declare -p "$test_variable" >/dev/null 2>&1; then
    printf '%s' "${!test_variable}" > "$MATHLAB_LAUNCHER_VALUE_CAPTURE/$test_variable.value"
  fi
done
STUB
  chmod 555 "$test_bundle/Contents/MacOS/mathlab-launcher" \
    "$test_bundle/Contents/MacOS/mathlab-desktop"
  chmod -R a-w "$test_bundle/Contents/Resources"
}

# Relocate prepared bundles before launching them. All adapter expectations
# must name the relocated absolute Contents directory, including spaces/Unicode.
make_bundle "$test_tmp/staging complete.app" yes
make_bundle "$test_tmp/staging omitted.app" no
make_bundle "$test_tmp/staging qualified.app" yes qualified
mv "$test_tmp/staging complete.app" "$test_tmp/relocated 名称/Complete 値.app"
mv "$test_tmp/staging omitted.app" "$test_tmp/relocated 名称/Omitted 値.app"
mv "$test_tmp/staging qualified.app" "$test_tmp/relocated 名称/Qualified 値.app"
test_complete="$(cd -- "$test_tmp/relocated 名称/Complete 値.app" && pwd -P)"
test_omitted="$(cd -- "$test_tmp/relocated 名称/Omitted 値.app" && pwd -P)"
test_qualified="$(cd -- "$test_tmp/relocated 名称/Qualified 値.app" && pwd -P)"
# Intent is carried by the launcher stamp even when the packaged config is absent.
# The fake Desktop captures selection; real config parsing belongs to Desktop.
[[ ! -e "$test_qualified/Contents/Resources/qualified-core-v2.json" ]]

snapshot_bundle() {
  # Record paths, contents and long metadata outside the bundle. This catches
  # additions and permission changes as well as writes to existing resources.
  (
    cd -- "$1"
    find . -print | LC_ALL=C sort
    find . -type f -exec cksum {} + | LC_ALL=C sort
    find . -exec ls -ldn {} + | LC_ALL=C sort
  ) > "$2"
}
snapshot_bundle "$test_complete" "$test_tmp/complete.before"
snapshot_bundle "$test_omitted" "$test_tmp/omitted.before"
snapshot_bundle "$test_qualified" "$test_tmp/qualified.before"

assert_bundles_unchanged() {
  snapshot_bundle "$test_complete" "$test_tmp/complete.after"
  snapshot_bundle "$test_omitted" "$test_tmp/omitted.after"
  snapshot_bundle "$test_qualified" "$test_tmp/qualified.after"
  cmp "$test_tmp/complete.before" "$test_tmp/complete.after"
  cmp "$test_tmp/omitted.before" "$test_tmp/omitted.after"
  cmp "$test_tmp/qualified.before" "$test_tmp/qualified.after"
  [[ ! -e "$test_tmp/unused default project" ]]
}

launch_case() {
  local test_name="$1" test_bundle="$2" test_value_mode="$3" test_other_mode="$4"
  test_capture="$test_tmp/capture $test_name"
  mkdir "$test_capture"
  (
    unset MATHLAB_DEBUG_BRIDGE MATHLAB_SCOPE_BRIDGE MATHLAB_INPUT_BRIDGE MATHLAB_VALUE_BRIDGE
    export MATHLAB_OCTAVE='../explicit runtime Ω/octave-cli'
    export OCTAVE_EXECUTABLE='/missing legacy fallback Ω/octave-cli'
    export MATHLAB_LAUNCHER_VALUE_CAPTURE="$test_capture"
    # Keep even an accidental default-directory fallback inside our owned tree;
    # --cwd must win. HOME is never overridden or repurposed by this harness.
    export MATHLAB_PROJECT_DIR="$test_tmp/unused default project"
    case "$test_value_mode" in
      unset) ;;
      existing) export MATHLAB_VALUE_BRIDGE="$test_override" ;;
      missing) export MATHLAB_VALUE_BRIDGE="$test_missing_override" ;;
      empty) export MATHLAB_VALUE_BRIDGE='' ;;
      *) printf 'Unknown synthetic value mode: %s\n' "$test_value_mode" >&2; exit 2 ;;
    esac
    if [[ "$test_other_mode" == overrides ]]; then
      export MATHLAB_DEBUG_BRIDGE='../explicit debug Ω.oct'
      export MATHLAB_SCOPE_BRIDGE='/missing explicit scope Ω.oct'
      export MATHLAB_INPUT_BRIDGE='../explicit input Ω.oct'
    fi
    cd -- "$test_launch_directory"
    "$test_bundle/Contents/MacOS/mathlab-launcher" --cwd "$test_project" --open 'chosen source 雪.m'
  )
  printf '%s\0' "$test_project" --cwd "$test_project" --open 'chosen source 雪.m' \
    > "$test_tmp/expected arguments"
  cmp "$test_tmp/expected arguments" "$test_capture/arguments"
  assert_adapter MATHLAB_OCTAVE '../explicit runtime Ω/octave-cli'
  assert_adapter OCTAVE_EXECUTABLE '/missing legacy fallback Ω/octave-cli'
  assert_bundles_unchanged
}

assert_adapter() {
  local test_adapter="$1" test_expected="$2"
  [[ -f "$test_capture/$test_adapter.value" ]] || {
    printf 'Expected exported %s adapter in %s\n' "$test_adapter" "$test_capture" >&2
    exit 1
  }
  printf '%s' "$test_expected" > "$test_tmp/expected adapter"
  cmp "$test_tmp/expected adapter" "$test_capture/$test_adapter.value"
}

assert_other_bundled_adapters() {
  local test_bundle="$1"
  assert_adapter DEBUG "$test_bundle/Contents/Resources/native/mathlab_debug_bridge.oct"
  assert_adapter SCOPE "$test_bundle/Contents/Resources/native/mathlab_scope_bridge.oct"
  assert_adapter INPUT "$test_bundle/Contents/Resources/native/mathlab_input_bridge.oct"
}

launch_case bundled "$test_complete" unset bundled
assert_adapter VALUE "$test_complete/Contents/Resources/native/mathlab_value_bridge.oct"
assert_other_bundled_adapters "$test_complete"

launch_case explicit-existing "$test_complete" existing bundled
assert_adapter VALUE "$test_override"
assert_other_bundled_adapters "$test_complete"

launch_case explicit-missing "$test_complete" missing bundled
assert_adapter VALUE "$test_missing_override"
assert_other_bundled_adapters "$test_complete"

launch_case explicit-empty "$test_complete" empty bundled
assert_adapter VALUE "$test_complete/Contents/Resources/native/mathlab_value_bridge.oct"
assert_other_bundled_adapters "$test_complete"

launch_case omitted "$test_omitted" unset bundled
[[ ! -e "$test_capture/VALUE.value" ]]
assert_other_bundled_adapters "$test_omitted"

launch_case omitted-empty "$test_omitted" empty bundled
assert_adapter VALUE ''
assert_other_bundled_adapters "$test_omitted"

launch_case inherited-others "$test_complete" unset overrides
assert_adapter VALUE "$test_complete/Contents/Resources/native/mathlab_value_bridge.oct"
assert_adapter DEBUG '../explicit debug Ω.oct'
assert_adapter SCOPE '/missing explicit scope Ω.oct'
assert_adapter INPUT '../explicit input Ω.oct'

startup_case() {
  local test_name="$1" test_bundle="$2" test_expected_packaged="$3" test_directory_mode="$4" test_environment_mode="$5"
  shift 5
  local test_argument test_packaged_count=0 test_selected_project="$test_project" test_options_ended=false
  test_capture="$test_tmp/capture $test_name"
  mkdir "$test_capture"
  if [[ "$test_directory_mode" == default ]]; then
    test_selected_project="$test_tmp/default project $test_name"
  fi
  (
    export MATHLAB_LAUNCHER_VALUE_CAPTURE="$test_capture"
    export MATHLAB_PROJECT_DIR="$test_selected_project"
    export MATHLAB_OCTAVE='../inherited runtime Ω/octave-cli'
    export OCTAVE_EXECUTABLE='/inherited fallback Ω/octave-cli'
    export MATHLAB_DEBUG_BRIDGE='../inherited debug Ω.oct'
    export MATHLAB_SCOPE_BRIDGE='../inherited scope Ω.oct'
    export MATHLAB_INPUT_BRIDGE='../inherited input Ω.oct'
    export MATHLAB_VALUE_BRIDGE='../inherited value Ω.oct'
    cd -- "$test_launch_directory"
    case "$test_directory_mode" in
      explicit) "$test_bundle/Contents/MacOS/mathlab-launcher" --cwd "$test_project" --open 'chosen source 雪.m' "$@" ;;
      before-cwd) "$test_bundle/Contents/MacOS/mathlab-launcher" "$@" --cwd "$test_project" --open 'chosen source 雪.m' ;;
      default) "$test_bundle/Contents/MacOS/mathlab-launcher" "$@" ;;
      *) exit 2 ;;
    esac
  )
  printf '%s\0' "$test_selected_project" --cwd "$test_selected_project" > "$test_tmp/expected startup arguments"
  if [[ "$test_directory_mode" == default ]]; then
    printf '%s\0' --open "$test_selected_project/getting_started.m" >> "$test_tmp/expected startup arguments"
  else
    printf '%s\0' --open 'chosen source 雪.m' >> "$test_tmp/expected startup arguments"
  fi
  for test_argument in "$@"; do
    if [[ "$test_argument" != --packaged-startup ]]; then
      printf '%s\0' "$test_argument" >> "$test_tmp/expected startup arguments"
    fi
  done
  : > "$test_tmp/normalized startup arguments"
  while IFS= read -r -d '' test_argument; do
    if [[ "$test_argument" == --packaged-startup ]]; then
      [[ "$test_options_ended" == false ]]
      test_packaged_count=$((test_packaged_count + 1))
    else
      if [[ "$test_argument" == -- ]]; then test_options_ended=true; fi
      printf '%s\0' "$test_argument" >> "$test_tmp/normalized startup arguments"
    fi
  done < "$test_capture/arguments"
  [[ "$test_packaged_count" == "$test_expected_packaged" ]]
  cmp "$test_tmp/expected startup arguments" "$test_tmp/normalized startup arguments"
  if [[ "$test_environment_mode" == scrub ]]; then
    for test_argument in MATHLAB_OCTAVE OCTAVE_EXECUTABLE DEBUG SCOPE INPUT VALUE; do
      [[ ! -e "$test_capture/$test_argument.value" ]]
    done
  else
    assert_adapter MATHLAB_OCTAVE '../inherited runtime Ω/octave-cli'
    assert_adapter OCTAVE_EXECUTABLE '/inherited fallback Ω/octave-cli'
    assert_adapter DEBUG '../inherited debug Ω.oct'
    assert_adapter SCOPE '../inherited scope Ω.oct'
    assert_adapter INPUT '../inherited input Ω.oct'
    assert_adapter VALUE '../inherited value Ω.oct'
  fi
  assert_bundles_unchanged
}

startup_case qualified-config-absent "$test_qualified" 1 explicit scrub
startup_case reserved-before-cwd "$test_qualified" 1 before-cwd scrub --packaged-startup
startup_case reserved-not-source "$test_qualified" 1 default scrub --packaged-startup
startup_case literal-split "$test_qualified" 0 explicit scrub --startup-config "$test_tmp/literal startup Ω.json"
startup_case literal-equals "$test_qualified" 0 explicit scrub "--startup-config=$test_tmp/literal startup Ω.json"
startup_case literal-missing-value "$test_qualified" 0 explicit scrub --startup-config
startup_case literal-empty-value "$test_qualified" 0 explicit scrub --startup-config ''
startup_case literal-empty-equals "$test_qualified" 0 explicit scrub --startup-config=
startup_case after-end-of-options "$test_qualified" 1 explicit scrub -- --startup-config
startup_case explicit-packaged-legacy-stamp "$test_complete" 1 explicit scrub --packaged-startup
startup_case literal-legacy-environment "$test_complete" 0 explicit preserve --startup-config "$test_tmp/literal startup Ω.json"

reject_startup_case() {
  local test_name="$1" test_bundle="$2"
  shift 2
  test_capture="$test_tmp/rejected $test_name"
  mkdir "$test_capture"
  if (
    export MATHLAB_LAUNCHER_VALUE_CAPTURE="$test_capture"
    export MATHLAB_PROJECT_DIR="$test_tmp/rejected default $test_name"
    cd -- "$test_launch_directory"
    "$test_bundle/Contents/MacOS/mathlab-launcher" "$@"
  ) > "$test_capture/stdout" 2> "$test_capture/stderr"; then
    printf 'Expected launcher selector rejection: %s\n' "$test_name" >&2
    exit 1
  fi
  [[ ! -e "$test_capture/arguments" ]]
  [[ ! -e "$test_tmp/rejected default $test_name" ]]
  assert_bundles_unchanged
}

reject_startup_case duplicate-packaged "$test_qualified" --packaged-startup --packaged-startup
reject_startup_case duplicate-literal "$test_qualified" --startup-config first.json --startup-config=second.json
reject_startup_case competing-selectors "$test_qualified" --packaged-startup --startup-config first.json
reject_startup_case qualified-octave "$test_qualified" --octave '../explicit runtime'
reject_startup_case qualified-octave-equals "$test_qualified" --startup-config=literal.json --octave=runtime
reject_startup_case qualified-debugger "$test_qualified" --debugger '../explicit debug.oct'
reject_startup_case explicit-packaged-debugger "$test_complete" --packaged-startup --debugger=debug.oct

printf '%s\n' 'PASS synthetic launcher value and packaged-startup selection, environment, relocation and read-only bundle contracts (no runtime/native/Cargo).'
