#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Synthetic script contracts only: fake cargo, compiler and runtime, no builds.
set -euo pipefail
test_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
test_tmp="$(mktemp -d "${TMPDIR:-/tmp}/mathlab verify-value.XXXXXX")"
trap 'rm -rf -- "$test_tmp"' EXIT
test_project="$test_tmp/project with spaces"
mkdir -p "$test_project/scripts" "$test_project/native" "$test_tmp/bin" "$test_tmp/runtime-only"
for test_script in verify.sh build-debugger.sh build-value-bridge.sh; do
  cp "$test_repo/scripts/$test_script" "$test_project/scripts/$test_script"
done

cat > "$test_tmp/bin/octave-cli" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == --version ]]; then printf '%s\n' "${MATHLAB_TEST_BANNER:-GNU Octave, version 11.3.0}";
else printf 'fixture %s\n' "$*" >> "$MATHLAB_TEST_LOG"; fi
STUB
cat > "$test_tmp/bin/mkoctfile" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == -p ]]; then printf '%s\n' "${MATHLAB_TEST_SDK_VERSION:-11.3.0}"; exit 0; fi
printf 'compile %s\n' "$(basename -- "$1")" >> "$MATHLAB_TEST_LOG"
if [[ "${MATHLAB_TEST_FAIL_BUILD:-0}" == 1 ]]; then exit 42; fi
while [[ $# -gt 0 ]]; do
  if [[ "$1" == -o ]]; then printf '%s\n' 'synthetic module, never load' > "$2"; exit 0; fi
  shift
done
exit 43
STUB
cat > "$test_tmp/bin/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
# Log entry before assertions so failure cases detect ANY Cargo invocation.
printf 'cargo %s\n' "$*" >> "$MATHLAB_TEST_LOG"
[[ "$MATHLAB_OCTAVE" == /* && "$MATHLAB_MKOCTFILE" == /* ]]
[[ -s "$MATHLAB_VALUE_BRIDGE" ]]
[[ "$MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS" == 1 && "$MATHLAB_REQUIRE_TYPED_PAGE_TESTS" == 1 ]]
[[ "$MATHLAB_REQUIRE_VALUE_EDIT_TESTS" == 1 && "$MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS" == 1 ]]
[[ "$MATHLAB_REQUIRE_READY_JSON_TESTS" == 1 ]]
[[ "$RUST_TEST_THREADS" == "${MATHLAB_TEST_EXPECT_THREADS:-1}" ]]
if [[ "${MATHLAB_TEST_EXPECT_DEBUGGER:-0}" == 1 ]]; then
  [[ -s "$MATHLAB_DEBUG_BRIDGE" && -s "$MATHLAB_SCOPE_BRIDGE" && -s "$MATHLAB_INPUT_BRIDGE" ]]
else
  [[ -z "${MATHLAB_DEBUG_BRIDGE+x}" && -z "${MATHLAB_SCOPE_BRIDGE+x}" && -z "${MATHLAB_INPUT_BRIDGE+x}" ]]
fi
STUB
chmod +x "$test_tmp/bin/octave-cli" "$test_tmp/bin/mkoctfile" "$test_tmp/bin/cargo"
cp "$test_tmp/bin/octave-cli" "$test_tmp/runtime-only/octave-cli"
export PATH="$test_tmp/bin:$PATH"
export MATHLAB_OCTAVE="$test_tmp/bin/octave-cli"
export MATHLAB_MKOCTFILE="$test_tmp/bin/mkoctfile"
export MATHLAB_TEST_LOG="$test_tmp/log"
unset OCTAVE_EXECUTABLE MATHLAB_DEBUG_BRIDGE MATHLAB_SCOPE_BRIDGE MATHLAB_INPUT_BRIDGE MATHLAB_VALUE_BRIDGE RUST_TEST_THREADS
unset MATHLAB_EXPECT_OCTAVE_VERSION MATHLAB_TEST_SDK_VERSION MATHLAB_TEST_FAIL_BUILD MATHLAB_TEST_BANNER

run_verify() { : > "$MATHLAB_TEST_LOG"; bash "$test_project/scripts/verify.sh" "$@" > "$test_tmp/output" 2>&1; }
expect_pre_cargo_failure() {
  if run_verify "$@"; then printf '%s\n' 'Expected provisioning failure, got success.' >&2; exit 1; fi
  if grep -q '^cargo ' "$MATHLAB_TEST_LOG"; then printf '%s\n' 'Cargo ran before prerequisites were accepted.' >&2; exit 1; fi
}

# Both actual observed banners must work through verify and each builder.
for test_banner in 'GNU Octave, version 11.3.0' 'GNU Octave (aarch64-apple-darwin25.4.0) version 11.3.0'; do
  MATHLAB_TEST_BANNER="$test_banner" run_verify --require-octave
  [[ "$(grep -c '^compile ' "$MATHLAB_TEST_LOG")" == 1 ]]
  grep -q '^compile mathlab_value_bridge.cc$' "$MATHLAB_TEST_LOG"
  grep -q '^cargo fmt --all -- --check$' "$MATHLAB_TEST_LOG"
  grep -q '^cargo check --workspace --locked$' "$MATHLAB_TEST_LOG"
  grep -q '^cargo test --workspace --locked$' "$MATHLAB_TEST_LOG"
  [[ "$(grep -c '^fixture ' "$MATHLAB_TEST_LOG")" == 6 ]]

  MATHLAB_TEST_BANNER="$test_banner" MATHLAB_TEST_EXPECT_DEBUGGER=1 RUST_TEST_THREADS=3 MATHLAB_TEST_EXPECT_THREADS=3 run_verify --with-debugger
  [[ "$(grep -c '^compile ' "$MATHLAB_TEST_LOG")" == 4 ]]
  for test_bridge in debug scope input value; do
    grep -q "^compile mathlab_${test_bridge}_bridge.cc$" "$MATHLAB_TEST_LOG"
  done
done
MATHLAB_TEST_BANNER='GNU Octave version 11.3.0' run_verify --require-octave
grep -q '^cargo test --workspace --locked$' "$MATHLAB_TEST_LOG"
# A value-only verification must not inherit unrelated adapters into Engine
# startup. Deliberately nonexistent paths are enough to expose the old mix.
MATHLAB_DEBUG_BRIDGE="$test_tmp/stale-debug.oct" MATHLAB_SCOPE_BRIDGE="$test_tmp/stale-scope.oct" MATHLAB_INPUT_BRIDGE="$test_tmp/stale-input.oct" run_verify --require-octave
grep -q '^cargo test --workspace --locked$' "$MATHLAB_TEST_LOG"
MATHLAB_TEST_BANNER='Unknown Octave runtime 11.3.0' expect_pre_cargo_failure --require-octave
grep -q 'runtime/SDK mismatch' "$test_tmp/output"
[[ "$(grep -c '^compile ' "$MATHLAB_TEST_LOG" || true)" == 0 ]]

MATHLAB_OCTAVE="$test_tmp/missing-octave" expect_pre_cargo_failure --require-octave
grep -q 'Configured Octave executable is missing' "$test_tmp/output"
MATHLAB_MKOCTFILE="$test_tmp/missing-mkoctfile" expect_pre_cargo_failure
grep -q 'Matching mkoctfile' "$test_tmp/output"
# No sibling compiler: an incompatible PATH fallback must be rejected too.
MATHLAB_OCTAVE="$test_tmp/runtime-only/octave-cli" MATHLAB_MKOCTFILE='' MATHLAB_TEST_SDK_VERSION=10.3.0 expect_pre_cargo_failure
grep -q 'runtime/SDK mismatch' "$test_tmp/output"
[[ "$(grep -c '^compile ' "$MATHLAB_TEST_LOG" || true)" == 0 ]]
test_saved_value="$(cksum "$test_project/native/mathlab_value_bridge.oct")"
MATHLAB_TEST_FAIL_BUILD=1 expect_pre_cargo_failure
grep -q '^compile mathlab_value_bridge.cc$' "$MATHLAB_TEST_LOG"
[[ "$(cksum "$test_project/native/mathlab_value_bridge.oct")" == "$test_saved_value" ]]
for test_build_dir in "$test_project/native"/.value-build-* "$test_project/native"/.build-*; do
  [[ ! -d "$test_build_dir" ]] || { printf '%s\n' 'Failed build left a private native directory.' >&2; exit 1; }
done
printf '%s\n' 'Synthetic value-adapter provisioning contracts passed (no real builds/runtime).'
