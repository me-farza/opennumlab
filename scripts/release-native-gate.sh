#!/usr/bin/env bash
# Require executed native proofs; libtest's successful early returns are not passes.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
mathlab_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 - "$mathlab_repo" "$@" <<'PY'
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time

repo = Path(sys.argv[1]).resolve()
parser = argparse.ArgumentParser(description="Require the versioned four-adapter value/native source contract.")
parser.add_argument("--report-dir", type=Path, default=repo / "target/native-four-adapter-value-v1")
parser.add_argument("--allow-skip", action="store_true", help="Explicit local opt-out if prerequisites are absent; CI must omit this.")
parser.add_argument("--build-timeout-seconds", type=int, default=900)
parser.add_argument("--test-timeout-seconds", type=int, default=180)
args = parser.parse_args(sys.argv[2:])
for bound in (args.build_timeout_seconds, args.test_timeout_seconds):
    if not 1 <= bound <= 3600:
        parser.error("timeouts must be between 1 and 3600 seconds")
report_dir = args.report_dir.resolve()
# Never let a caller turn a source subtree into an inventory exclusion. Reports
# inside the checkout must already belong to ignored output space and contain
# no tracked inputs. Validate before creating a directory or writing any log.
try:
    output_relative = report_dir.relative_to(repo)
except ValueError:
    pass
else:
    if not output_relative.parts:
        parser.error("Report directory must not be the source repository root")
    try:
        tracked = subprocess.check_output(["git", "ls-files", "--cached", "-z", "--", output_relative.as_posix()], cwd=repo, timeout=30)
        ignored = subprocess.run(["git", "check-ignore", "--no-index", "--quiet", "--", output_relative.as_posix() + "/.native-gate-output"], cwd=repo, timeout=30)
    except (OSError, subprocess.SubprocessError) as error:
        parser.error("Cannot validate report directory: " + str(error))
    if tracked or ignored.returncode != 0:
        parser.error("In-repository report directory must be ignored and contain no tracked source/build inputs")
report_dir.mkdir(parents=True, exist_ok=True)
CONTRACT = "native-four-adapter-value-v1"
ADAPTERS = (("MATHLAB_DEBUG_BRIDGE", "mathlab_debug_bridge.oct"),
            ("MATHLAB_SCOPE_BRIDGE", "mathlab_scope_bridge.oct"),
            ("MATHLAB_INPUT_BRIDGE", "mathlab_input_bridge.oct"),
            ("MATHLAB_VALUE_BRIDGE", "mathlab_value_bridge.oct"))
REQUIRED_FLAGS = ("MATHLAB_REQUIRE_TYPED_PAGE_TESTS", "MATHLAB_REQUIRE_VALUE_EDIT_TESTS",
                  "MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS", "MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS")
