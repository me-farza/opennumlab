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


# Independent fixture names: a dropped/renamed gate proof cannot manufacture a
# matching libtest result from a count alone. These fakes never invoke Octave.
EXTRA_PROOFS = {
    "binding_session": [
        "negotiated_snapshots_and_primitive_reads_preserve_binding_context",
        "checked_edit_issues_a_new_token_and_revokes_every_old_workspace_binding",
        "same_value_and_same_shape_rebindings_cannot_reuse_an_old_token",
        "assignment_followed_by_error_revokes_tokens_and_exposes_the_partial_workspace",
        "clear_and_restart_revoke_bindings_even_when_the_variable_is_recreated",
        "another_worker_cannot_consume_a_token_for_an_identical_variable",
        "rejected_checked_edits_preserve_values_class_and_revision",
        "stop_revokes_old_bindings_and_preserves_the_same_process_partial_workspace",
        "snapshot_and_checked_object_rejection_do_not_dispatch_user_inspection_methods",
        "base_helper_name_collisions_cannot_execute_during_checked_operations",
        "shadowed_global_helpers_cannot_mutate_checked_reads_or_validated_edits",
        "optional_quiet_paused_evaluation_error_and_quit_preserve_partial_bindings",
        "optional_streamed_paused_evaluation_and_quit_preserve_partial_bindings"],
    "value_page_session": [
        "original_nd_fixture_plane_four_and_value_copy_match_independent_oracles",
        "original_integer_fixture_keeps_exact_uint64_and_signed_unsigned_limits",
        "original_complex_single_double_and_dense_logical_values_keep_their_types",
        "original_sparse_real_complex_logical_windows_use_local_ordered_coordinates",
        "original_mat_roundtrip_payload_and_loaded_values_match_independent_oracles",
        "empty_view_axes_keep_shape_while_a_zero_higher_axis_has_no_invented_plane",
        "nested_cell_struct_navigation_rejects_objects_without_running_inspection_hooks",
        "same_value_rebinding_clear_and_partial_error_revoke_typed_read_tokens",
        "identical_values_in_another_worker_or_restart_cannot_reuse_a_root_token",
        "rejected_windows_paths_and_budgets_preserve_the_current_workspace_context",
        "double_single_signed_zero_minimum_subnormal_and_nonfinite_values_round_trip",
        "project_helper_shadowing_cannot_enter_typed_page_resolution_or_publication"],
    "value_bridge_session": [
        "optional_value_bridge_startup_and_restart_refresh_capabilities",
        "optional_value_bridge_stop_restores_the_captured_helper_in_the_same_process",
        "value_bridge_invalid_paths_and_configured_hot_swap_fail_without_dispatch"],
    "value_bridge_precision_session": [
        "dense_complex_singleton_all_real_and_nd_plane_pages_preserve_imaginary_negative_zero",
        "independent_undo_and_redo_journeys_preserve_nan_payloads_signed_zeros_and_aliases",
        "original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly",
        "nested_original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly",
        "forced_complex_roots_and_nested_values_keep_state_and_missing_adapter_rejects"],
    "value_edit_session": [
        "dense_rectangle_validates_the_last_component_before_any_commit_and_never_clips",
        "every_integer_class_and_logical_keep_exact_bounds_without_saturation",
        "native_float_bits_survive_write_undo_redo_without_double_rounding",
        "complex_single_and_double_preserve_component_class_and_zero_imaginary_state",
        "original_nd_fixture_edits_plane_four_with_value_copy_and_other_planes_untouched",
        "nested_cell_struct_rebuild_preserves_opaque_siblings_aliases_and_peers",
        "sparse_rectangle_clears_implicit_zero_positions_and_restores_real_logical_complex_storage",
        "global_multi_undo_redo_across_roots_uses_fresh_context_and_exact_head_ids",
        "retained_history_is_bounded_and_reports_oldest_undo_eviction",
        "arbitrary_execution_partial_error_and_scalar_editor_invalidate_history_even_with_fresh_binding",
        "cross_worker_rebound_and_restart_reject_old_edit_and_history_authority",
        "explicit_history_reset_preserves_values_revokes_revision_and_allows_fresh_edits",
        "stop_after_a_causal_output_barrier_invalidates_prior_history_in_the_same_process"],
    "mathlab_engine": [
        "value_bridge_recovery_tests::retained_value_bridge_path_without_current_capability_is_not_successful_installation",
        "value_bridge_recovery_tests::failed_recovery_loader_closes_transport_and_latches_a_postcommit_edit_as_uncertain",
        "value_edit_lifetime_tests::finished_nested_edit_releases_opaque_sibling_before_user_clear",
        "value_edit_lifetime_tests::history_moves_release_root_while_base_alias_preserves_peer",
        "value_edit_lifetime_tests::variable_only_cleanup_preserves_base_collisions_and_unrelated_function_scope",
        "value_edit_lifetime_tests::precommit_candidate_rejection_releases_hidden_root_without_revocation",
        "value_edit_lifetime_tests::postassign_uncertainty_releases_hidden_root_before_next_user_clear",
        "value_edit_lifetime_tests::sigint_precommit_releases_candidates_without_deleting_base_peer",
        "value_edit_lifetime_tests::sigint_postassign_releases_candidates_without_deleting_base_peer",
        "value_edit_lifetime_tests::immediate_next_user_clear_proves_write_candidate_was_already_released",
        "value_edit_lifetime_tests::precommit_rejection_preserves_prior_history_independently_of_lifetime_probe",
        "value_edit_cancel_tests::sigint_before_write_revision_keeps_old_root_and_revokes_history",
        "value_edit_cancel_tests::sigint_after_write_assignment_keeps_new_root_and_reports_uncertain",
        "value_edit_cancel_tests::sigint_before_undo_revision_keeps_current_root_and_revokes_history",
        "value_edit_cancel_tests::sigint_after_undo_assignment_keeps_restored_root_and_reports_uncertain",
        "value_edit_cancel_tests::sigint_before_redo_revision_keeps_current_root_and_revokes_history",
        "value_edit_cancel_tests::sigint_after_redo_assignment_keeps_redone_root_and_reports_uncertain",
        "value_edit_submission_tests::closed_transport_rejects_public_edits_and_reset_before_publication_without_new_uncertainty",
        "value_edit_submission_tests::request_file_creation_failure_is_known_and_allows_recovery_without_reset",
        "value_edit_submission_tests::local_reset_failure_preserves_prior_uncertainty_until_a_valid_submitted_reset",
        "value_edit_submission_tests::submitted_reset_with_invalid_snapshot_stays_uncertain_until_an_explicit_valid_reset"],
    "mathlab_desktop": [
        "range_transaction_tests::view_switch_settles_committed_receipt_without_applying_old_page_to_new_variable",
        "range_transaction_tests::changed_view_draft_is_released_but_never_replaced_by_old_transaction_page",
        "range_transaction_tests::changed_selector_draft_cannot_submit_old_page_paste_history_or_reset",
        "range_transaction_tests::closed_actor_channel_releases_pending_and_reports_known_failed_send_offline",
        "range_transaction_tests::uncertainty_survives_explicit_read_and_does_not_dispatch_another_mutation",
        "range_transaction_tests::known_connected_rejection_requires_explicit_read_before_preserved_undo_or_redo",
        "range_transaction_tests::ordinary_execution_retires_successful_transaction_heads_and_page_token",
        "range_transaction_tests::stale_preview_reply_does_not_retire_newer_history_or_current_accepted_page",
        "range_transaction_tests::disconnect_before_nominally_successful_receipt_reports_commit_without_reviving_view"],
}


