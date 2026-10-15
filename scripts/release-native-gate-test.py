#!/usr/bin/env python3
"""Synthetic gate regressions; never compile or launch the real runtime."""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


STUB = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
import time
name = Path(sys.argv[0]).name
args = sys.argv[1:]
mode = os.environ.get("NATIVE_GATE_TEST_MODE", "pass")
if name == "git":
    if args[0] == "rev-parse":
        print("f" * 40 if Path("changed-head").exists() else "1234567890abcdef1234567890abcdef12345678")
    elif args[0] == "ls-files":
        names = ["fixture-source.txt", "scripts/release-native-gate.sh", "scripts/build-debugger.sh"]
        if Path("added-source.rs").exists():
            names.append("added-source.rs")
        if Path("target/tracked-source.rs").exists():
            names.append("target/tracked-source.rs")
        if "--others" not in args:
            directory = args[-1].rstrip("/")
            names = [path for path in names if path == directory or path.startswith(directory + "/")]
        if names:
            sys.stdout.buffer.write(("\0".join(names) + "\0").encode())
    elif args[0] == "check-ignore":
        sys.exit(0 if args[-1].startswith("target/") else 1)
    elif args[0] == "status":
        # Deliberately stay dirty both before and after a source mutation.
        sys.stdout.buffer.write(b" M fixture-source.txt\0")
elif name == "rustc":
    print("rustc synthetic (no compilation)")
elif name == "octave-cli":
    if "mathlab_debug_bridge" in args[-1]:
        if mode == "abi-failure":
            sys.exit(19)
        print("" if mode == "abi-no-marker" else "MATHLAB_NATIVE_ABI_OK debugger=4 scope=2 input=1")
    else:
        print("11.3.0")
elif name == "mkoctfile":
    print(("11.2.0" if mode == "sdk-mismatch" else "11.3.0") if args[-1] == "OCTAVE_VERSION" else "g++")
elif name == "cargo":
    assert all(os.environ.get(key) for key in ("MATHLAB_OCTAVE", "MATHLAB_MKOCTFILE", "MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE"))
    if "--no-run" in args:
        if mode == "compile-failure":
            sys.exit(17)
        if mode == "cargo-no-artifacts":
            print('{"reason":"build-finished","success":true}')
            sys.exit(0)
        binaries = Path("synthetic-target")
        binaries.mkdir(exist_ok=True)
        for target, kind, package, is_test in [
            ("octave_debugger", "test", "mathlab-debugger", True),
            ("octave_session", "test", "mathlab-engine", True),
            ("input_session", "test", "mathlab-engine", True),
            ("mathlab_desktop", "lib", "mathlab-desktop", True),
            ("conformance", "test", "mathlab-cli", True),
            ("mathlab", "bin", "mathlab-cli", False)]:
            if mode == "cargo-no-cli" and target == "mathlab":
                continue
            path = (binaries / target).absolute()
            path.write_text(Path(__file__).read_text())
            path.chmod(0o755)
            print(json.dumps({"reason":"compiler-artifact", "executable":str(path),
                              "target":{"name":target,"kind":[kind]}, "package_id":package,
                              "profile":{"test":is_test}}))
        if mode == "compile-source-edit":
            Path("fixture-source.txt").write_text("source changed while compiling")
        sys.exit(0)
    sys.exit("gate must execute captured binaries instead of invoking Cargo again")
