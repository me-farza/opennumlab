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
import time

repo = Path(sys.argv[1]).resolve()
parser = argparse.ArgumentParser(description="Build matching Octave adapters and require enabled native integration proofs.")
parser.add_argument("--report-dir", type=Path, default=repo / "target/release-native-gate")
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
report = {"schema_version": 2, "status": "failed", "native_enabled": False,
          "allow_skip": args.allow_skip, "commands": [], "suites": [], "artifacts": {}, "test_executables": [],
          "release_candidate_qualification": False,
          "scope": "Selected source native and CLI conformance contracts; no installed-candidate, Core, Full or MATLAB parity qualification.",
          "limitations": ["CLI conformance intentionally exits zero for incomplete evidence. Final candidate acceptance must inspect every required route, oracle comparison and summary; this source CI gate does not accept an installed release."]}
environment = os.environ.copy()


class GateFailure(Exception):
    pass


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


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
    path = Path(record["path"])
    if (path.is_symlink() or not path.is_file() or digest(path) != record["sha256"]
            or path.stat().st_size != record["bytes"] or stat.S_IMODE(path.stat().st_mode) != record["mode"]):
        raise GateFailure("Compiled test executable changed: " + str(path))


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
        report["test_executables"].append({"path": str(path), "sha256": digest(path), "bytes": path.stat().st_size,
                                           "mode": stat.S_IMODE(path.stat().st_mode),
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


def suite(label, package, target_args, names):
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
    argv = [binary["path"]]
    if len(names) == 1:
        argv.append(names[0])
    argv.extend(["--nocapture", "--test-threads=1"])
    if len(names) == 1:
        argv.append("--exact")
    output = command(label, argv, args.test_timeout_seconds)
    verify_executable(binary)
    result = {"label": label, "required_tests": names, "status": "failed", "executable_sha256": binary["sha256"]}
    report["suites"].append(result)
    # Optional tests currently return Ok(()) when disabled. --nocapture exposes
    # their skip messages; require positive counts and named tests as well.
    if re.search(r"\bskip(?:ped|ping)?\b", output, re.IGNORECASE):
        raise GateFailure(label + " reported a skip; native proof was not enabled")
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", output)
    if not summaries or any(int(failed) or int(ignored) for _, failed, ignored in summaries):
        raise GateFailure(label + " has no clean libtest summary")
    passed = sum(int(count) for count, _, _ in summaries)
    if passed < len(names) or any("test " + name + " ..." not in output for name in names):
        raise GateFailure(label + " did not execute every required native test")
    result.update(status="passed", passed=passed)


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
        runtime_version = command("runtime-version", [octave, "--no-gui", "--no-init-file", "--quiet", "--eval", "fprintf('%s\\n', version());"], 30)
        sdk_version = command("sdk-version", [mkoctfile, "-p", "OCTAVE_VERSION"], 30)
        report["runtime"] = {"executable": octave, "version": runtime_version,
                             "mkoctfile": mkoctfile, "sdk_version": sdk_version}
        if not re.fullmatch(r"\d+\.\d+\.\d+", runtime_version) or sdk_version != runtime_version:
            raise GateFailure("Octave runtime and mkoctfile SDK versions must agree exactly")
        expected = environment.get("MATHLAB_EXPECT_OCTAVE_VERSION")
        if expected and runtime_version != expected:
            raise GateFailure("Expected Octave " + expected + ", found " + runtime_version)
        report["rustc"] = command("rustc-version", ["rustc", "--version", "--verbose"], 30)
        report["compiler"] = command("native-compiler", [mkoctfile, "-p", "CXX"], 30)
        command("build-adapters", ["bash", "scripts/build-debugger.sh"], args.build_timeout_seconds)
        for setting, name in (("MATHLAB_DEBUG_BRIDGE", "mathlab_debug_bridge.oct"),
                              ("MATHLAB_SCOPE_BRIDGE", "mathlab_scope_bridge.oct"),
                              ("MATHLAB_INPUT_BRIDGE", "mathlab_input_bridge.oct")):
            path = repo / "native" / name
            if not path.is_file() or path.stat().st_size == 0:
                raise GateFailure("Native build did not produce " + name)
            environment[setting] = str(path.resolve())
            report["artifacts"][name] = {"sha256": digest(path), "bytes": path.stat().st_size}
        recorded = (repo / "native/runtime-version.txt").read_text().strip()
        if recorded != runtime_version:
            raise GateFailure("Native build runtime-version.txt disagrees with selected runtime")
        native_directory = str((repo / "native").resolve()).replace("'", "''")
        marker = "MATHLAB_NATIVE_ABI_OK debugger=4 scope=2 input=1"
        probe = (
            "addpath('" + native_directory + "','-begin'); "
            "d=mathlab_debug_bridge('protocol',0); s=mathlab_scope_bridge('protocol',0); i=mathlab_input_bridge('protocol',0); "
            "assert(strcmp(d.protocol,'mathlab-debugger') && d.version==4 && d.source_entry && d.stable_registry && d.off_path_registry && d.breakpoint_source_metadata && d.streamed_evaluation && d.input_wait_cancel); "
            "assert(strcmp(s.protocol,'mathlab-scope') && s.version==2 && s.pre_entry && s.original_source_reflection && s.source_entry && s.stable_registry && s.off_path_registry); "
            "assert(strcmp(i.protocol,'mathlab-input') && i.version==1 && i.original_builtin && i.caller_scope && i.separate_reply && i.genuine_reads); "
            "fprintf('" + marker + "\\n');"
        )
        loaded = command("load-native-abi", [octave, "--no-gui", "--no-init-file", "--quiet", "--eval", probe], 30)
        if marker not in loaded.splitlines():
            raise GateFailure("Native ABI probe did not confirm all three loaded protocols")
        report["protocols"] = {"mathlab-debugger": 4, "mathlab-scope": 2, "mathlab-input": 1}
        report["native_enabled"] = True
        compiled = command("compile-tests", [cargo, "test", "--locked", "-p", "mathlab-engine", "-p", "mathlab-debugger", "-p", "mathlab-desktop", "-p", "mathlab-cli", "--no-run", "--message-format=json"], args.build_timeout_seconds)
        capture_test_executables(compiled)
        if source_snapshot() != report["source"]:
            raise GateFailure("Source snapshot changed during compilation")
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
        report["status"] = "passed"
except (GateFailure, OSError, subprocess.SubprocessError) as error:
    report["reason"] = str(error)
    print("FAIL native gate: " + str(error), file=sys.stderr)
finally:
    try:
        if "source" in report:
            report["source_after"] = source_snapshot()
            report["source_unchanged"] = report["source_after"] == report["source"]
            if not report["source_unchanged"]:
                raise GateFailure("Source snapshot changed before gate completion")
        for record in report["test_executables"]:
            verify_executable(record)
        report["required_test_count"] = sum(len(suite["required_tests"]) for suite in report["suites"])
        for name, record in report["artifacts"].items():
            path = repo / "native" / name
            if path.is_symlink() or not path.is_file() or digest(path) != record["sha256"]:
                raise GateFailure("Native artifact changed before gate completion: " + name)
    except (GateFailure, OSError, subprocess.SubprocessError) as error:
        report["status"] = "failed"
        report["reason"] = report.get("reason", "") + ("; " if report.get("reason") else "") + str(error)
        print("FAIL native gate identity: " + str(error), file=sys.stderr)
    (report_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Native gate " + report["status"] + ": " + str(report_dir / "report.json"))
sys.exit(0 if report["status"] in ("passed", "skipped") else 1)
PY