REQUIRED_CAPABILITIES = ("dense_complex_native_components.v1", "typed_nd_page.v1", "atomic_range_edit.v1")
# Explicit named proofs, rather than a broad lib-test invocation that can hide
# optional early returns or accidentally run unrelated optional adapter tests.
VALUE_SUITES = [
    ("binding-session", "mathlab-engine", ["--test", "binding_session"], [
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
        "optional_streamed_paused_evaluation_and_quit_preserve_partial_bindings"], None),
    ("typed-pages", "mathlab-engine", ["--test", "value_page_session"], [
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
        "project_helper_shadowing_cannot_enter_typed_page_resolution_or_publication"], None),
    ("value-lifecycle", "mathlab-engine", ["--test", "value_bridge_session"], [
        "optional_value_bridge_startup_and_restart_refresh_capabilities",
        "optional_value_bridge_stop_restores_the_captured_helper_in_the_same_process",
        "value_bridge_invalid_paths_and_configured_hot_swap_fail_without_dispatch"], None),
    ("value-precision", "mathlab-engine", ["--test", "value_bridge_precision_session"], [
        "dense_complex_singleton_all_real_and_nd_plane_pages_preserve_imaginary_negative_zero",
        "independent_undo_and_redo_journeys_preserve_nan_payloads_signed_zeros_and_aliases",
        "original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly",
        "nested_original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly",
        "forced_complex_roots_and_nested_values_keep_state_and_missing_adapter_rejects"], None),
    ("range-edits", "mathlab-engine", ["--test", "value_edit_session"], [
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
        "stop_after_a_causal_output_barrier_invalidates_prior_history_in_the_same_process"], None),
    ("value-recovery", "mathlab-engine", ["--lib"], [
        "retained_value_bridge_path_without_current_capability_is_not_successful_installation",
        "failed_recovery_loader_closes_transport_and_latches_a_postcommit_edit_as_uncertain"], "value_bridge_recovery_tests::"),
    ("edit-lifetime", "mathlab-engine", ["--lib"], [
        "finished_nested_edit_releases_opaque_sibling_before_user_clear",
        "history_moves_release_root_while_base_alias_preserves_peer",
        "variable_only_cleanup_preserves_base_collisions_and_unrelated_function_scope",
        "precommit_candidate_rejection_releases_hidden_root_without_revocation",
        "postassign_uncertainty_releases_hidden_root_before_next_user_clear",
        "sigint_precommit_releases_candidates_without_deleting_base_peer",
        "sigint_postassign_releases_candidates_without_deleting_base_peer",
        "immediate_next_user_clear_proves_write_candidate_was_already_released",
        "precommit_rejection_preserves_prior_history_independently_of_lifetime_probe"], "value_edit_lifetime_tests::"),
    ("edit-cancellation", "mathlab-engine", ["--lib"], [
        "sigint_before_write_revision_keeps_old_root_and_revokes_history",
        "sigint_after_write_assignment_keeps_new_root_and_reports_uncertain",
        "sigint_before_undo_revision_keeps_current_root_and_revokes_history",
        "sigint_after_undo_assignment_keeps_restored_root_and_reports_uncertain",
        "sigint_before_redo_revision_keeps_current_root_and_revokes_history",
        "sigint_after_redo_assignment_keeps_redone_root_and_reports_uncertain"], "value_edit_cancel_tests::"),
    ("edit-submission", "mathlab-engine", ["--lib"], [
        "closed_transport_rejects_public_edits_and_reset_before_publication_without_new_uncertainty",
        "request_file_creation_failure_is_known_and_allows_recovery_without_reset",
        "local_reset_failure_preserves_prior_uncertainty_until_a_valid_submitted_reset",
        "submitted_reset_with_invalid_snapshot_stays_uncertain_until_an_explicit_valid_reset"], "value_edit_submission_tests::"),
    ("desktop-transactions", "mathlab-desktop", ["--lib"], [
        "view_switch_settles_committed_receipt_without_applying_old_page_to_new_variable",
        "changed_view_draft_is_released_but_never_replaced_by_old_transaction_page",
        "changed_selector_draft_cannot_submit_old_page_paste_history_or_reset",
        "closed_actor_channel_releases_pending_and_reports_known_failed_send_offline",
        "uncertainty_survives_explicit_read_and_does_not_dispatch_another_mutation",
        "known_connected_rejection_requires_explicit_read_before_preserved_undo_or_redo",
        "ordinary_execution_retires_successful_transaction_heads_and_page_token",
        "stale_preview_reply_does_not_retire_newer_history_or_current_accepted_page",
        "disconnect_before_nominally_successful_receipt_reports_commit_without_reviving_view"], "range_transaction_tests::"),
    ("desktop-typed-pages", "mathlab-desktop", ["--lib"], [
        "worker::tests::desktop_worker_typed_nd_route_preserves_plane_context_and_rejects_rebinding"], None),
    ("desktop-range-actor", "mathlab-desktop", ["--lib"], [
        "worker::tests::desktop_range_actor_routes_exact_nd_paste_history_and_stale_reset_without_retry"], None),
]
report = {"schema_version": 3, "contract": CONTRACT, "status": "failed", "native_enabled": False,
          "adapter_profile": {"schema_version": 1, "name": "four-adapter-value-v1"},
          "required_test_count": 27 + sum(len(item[3]) for item in VALUE_SUITES),
          "executed_required_test_count": 0, "executed_test_count": 0,
          "historical_three_adapter_required_test_count": 27,
          "allow_skip": args.allow_skip, "commands": [], "suites": [], "artifacts": {}, "test_executables": [],
          "release_candidate_qualification": False,
          "scope": "Four-adapter value/native source contract, including selected CLI and desktop controller contracts; no installed-candidate, GUI, Core, Full or MATLAB parity qualification.",
          "limitations": ["Historical three-adapter/27-proof evidence cannot satisfy this versioned four-adapter contract.",
                          "CLI conformance intentionally exits zero for incomplete evidence. Final candidate acceptance must inspect every required route, oracle comparison and summary; this source CI gate does not accept an installed release."]}