STUB = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
import time
name = Path(sys.argv[0]).name
args = sys.argv[1:]
mode = os.environ.get("NATIVE_GATE_TEST_MODE", "pass")
extra = json.loads(Path("synthetic-proofs.json").read_text())
required_flags = ("MATHLAB_REQUIRE_TYPED_PAGE_TESTS", "MATHLAB_REQUIRE_VALUE_EDIT_TESTS", "MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS", "MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS")
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
        assert "v=mathlab_value_bridge('protocol');" in args[-1]
        assert "mathlab_value_bridge('protocol',0)" not in args[-1]
        assert "v.max_cells==2000" in args[-1] and "v.max_rank==32" in args[-1] and "v.max_replace_leaf_bytes==134217728" in args[-1]
        if mode in ("abi-failure", "value-protocol-failure"):
            sys.exit(19)
        protocol = {"protocol":"mathlab-value", "version":1, "dense_complex_components":True,
                    "max_cells":2000, "max_rank":32, "max_replace_leaf_bytes":134217728}
        if mode == "value-json-float-version":
            protocol["version"] = 1.0
        elif mode == "value-json-bool-limit":
            protocol["max_rank"] = True
        elif mode == "value-json-wrong-limit":
            protocol["max_replace_leaf_bytes"] -= 1
        elif mode == "value-json-extra-field":
            protocol["unrequested_capability"] = True
        elif mode == "value-json-missing-field":
            del protocol["dense_complex_components"]
        elif mode == "value-json-false-capability":
            protocol["dense_complex_components"] = False
        wire = json.dumps(protocol)
        if mode == "value-json-duplicate-key":
            wire = wire[:-1] + ',"version":1}'
        elif mode == "value-json-invalid":
            wire = "not JSON"
        elif mode == "value-json-nan":
            wire = wire.replace('"version": 1', '"version": NaN')
        if mode != "value-json-missing":
            print("MATHLAB_VALUE_PROTOCOL_JSON=" + wire)
        if mode == "value-json-repeat":
            print("MATHLAB_VALUE_PROTOCOL_JSON=" + wire)
        marker = "MATHLAB_NATIVE_ABI_OK debugger=4 scope=2 input=1 value=1"
        if mode == "old-three-marker":
            marker = "MATHLAB_NATIVE_ABI_OK debugger=4 scope=2 input=1"
        print("" if mode == "abi-no-marker" else marker)
    else:
        print("11.3.0")