elif name in ("octave_debugger", "octave_session", "input_session", "mathlab_desktop", "conformance"):
    if mode == "timeout":
        time.sleep(5)
    if "--exact" in args:
        names = [args[0]]
    elif name == "octave_debugger":
        names = ["real_breakpoint_scopes_step_and_continue", "guarded_debug_quit_preserves_assignments_and_allows_more_commands", "step_into_and_out_preserve_function_scopes", "nested_debug_evaluation_can_resume_and_restores_enclosing_pause"]
    elif name == "conformance":
        names = ["actual_three_route_report_preserves_unavailable_and_source_preparation_boundaries", "synthetic_saved_references_are_rejected_without_publishing_a_reference_match", "observed_unavailable_reference_and_tampered_hashes_are_rejected", "unexpected_fixture_assertion_failure_is_identified_and_exits_unsuccessfully", "two_agreeing_successful_routes_cannot_pass_with_an_empty_typed_result", "known_sparse_logical_mat_gap_remains_a_failure_in_both_actual_routes", "missing_octave_is_explicitly_unavailable_and_never_a_match", "strict_incomplete_acceptance_fails_after_publishing_the_complete_report", "malformed_reference_case_sets_are_rejected_before_route_provenance", "original_core_smoke_runner_keeps_its_schema_and_unique_fixture_execution"]
    else:
        names = ["optional_input_preserves_caller_scope_unicode_echo_and_ordered_prompt_events", "optional_input_nested_expression_consumes_outer_line_before_inner_request", "optional_input_stop_and_restart_revoke_old_keys_without_cross_consuming_recovery", "optional_input_clear_and_no_output_calls_keep_the_original_builtin_contract", "optional_input_eof_preserves_original_error_and_next_request_remains_usable", "optional_input_caught_corrupt_reply_finishes_error_before_the_next_read", "optional_input_rejects_late_activation_after_old_builtin_handle_capture"]
    if mode == "zero-tests":
        names = []
    for test in names:
        print("test " + test + " ... ok")
    if mode == "skip" or (mode == "cli-skip" and name == "conformance"):
        print("Skipping native debugger proof: adapter unavailable", file=sys.stderr)
    if name == "octave_debugger":
        if mode == "source-edit":
            Path("fixture-source.txt").write_text("concurrently edited source")
        elif mode == "source-add":
            Path("added-source.rs").write_text("concurrently added source")
        elif mode == "source-commit":
            Path("changed-head").write_text("concurrent commit")
        elif mode == "test-executable-edit":
            with Path(__file__).open("a") as output:
                output.write("\n# concurrently changed executable\n")
        elif mode == "native-artifact-edit":
            Path("native/mathlab_debug_bridge.oct").write_text("changed native artifact")
    if name == "conformance" and mode == "cli-binary-edit":
        with Path("synthetic-target/mathlab").open("a") as output:
            output.write("\n# concurrently changed CLI binary\n")
    print("test result: ok. %d passed; 0 failed; %d ignored; 0 measured; 0 filtered out" % (len(names), mode == "ignored"))
'''

BUILD = '''#!/usr/bin/env bash
set -euo pipefail
python3 - <<'PY'
import os
from pathlib import Path
root = Path("native")
root.mkdir(exist_ok=True)
for name in ("debug", "scope", "input"):
    if name == "input" and os.environ.get("NATIVE_GATE_TEST_MODE") == "missing-artifact":
        continue
    (root / ("mathlab_" + name + "_bridge.oct")).write_bytes(b"synthetic adapter, not executable")