environment = os.environ.copy()
environment.update({name: "1" for name in REQUIRED_FLAGS})
environment["RUST_TEST_THREADS"] = "1"
report["required_environment"] = {name: environment[name] for name in (*REQUIRED_FLAGS, "RUST_TEST_THREADS")}


class GateFailure(Exception):
    pass


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def file_identity(path):
    if path.is_symlink() or not path.is_file():
        raise GateFailure("Expected a regular, non-symlinked file: " + str(path))
    info = path.stat()
    return {"path": str(path), "sha256": digest(path), "bytes": info.st_size,
            "mode": stat.S_IMODE(info.st_mode), "device": info.st_dev, "inode": info.st_ino}


def verify_file(record, description):
    current = file_identity(Path(record["path"]))
    if any(current[key] != record[key] for key in current):
        raise GateFailure(description + " changed: " + record["path"])


def strict_json(text, description):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise GateFailure(description + " contains duplicate JSON key: " + key)
            result[key] = value
        return result

    def invalid_constant(value):
        raise GateFailure(description + " contains a non-JSON numeric constant: " + value)

    try:
        return json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    except ValueError as error:
        raise GateFailure(description + " must publish JSON: " + str(error))


def verify_runtime():
    runtime = report["runtime"]
    for setting, identity, description in (
            ("executable", "executable_identity", "Runtime executable before gate completion"),
            ("mkoctfile", "mkoctfile_identity", "SDK executable before gate completion")):
        record = runtime[identity]
        if Path(runtime[setting]).resolve() != Path(record["path"]):
            raise GateFailure(description + " changed its resolved path")
        verify_file(record, description)


def verify_native_inputs():
    verify_runtime()
    for record in report["artifacts"].values():
        verify_file(record, "Native artifact")


def git_bytes(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=repo, timeout=30)