elif name == "mkoctfile":
    print(("11.2.0" if mode == "sdk-mismatch" else "11.3.0") if args[-1] == "OCTAVE_VERSION" else "g++")
elif name == "cargo":
    assert all(os.environ.get(key) for key in ("MATHLAB_OCTAVE", "MATHLAB_MKOCTFILE", "MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE", "MATHLAB_VALUE_BRIDGE"))
    assert all(os.environ.get(key) == "1" for key in required_flags)
    assert os.environ.get("RUST_TEST_THREADS") == "1"
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
            ("mathlab_engine", "lib", "mathlab-engine", True),
            ("binding_session", "test", "mathlab-engine", True),
            ("value_page_session", "test", "mathlab-engine", True),
            ("value_bridge_session", "test", "mathlab-engine", True),
            ("value_bridge_precision_session", "test", "mathlab-engine", True),
            ("value_edit_session", "test", "mathlab-engine", True),
            ("conformance", "test", "mathlab-cli", True),
            ("mathlab", "bin", "mathlab-cli", False)]:
            if mode == "cargo-no-cli" and target == "mathlab":
                continue
            path = (binaries / target).absolute()
            path.write_text(Path(__file__).read_text())
            path.chmod(0o755)
            print(json.dumps({"reason":"compiler-artifact", "executable":str(path),
                              "target":{"name":target,"kind":[kind]}, "package_id":package,
                              "profile":{"test":is_test}, "fresh":False}))
        if mode == "compile-source-edit":
            Path("fixture-source.txt").write_text("source changed while compiling")
        sys.exit(0)
    sys.exit("gate must execute captured binaries instead of invoking Cargo again")
elif name == "mathlab":
    assert args[-1] == "doctor" and "--json" in args
    assert all(os.environ.get(key) == "1" for key in required_flags)
    capabilities = ["binding_snapshot.v1", "checked_scalar_write.v1", "display_page.v1", "dense_complex_native_components.v1", "typed_nd_page.v1", "atomic_range_edit.v1"]
    observed = {"ready":True, "runtime":os.environ["MATHLAB_OCTAVE"], "value_adapter":os.environ["MATHLAB_VALUE_BRIDGE"], "worker_capabilities":capabilities}
    assert observed["value_adapter"] == str(Path("native/mathlab_value_bridge.oct").resolve())
    if mode == "doctor-no-json":
        print("doctor does not yet expose capability evidence")
        sys.exit(0)
    if mode == "doctor-no-capabilities":
        del observed["worker_capabilities"]
    elif mode == "doctor-missing-value-capability":
        capabilities.remove("dense_complex_native_components.v1")
    elif mode == "doctor-missing-typed-capability":
        capabilities.remove("typed_nd_page.v1")
    elif mode == "doctor-missing-edit-capability":
        capabilities.remove("atomic_range_edit.v1")
    elif mode == "doctor-duplicate-capability":
        capabilities.append(capabilities[0])
    elif mode == "doctor-capabilities-not-array":
        observed["worker_capabilities"] = ",".join(capabilities)
    elif mode == "doctor-wrong-value-path":
        observed["value_adapter"] = "/stale/value.oct"
    elif mode == "doctor-wrong-runtime":
        observed["runtime"] = "/stale/octave-cli"
    elif mode == "doctor-not-ready":
        observed["ready"] = False
    elif mode in ("doctor-value-edit", "doctor-runtime-edit", "doctor-cli-edit"):
        changed = {"doctor-value-edit":Path(observed["value_adapter"]),
                   "doctor-runtime-edit":Path(observed["runtime"]), "doctor-cli-edit":Path(__file__)}[mode]
        with changed.open("a") as output:
            output.write("\n# concurrently changed doctor input\n")
    wire = json.dumps(observed)
    if mode == "doctor-duplicate-key":
        wire = wire[:-1] + ',"ready":true}'
    elif mode == "doctor-nan":
        wire = wire[:-1] + ',"invalid_number":NaN}'
    print(wire)