(root / "runtime-version.txt").write_text("11.3.0\\n")
PY
'''


class NativeGateTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mathlab-native-gate-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        source = Path(__file__).resolve().with_name("release-native-gate.sh")
        shutil.copyfile(source, self.root / "scripts/release-native-gate.sh")
        (self.root / "scripts/build-debugger.sh").write_text(BUILD)
        (self.root / "fixture-source.txt").write_text("initial source bytes")
        for name in ("git", "rustc", "cargo", "octave-cli", "mkoctfile"):
            path = self.bin / name
            path.write_text(STUB)
            path.chmod(0o755)
        self.environment = os.environ.copy()
        for key in ("MATHLAB_OCTAVE", "MATHLAB_MKOCTFILE", "MATHLAB_CARGO", "MATHLAB_EXPECT_OCTAVE_VERSION"):
            self.environment.pop(key, None)
        self.environment["PATH"] = str(self.bin) + ":/usr/bin:/bin"
        self.report_dir = self.root / "target/release-native-gate"

    def run_gate(self, mode="pass", arguments=()):
        self.environment["NATIVE_GATE_TEST_MODE"] = mode
        result = subprocess.run(["bash", str(self.root / "scripts/release-native-gate.sh"),
                                 "--report-dir", str(self.report_dir), *arguments],
                                env=self.environment, capture_output=True, text=True, timeout=20)
        report = json.loads((self.report_dir / "report.json").read_text())
        return result, report

    def test_enabled_lane_requires_all_proofs_and_records_artifact_identity(self):
        self.environment["MATHLAB_EXPECT_OCTAVE_VERSION"] = "11.3.0"
        result, report = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["status"], "passed")
        self.assertTrue(report["native_enabled"])
        self.assertEqual(len(report["suites"]), 9)
        self.assertEqual(sum(suite["passed"] for suite in report["suites"]), 27)
        self.assertEqual(report["runtime"]["version"], report["runtime"]["sdk_version"])
        self.assertEqual(report["source"]["commit"], "1234567890abcdef1234567890abcdef12345678")
        self.assertEqual(report["source"], report["source_after"])
        self.assertIn("scripts/build-debugger.sh", {item["path"] for item in report["source"]["files"]})
        self.assertTrue(report["source_unchanged"])
        self.assertEqual(len(report["test_executables"]), 6)
        self.assertEqual(report["required_test_count"], 27)
        self.assertFalse(report["release_candidate_qualification"])
        for binary in report["test_executables"]:
            self.assertEqual(binary["sha256"], hashlib.sha256(Path(binary["path"]).read_bytes()).hexdigest())
        self.assertEqual(report["artifacts"]["mathlab_debug_bridge.oct"]["sha256"],
                         hashlib.sha256(b"synthetic adapter, not executable").hexdigest())
        for entry in report["commands"]:
            self.assertEqual(entry["sha256"], hashlib.sha256((self.report_dir / entry["log"]).read_bytes()).hexdigest())

    def test_required_lane_fails_when_prerequisites_are_missing(self):
        self.environment["MATHLAB_OCTAVE"] = "/missing/octave-cli"
        result, report = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(report["status"], "failed")
        self.assertFalse(report["native_enabled"])
        self.assertIn("Missing native prerequisites", report["reason"])

    def test_opt_out_reports_skipped_without_claiming_native_execution(self):
        self.environment["MATHLAB_OCTAVE"] = "/missing/octave-cli"
        result, report = self.run_gate(arguments=["--allow-skip"])
        self.assertEqual(result.returncode, 0)
        self.assertEqual(report["status"], "skipped")
        self.assertFalse(report["native_enabled"])
        self.assertEqual(report["suites"], [])

    def test_early_returns_ignored_and_zero_tests_cannot_pass(self):
        for mode in ("skip", "ignored", "zero-tests"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertNotEqual(report["suites"][0]["status"], "passed")

    def test_sdk_version_build_failure_and_missing_artifact_fail(self):
        for mode in ("sdk-mismatch", "compile-failure", "missing-artifact", "abi-failure", "abi-no-marker", "cargo-no-artifacts", "cargo-no-cli"):
            with self.subTest(mode=mode):
                # Each check must start with no synthetic build outputs.
                shutil.rmtree(self.root / "native", ignore_errors=True)
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertEqual(report["suites"], [])

    def test_expected_runtime_version_is_required(self):
        self.environment["MATHLAB_EXPECT_OCTAVE_VERSION"] = "11.2.0"
        result, report = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Expected Octave 11.2.0", report["reason"])

    def test_cargo_symlink_keeps_dispatcher_name(self):
        cargo = self.bin / "cargo"
        dispatcher = self.bin / "tool-dispatcher"
        cargo.rename(dispatcher)
        cargo.symlink_to(dispatcher)
        result, report = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["status"], "passed")

    def test_native_test_hang_is_bounded_and_reported(self):
        result, report = self.run_gate("timeout", ["--test-timeout-seconds", "1"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("timed out", report["reason"])
        self.assertTrue(report["commands"][-1]["timed_out"])

    def test_source_edits_additions_and_commits_fail_even_when_dirty_stays_true(self):
        for mode in ("source-edit", "source-add", "source-commit", "compile-source-edit"):
            with self.subTest(mode=mode):
                (self.root / "fixture-source.txt").write_text("initial source bytes")
                for path in (self.root / "added-source.rs", self.root / "changed-head"):
                    path.unlink(missing_ok=True)
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertTrue(report["source"]["dirty"])
                self.assertTrue(report["source_after"]["dirty"])
                self.assertFalse(report["source_unchanged"])
                self.assertIn("Source snapshot changed", report["reason"])

    def test_changed_test_executable_and_native_artifact_fail(self):
        for mode in ("test-executable-edit", "native-artifact-edit", "cli-binary-edit"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertIn("changed", report["reason"])

    def test_cli_conformance_skip_cannot_pass(self):
        result, report = self.run_gate("cli-skip")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cli-conformance reported a skip", report["reason"])

    def test_source_symlink_cannot_hide_mutable_build_inputs(self):
        source = self.root / "fixture-source.txt"
        source.rename(self.root / "outside-source.txt")
        source.symlink_to(self.root / "outside-source.txt")
        result, report = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Source symlink", report["reason"])

    def test_report_directory_cannot_exclude_source_before_any_write(self):
        build_script = self.root / "scripts/build-debugger.sh"
        original = build_script.read_bytes()
        self.environment["NATIVE_GATE_TEST_MODE"] = "source-edit"
        (self.root / "target").mkdir()
        (self.root / "target/tracked-source.rs").write_text("tracked despite ignored parent")
        for relative in ("scripts", "not-ignored-output", "target"):
            with self.subTest(relative=relative):
                output = self.root / relative
                result = subprocess.run(["bash", str(self.root / "scripts/release-native-gate.sh"),
                                         "--report-dir", str(output)], env=self.environment,
                                        capture_output=True, text=True, timeout=5)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("must be ignored and contain no tracked", result.stderr)
                self.assertFalse((output / "report.json").exists())
                self.assertFalse((output / "runtime-version.log").exists())
                self.assertEqual(build_script.read_bytes(), original)
        self.assertFalse((self.root / "not-ignored-output").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