def source_snapshot():
    """Fingerprint actual input bytes, modes, index state and commit, not a dirty bit."""
    revision = git_bytes("rev-parse", "HEAD").decode().strip()
    scope = ["--", "."]
    names = git_bytes("ls-files", "--cached", "--others", "--exclude-standard", "-z", *scope)
    records = []
    for name in sorted(set(os.fsdecode(item) for item in names.split(b"\0") if item)):
        path = repo / name
        if not path.exists() and not path.is_symlink():
            records.append({"path": name, "type": "missing"})
        elif path.is_symlink():
            raise GateFailure("Source symlink cannot identify immutable build inputs: " + name)
        elif path.is_file():
            info = path.stat()
            records.append({"path": name, "type": "file", "sha256": digest(path),
                            "bytes": info.st_size, "mode": stat.S_IMODE(info.st_mode)})
        else:
            raise GateFailure("Unsupported source input: " + name)
    status = git_bytes("status", "--porcelain=v1", "-z", "--untracked-files=all", *scope)
    if git_bytes("rev-parse", "HEAD").decode().strip() != revision:
        raise GateFailure("Source commit changed while capturing its inventory")
    inventory_sha = hashlib.sha256(json.dumps(records, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"commit": revision, "dirty": bool(status), "status_sha256": hashlib.sha256(status).hexdigest(),
            "inventory_sha256": inventory_sha, "files": records}


def verify_executable(record):
    verify_file(record, "Compiled test executable")


def capture_test_executables(output):
    for line in output.splitlines():
        try:
            artifact = json.loads(line)
        except ValueError:
            continue
        if artifact.get("reason") != "compiler-artifact" or not artifact.get("executable"):
            continue
        target = artifact["target"]
        is_test = artifact.get("profile", {}).get("test", False)
        if not is_test and not (target["name"] == "mathlab" and "bin" in target["kind"]):
            continue
        path = Path(artifact["executable"]).absolute()
        if path.is_symlink() or not path.is_file():
            raise GateFailure("Cargo reported an absent or symlinked executable: " + str(path))
        report["test_executables"].append({**file_identity(path), "cargo_fresh": artifact.get("fresh"),
                                           "target": target["name"], "kind": target["kind"],
                                           "package_id": artifact["package_id"], "is_test": is_test})
    if not report["test_executables"]:
        raise GateFailure("Cargo --no-run JSON did not identify compiled test executables")
    cli_binaries = [record for record in report["test_executables"]
                    if record["target"] == "mathlab" and "bin" in record["kind"] and not record["is_test"]]
    if len(cli_binaries) != 1:
        raise GateFailure("Cargo JSON must identify the normal CLI binary used by CARGO_BIN_EXE_mathlab")


def command(label, argv, timeout):
    """Bound the complete process group, including native child interpreters."""
    logfile = report_dir / (label + ".log")
    entry = {"label": label, "argv": argv, "timeout_seconds": timeout, "log": logfile.name}
    report["commands"].append(entry)
    print("RUN " + label + ": " + " ".join(argv), flush=True)
    started = time.monotonic()
    with logfile.open("w") as output:
        process = subprocess.Popen(argv, cwd=repo, env=environment, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            entry["exit_code"] = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            entry["timed_out"] = True
            entry["exit_code"] = process.returncode
    entry["elapsed_seconds"] = round(time.monotonic() - started, 3)
    entry["sha256"] = digest(logfile)
    output = logfile.read_text(errors="replace")
    print(output, end="" if output.endswith("\n") else "\n", flush=True)
    if entry.get("timed_out"):
        raise GateFailure(label + " timed out")
    if entry["exit_code"] != 0:
        raise GateFailure(label + " failed (see " + str(logfile) + ")")
    return output.strip()


def executable(setting, default):
    selected = environment.get(setting) or default
    found = shutil.which(selected)
    # Keep argv[0] intact: Cargo commonly symlinks to the rustup dispatcher.
    return os.path.abspath(found) if found else None


def suite(label, package, target_args, names, prefix=None):
    target = target_args[1] if target_args[0] == "--test" else package.replace("-", "_")
    kind = "test" if target_args[0] == "--test" else "lib"
    matches = [record for record in report["test_executables"]
               if record["target"] == target and kind in record["kind"] and record["is_test"]]
    if len(matches) != 1:
        raise GateFailure("Cargo JSON did not identify exactly one test executable for " + label)
    binary = matches[0]
    verify_executable(binary)
    # The CLI integration executable invokes CARGO_BIN_EXE_mathlab internally.
    for record in report["test_executables"]:
        if label == "cli-conformance" and record["target"] == "mathlab" and not record["is_test"]:
            verify_executable(record)
    if prefix:
        names = [prefix + name for name in names]
    result = {"label": label, "required_tests": names, "status": "failed", "executable_sha256": binary["sha256"]}
    report["suites"].append(result)
    argv = [binary["path"]]
    if len(names) == 1:
        argv.append(names[0])
    elif prefix:
        argv.append(prefix)
    argv.extend(["--nocapture", "--test-threads=1"])
    if len(names) == 1:
        argv.append("--exact")
    output = command(label, argv, args.test_timeout_seconds)
    verify_executable(binary)
    # Optional tests currently return Ok(()) when disabled. --nocapture exposes
    # their skip messages; require positive counts and named tests as well.
    if re.search(r"\bskip(?:ped|ping)?\b", output, re.IGNORECASE):
        raise GateFailure(label + " reported a skip; native proof was not enabled")
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", output)
    if len(summaries) != 1 or any(int(failed) or int(ignored) for _, failed, ignored in summaries):
        raise GateFailure(label + " has no clean libtest summary")
    passed = sum(int(count) for count, _, _ in summaries)
    if passed != len(names) or any("test " + name + " ..." not in output for name in names):
        raise GateFailure(label + " did not execute every required native test")
    result.update(status="passed", passed=passed)


def observe_worker(octave):
    cli = next(record for record in report["test_executables"]
               if record["target"] == "mathlab" and "bin" in record["kind"] and not record["is_test"])
    verify_executable(cli)
    verify_native_inputs()
    project = Path(tempfile.mkdtemp(prefix="worker-project-", dir=report_dir))
    output = command("worker-capabilities", [cli["path"], "--json", "--octave", octave,
                                             "--directory", str(project), "doctor"], args.test_timeout_seconds)
    verify_executable(cli)
    verify_native_inputs()
    observed = strict_json(output, "Built CLI doctor")
    if not isinstance(observed, dict) or observed.get("ready") is not True:
        raise GateFailure("Built CLI doctor did not report ready:true")
    capabilities = observed.get("worker_capabilities")
    if (not isinstance(capabilities, list) or any(not isinstance(item, str) for item in capabilities)
            or len(capabilities) != len(set(capabilities))
            or not set(REQUIRED_CAPABILITIES).issubset(capabilities)):
        raise GateFailure("Built CLI doctor must expose unique observed worker_capabilities including " + ", ".join(REQUIRED_CAPABILITIES))
    value_path = environment["MATHLAB_VALUE_BRIDGE"]
    if observed.get("value_adapter") != value_path:
        raise GateFailure("Built CLI doctor value_adapter must identify the fresh canonical value module")
    runtime = observed.get("runtime")
    if not isinstance(runtime, str) or not Path(runtime).is_absolute() or Path(runtime).resolve() != Path(octave).resolve():
        raise GateFailure("Built CLI doctor runtime must identify the selected absolute runtime")
    report["worker_observation"] = {"method": "built-cli-doctor", "ready": True, "runtime": runtime,
                                    "value_adapter": value_path, "worker_capabilities": capabilities,
                                    "module_sha256": report["artifacts"]["mathlab_value_bridge.oct"]["sha256"],
                                    "runtime_executable_sha256": report["runtime"]["executable_identity"]["sha256"],
                                    "cli_executable_sha256": cli["sha256"]}


try:
    report["source"] = source_snapshot()
    report["platform"] = {"system": platform.system(), "machine": platform.machine()}
    cargo = executable("MATHLAB_CARGO", "cargo")
    octave = executable("MATHLAB_OCTAVE", "octave-cli") or executable("MATHLAB_OCTAVE", "octave")
    default_mkoctfile = str(Path(octave).parent / "mkoctfile") if octave else "mkoctfile"
    mkoctfile = executable("MATHLAB_MKOCTFILE", default_mkoctfile)
    missing = [name for name, path in (("cargo", cargo), ("Octave", octave), ("matching mkoctfile", mkoctfile)) if path is None]
    if missing:
        report["reason"] = "Missing native prerequisites: " + ", ".join(missing)
        if args.allow_skip:
            report["status"] = "skipped"
            print("SKIP " + report["reason"])
        else:
            raise GateFailure(report["reason"])
    else:
        environment.update(MATHLAB_OCTAVE=octave, MATHLAB_MKOCTFILE=mkoctfile)
        # Capture before the first invocation; retain the selected symlink path
        # as well as its hashed referent, so retargeting cannot preserve a hash
        # for a different executable than the one subsequently invoked.
        report["runtime"] = {"executable": octave, "executable_identity": file_identity(Path(octave).resolve()),
                             "mkoctfile": mkoctfile, "mkoctfile_identity": file_identity(Path(mkoctfile).resolve())}
        runtime_version = command("runtime-version", [octave, "--no-gui", "--no-init-file", "--quiet", "--eval", "fprintf('%s\\n', version());"], 30)
        sdk_version = command("sdk-version", [mkoctfile, "-p", "OCTAVE_VERSION"], 30)
        report["runtime"].update(version=runtime_version, sdk_version=sdk_version)
        verify_runtime()
        if not re.fullmatch(r"\d+\.\d+\.\d+", runtime_version) or sdk_version != runtime_version:
            raise GateFailure("Octave runtime and mkoctfile SDK versions must agree exactly")
        expected = environment.get("MATHLAB_EXPECT_OCTAVE_VERSION")
        if expected and runtime_version != expected:
            raise GateFailure("Expected Octave " + expected + ", found " + runtime_version)
        report["rustc"] = command("rustc-version", ["rustc", "--version", "--verbose"], 30)
        report["compiler"] = command("native-compiler", [mkoctfile, "-p", "CXX"], 30)
        previous = {}
        for _, name in ADAPTERS:
            path = repo / "native" / name
            if path.exists() or path.is_symlink():
                previous[name] = file_identity(path)
        command("build-adapters", ["bash", "scripts/build-debugger.sh"], args.build_timeout_seconds)
        for setting, name in ADAPTERS:
            path = repo / "native" / name
            record = file_identity(path)
            if record["bytes"] == 0:
                raise GateFailure("Native build did not produce " + name)
            old = previous.get(name)
            if old and (old["device"], old["inode"]) == (record["device"], record["inode"]):
                raise GateFailure("Native build must freshly replace all four adapter inodes: " + name)
            environment[setting] = str(path.resolve())
            report["artifacts"][name] = {**record, "fresh_build": True, "previous_identity": old}
        recorded = (repo / "native/runtime-version.txt").read_text().strip()
        if recorded != runtime_version:
            raise GateFailure("Native build runtime-version.txt disagrees with selected runtime")
        native_directory = str((repo / "native").resolve()).replace("'", "''")
        marker = "MATHLAB_NATIVE_ABI_OK debugger=4 scope=2 input=1 value=1"
        protocol_prefix = "MATHLAB_VALUE_PROTOCOL_JSON="
        probe = (
            "addpath('" + native_directory + "','-begin'); "
            "d=mathlab_debug_bridge('protocol',0); s=mathlab_scope_bridge('protocol',0); i=mathlab_input_bridge('protocol',0); v=mathlab_value_bridge('protocol'); "
            "assert(strcmp(d.protocol,'mathlab-debugger') && d.version==4 && d.source_entry && d.stable_registry && d.off_path_registry && d.breakpoint_source_metadata && d.streamed_evaluation && d.input_wait_cancel); "
            "assert(strcmp(s.protocol,'mathlab-scope') && s.version==2 && s.pre_entry && s.original_source_reflection && s.source_entry && s.stable_registry && s.off_path_registry); "
            "assert(strcmp(i.protocol,'mathlab-input') && i.version==1 && i.original_builtin && i.caller_scope && i.separate_reply && i.genuine_reads); "
            "assert(strcmp(v.protocol,'mathlab-value') && v.version==1 && isequal(v.dense_complex_components,true) && v.max_cells==2000 && v.max_rank==32 && v.max_replace_leaf_bytes==134217728); "
            "fprintf('" + protocol_prefix + "%s\\n',jsonencode(v)); "
            "fprintf('" + marker + "\\n');"
        )
        verify_native_inputs()
        loaded = command("load-native-abi", [octave, "--no-gui", "--no-init-file", "--quiet", "--eval", probe], 30)
        verify_native_inputs()
        if marker not in loaded.splitlines():
            raise GateFailure("Native ABI probe did not confirm all four loaded protocols")
        protocol_lines = [line[len(protocol_prefix):] for line in loaded.splitlines() if line.startswith(protocol_prefix)]
        if len(protocol_lines) != 1:
            raise GateFailure("Native ABI probe must emit exactly one actual value protocol JSON object")
        value_protocol = strict_json(protocol_lines[0], "Value protocol")
        expected_protocol = {"protocol": "mathlab-value", "version": 1, "dense_complex_components": True,
                             "max_cells": 2000, "max_rank": 32, "max_replace_leaf_bytes": 134217728}
        if (not isinstance(value_protocol, dict) or set(value_protocol) != set(expected_protocol)
                or any(type(value_protocol[key]) is not type(value) or value_protocol[key] != value
                       for key, value in expected_protocol.items())):
            raise GateFailure("Actual value protocol must match the exact six-field typed contract and limits")
        report["protocols"] = {"mathlab-debugger": 4, "mathlab-scope": 2, "mathlab-input": 1, "mathlab-value": 1}
        report["value_protocol"] = value_protocol
        compiled = command("compile-tests", [cargo, "test", "--locked", "-p", "mathlab-engine", "-p", "mathlab-debugger", "-p", "mathlab-desktop", "-p", "mathlab-cli", "--no-run", "--message-format=json"], args.build_timeout_seconds)
        capture_test_executables(compiled)
        if source_snapshot() != report["source"]:
            raise GateFailure("Source snapshot changed during compilation")
        observe_worker(octave)
        report["native_enabled"] = True
        suite("debugger", "mathlab-debugger", ["--test", "octave_debugger"], [
            "real_breakpoint_scopes_step_and_continue",
            "guarded_debug_quit_preserves_assignments_and_allows_more_commands",
            "step_into_and_out_preserve_function_scopes",
            "nested_debug_evaluation_can_resume_and_restores_enclosing_pause"])
        for label, name in (
            ("scope", "optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears"),
            ("prepared-source", "optional_prepared_debugger_breaks_in_saved_and_unsaved_script_local_and_nested_sources"),
            ("parse-recovery", "optional_prepared_debugger_parse_error_preserves_workspace_and_breakpoints")):
            suite(label, "mathlab-engine", ["--test", "octave_session"], [name])
        suite("input", "mathlab-engine", ["--test", "input_session"], [
            "optional_input_preserves_caller_scope_unicode_echo_and_ordered_prompt_events",
            "optional_input_nested_expression_consumes_outer_line_before_inner_request",
            "optional_input_stop_and_restart_revoke_old_keys_without_cross_consuming_recovery",
            "optional_input_clear_and_no_output_calls_keep_the_original_builtin_contract",
            "optional_input_eof_preserves_original_error_and_next_request_remains_usable",
            "optional_input_caught_corrupt_reply_finishes_error_before_the_next_read",
            "optional_input_rejects_late_activation_after_old_builtin_handle_capture"])
        for label, name in (
            ("desktop-input", "input_replies_and_eof_bypass_the_busy_worker_and_stop_preserves_the_session"),
            ("desktop-paused-input", "paused_scope_input_is_answered_without_queuing_another_engine_request"),
            ("desktop-stop", "app_stop_interrupts_a_paused_evaluation_after_input_has_already_been_consumed")):
            suite(label, "mathlab-desktop", ["--lib"], ["worker::tests::" + name])
        suite("cli-conformance", "mathlab-cli", ["--test", "conformance"], [
            "actual_three_route_report_preserves_unavailable_and_source_preparation_boundaries",
            "synthetic_saved_references_are_rejected_without_publishing_a_reference_match",
            "observed_unavailable_reference_and_tampered_hashes_are_rejected",
            "unexpected_fixture_assertion_failure_is_identified_and_exits_unsuccessfully",
            "two_agreeing_successful_routes_cannot_pass_with_an_empty_typed_result",
            "known_sparse_logical_mat_gap_remains_a_failure_in_both_actual_routes",
            "missing_octave_is_explicitly_unavailable_and_never_a_match",
            "strict_incomplete_acceptance_fails_after_publishing_the_complete_report",
            "malformed_reference_case_sets_are_rejected_before_route_provenance",
            "original_core_smoke_runner_keeps_its_schema_and_unique_fixture_execution"])
        for specification in VALUE_SUITES:
            suite(*specification)
        if sum(len(item["required_tests"]) for item in report["suites"]) != report["required_test_count"]:
            raise GateFailure("Versioned contract did not execute its complete required proof set")
        report["status"] = "passed"
except (GateFailure, OSError, subprocess.SubprocessError) as error:
    report["reason"] = str(error)
    print("FAIL native gate: " + str(error), file=sys.stderr)
finally:
    report["executed_required_test_count"] = sum(len(item["required_tests"]) for item in report["suites"] if item["status"] == "passed")
    report["executed_test_count"] = sum(item.get("passed", 0) for item in report["suites"])
    try:
        if "source" in report:
            report["source_after"] = source_snapshot()
            report["source_unchanged"] = report["source_after"] == report["source"]
            if not report["source_unchanged"]:
                raise GateFailure("Source snapshot changed before gate completion")
        for record in report["test_executables"]:
            verify_executable(record)
        for name, record in report["artifacts"].items():
            verify_file(record, "Native artifact before gate completion")
        if "runtime" in report:
            verify_runtime()
    except (GateFailure, OSError, subprocess.SubprocessError) as error:
        report["status"] = "failed"
        report["reason"] = report.get("reason", "") + ("; " if report.get("reason") else "") + str(error)
        print("FAIL native gate identity: " + str(error), file=sys.stderr)
    (report_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Native gate " + report["status"] + ": " + str(report_dir / "report.json"))
sys.exit(0 if report["status"] in ("passed", "skipped") else 1)
PY