elif name in ("octave_debugger", "octave_session", "input_session", "mathlab_desktop", "conformance") or name in extra:
    assert all(os.environ.get(key) == "1" for key in required_flags)
    if mode == "timeout":
        time.sleep(5)
    if "--exact" in args:
        names = [args[0]]
    elif name == "octave_debugger":
        names = ["real_breakpoint_scopes_step_and_continue", "guarded_debug_quit_preserves_assignments_and_allows_more_commands", "step_into_and_out_preserve_function_scopes", "nested_debug_evaluation_can_resume_and_restores_enclosing_pause"]
    elif name == "conformance":
        names = ["actual_three_route_report_preserves_unavailable_and_source_preparation_boundaries", "synthetic_saved_references_are_rejected_without_publishing_a_reference_match", "observed_unavailable_reference_and_tampered_hashes_are_rejected", "unexpected_fixture_assertion_failure_is_identified_and_exits_unsuccessfully", "two_agreeing_successful_routes_cannot_pass_with_an_empty_typed_result", "known_sparse_logical_mat_gap_remains_a_failure_in_both_actual_routes", "missing_octave_is_explicitly_unavailable_and_never_a_match", "strict_incomplete_acceptance_fails_after_publishing_the_complete_report", "malformed_reference_case_sets_are_rejected_before_route_provenance", "original_core_smoke_runner_keeps_its_schema_and_unique_fixture_execution"]
    elif name in extra:
        names = extra[name]
        if args and not args[0].startswith("--"):
            names = [test for test in names if args[0] in test]
    else:
        names = ["optional_input_preserves_caller_scope_unicode_echo_and_ordered_prompt_events", "optional_input_nested_expression_consumes_outer_line_before_inner_request", "optional_input_stop_and_restart_revoke_old_keys_without_cross_consuming_recovery", "optional_input_clear_and_no_output_calls_keep_the_original_builtin_contract", "optional_input_eof_preserves_original_error_and_next_request_remains_usable", "optional_input_caught_corrupt_reply_finishes_error_before_the_next_read", "optional_input_rejects_late_activation_after_old_builtin_handle_capture"]
    if mode == "zero-tests":
        names = []
    if mode == "missing-value-test" and name == "value_page_session":
        names = names[1:]
    if mode == "wrong-value-test-name" and name == "value_edit_session":
        names = ["unrelated_proof"] + names[1:]
    if mode == "extra-value-test" and name == "value_bridge_session":
        names = names + ["unexpected_extra_test"]
    for test in names:
        print("test " + test + " ... ok")
    if mode == "skip" or (mode == "cli-skip" and name == "conformance") or (mode == "value-skip" and name == "value_bridge_session"):
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
        elif mode == "value-artifact-edit":
            Path("native/mathlab_value_bridge.oct").write_text("changed value artifact")
        elif mode in ("runtime-edit", "sdk-edit"):
            setting = "MATHLAB_OCTAVE" if mode == "runtime-edit" else "MATHLAB_MKOCTFILE"
            with Path(os.environ[setting]).open("a") as output:
                output.write("\n# concurrently changed runtime or SDK\n")
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
for name in ("debug", "scope", "input", "value"):
    mode = os.environ.get("NATIVE_GATE_TEST_MODE")
    if (name == "input" and mode == "missing-artifact") or (name == "value" and mode in ("missing-value", "stale-value")):
        continue
    target = root / ("mathlab_" + name + "_bridge.oct")
    staged = target.with_suffix(".fresh")
    if name == "value" and mode == "symlink-value":
        staged.symlink_to(Path("../fixture-source.txt"))
    else:
        staged.write_bytes(b"synthetic adapter, not executable")
    staged.replace(target)
(root / "runtime-version.txt").write_text("11.3.0\\n")
PY
'''


class NativeGateTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mathlab-native-gate-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "scripts").mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        source = Path(__file__).resolve().with_name("release-native-gate.sh")
        shutil.copyfile(source, self.root / "scripts/release-native-gate.sh")
        (self.root / "scripts/build-debugger.sh").write_text(BUILD)
        (self.root / "fixture-source.txt").write_text("initial source bytes")
        (self.root / "synthetic-proofs.json").write_text(json.dumps(EXTRA_PROOFS))
        for name in ("git", "rustc", "cargo", "octave-cli", "mkoctfile"):
            path = self.bin / name
            path.write_text(STUB)
            path.chmod(0o755)
        self.environment = os.environ.copy()
        for key in ("MATHLAB_OCTAVE", "MATHLAB_MKOCTFILE", "MATHLAB_CARGO", "MATHLAB_EXPECT_OCTAVE_VERSION"):
            self.environment.pop(key, None)
        self.environment["PATH"] = str(self.bin) + ":/usr/bin:/bin"
        self.report_dir = self.root / "target/native-four-adapter-value-v1"

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
        self.assertEqual(report["schema_version"], 3)
        self.assertEqual(report["contract"], "native-four-adapter-value-v1")
        self.assertEqual(report["adapter_profile"], {"schema_version": 1, "name": "four-adapter-value-v1"})
        self.assertTrue(report["native_enabled"])
        self.assertEqual(len(report["suites"]), 21)
        self.assertEqual(sum(suite["passed"] for suite in report["suites"]), 105)
        self.assertEqual(report["runtime"]["version"], report["runtime"]["sdk_version"])
        self.assertEqual(report["source"]["commit"], "1234567890abcdef1234567890abcdef12345678")
        self.assertEqual(report["source"], report["source_after"])
        self.assertIn("scripts/build-debugger.sh", {item["path"] for item in report["source"]["files"]})
        self.assertTrue(report["source_unchanged"])
        self.assertEqual(len(report["test_executables"]), 12)
        self.assertEqual(report["required_test_count"], 105)
        self.assertEqual(report["executed_required_test_count"], 105)
        self.assertEqual(report["executed_test_count"], 105)
        self.assertEqual(report["historical_three_adapter_required_test_count"], 27)
        self.assertFalse(report["release_candidate_qualification"])
        for binary in report["test_executables"]:
            self.assertEqual(binary["sha256"], hashlib.sha256(Path(binary["path"]).read_bytes()).hexdigest())
            self.assertIs(binary["cargo_fresh"], False)
        self.assertEqual(len(report["artifacts"]), 4)
        self.assertEqual(report["protocols"]["mathlab-value"], 1)
        self.assertEqual(report["value_protocol"], {"protocol": "mathlab-value", "version": 1,
                         "dense_complex_components": True, "max_cells": 2000, "max_rank": 32,
                         "max_replace_leaf_bytes": 134217728})
        observation = report["worker_observation"]
        self.assertEqual(observation["method"], "built-cli-doctor")
        self.assertIn("dense_complex_native_components.v1", observation["worker_capabilities"])
        self.assertIn("typed_nd_page.v1", observation["worker_capabilities"])
        self.assertIn("atomic_range_edit.v1", observation["worker_capabilities"])
        self.assertEqual(observation["value_adapter"], str(self.root / "native/mathlab_value_bridge.oct"))
        self.assertEqual(observation["module_sha256"], report["artifacts"]["mathlab_value_bridge.oct"]["sha256"])
        self.assertEqual(observation["runtime_executable_sha256"], hashlib.sha256((self.bin / "octave-cli").read_bytes()).hexdigest())
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
        for mode in ("sdk-mismatch", "compile-failure", "missing-artifact", "abi-failure", "abi-no-marker", "old-three-marker", "value-protocol-failure", "cargo-no-artifacts", "cargo-no-cli"):
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
        for mode in ("test-executable-edit", "native-artifact-edit", "value-artifact-edit", "cli-binary-edit",
                     "doctor-value-edit", "doctor-runtime-edit", "doctor-cli-edit"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertIn("changed", report["reason"])

    def test_missing_stale_or_symlinked_value_module_cannot_enable_the_gate(self):
        for mode in ("missing-value", "stale-value", "symlink-value"):
            with self.subTest(mode=mode):
                shutil.rmtree(self.root / "native", ignore_errors=True)
                if mode == "stale-value":
                    (self.root / "native").mkdir()
                    (self.root / "native/mathlab_value_bridge.oct").write_bytes(b"pre-existing stale module")
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertFalse(report["native_enabled"])
                self.assertEqual(report["suites"], [])
                self.assertNotIn("worker_observation", report)
                if mode == "stale-value":
                    self.assertIn("freshly replace all four", report["reason"])

    def test_identical_module_bytes_still_require_fresh_replacement_inodes(self):
        native = self.root / "native"
        native.mkdir()
        for name in ("debug", "scope", "input", "value"):
            (native / ("mathlab_" + name + "_bridge.oct")).write_bytes(b"synthetic adapter, not executable")
        result, report = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for record in report["artifacts"].values():
            self.assertTrue(record["fresh_build"])
            self.assertEqual(record["sha256"], record["previous_identity"]["sha256"])
            self.assertNotEqual((record["device"], record["inode"]),
                                (record["previous_identity"]["device"], record["previous_identity"]["inode"]))

    def test_actual_value_protocol_requires_exact_json_types_limits_and_unique_fields(self):
        for mode in ("value-json-float-version", "value-json-bool-limit", "value-json-wrong-limit",
                     "value-json-extra-field", "value-json-missing-field", "value-json-false-capability",
                     "value-json-duplicate-key", "value-json-invalid", "value-json-nan",
                     "value-json-missing", "value-json-repeat"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertFalse(report["native_enabled"])
                self.assertEqual(report["suites"], [])
                self.assertNotIn("value_protocol", report)
                self.assertIn("value protocol", report["reason"].lower())

    def test_doctor_must_publish_observed_unique_capabilities_and_exact_paths(self):
        for mode in ("doctor-no-json", "doctor-no-capabilities", "doctor-missing-value-capability",
                     "doctor-missing-typed-capability", "doctor-missing-edit-capability",
                     "doctor-duplicate-capability", "doctor-capabilities-not-array", "doctor-wrong-value-path",
                     "doctor-wrong-runtime", "doctor-not-ready", "doctor-duplicate-key", "doctor-nan"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertFalse(report["native_enabled"])
                self.assertEqual(report["suites"], [])
                self.assertIn("Built CLI doctor", report["reason"])
                self.assertEqual(report["executed_required_test_count"], 0)

    def test_value_proofs_must_be_named_complete_exactly_counted_and_enabled(self):
        for mode in ("missing-value-test", "wrong-value-test-name", "extra-value-test", "value-skip"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertTrue(report["native_enabled"])
                self.assertEqual(report["required_test_count"], 105)
                self.assertLess(report["executed_required_test_count"], 105)
                self.assertEqual(report["suites"][-1]["status"], "failed")
                self.assertIn("reported a skip" if mode == "value-skip" else "did not execute every required", report["reason"])

    def test_runtime_and_sdk_identity_changes_cannot_pass(self):
        for mode in ("runtime-edit", "sdk-edit"):
            with self.subTest(mode=mode):
                result, report = self.run_gate(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["status"], "failed")
                self.assertIn("executable before gate completion changed", report["reason"])

    def test_inherited_opt_out_flags_and_value_path_cannot_weaken_required_profile(self):
        required = ("MATHLAB_REQUIRE_TYPED_PAGE_TESTS", "MATHLAB_REQUIRE_VALUE_EDIT_TESTS",
                    "MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS", "MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS",
                    "RUST_TEST_THREADS")
        for name in required:
            self.environment[name] = "0"
        self.environment["MATHLAB_VALUE_BRIDGE"] = "/stale/value.oct"
        result, report = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["required_environment"], {name: "1" for name in required})

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
