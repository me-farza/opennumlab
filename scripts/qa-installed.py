#!/usr/bin/env python3
"""Inspect and exercise an installed CLI; this does not qualify a whole release.

Uses only the packaged CLI, an external Octave selected by that CLI, and owned
temporary projects. No Cargo, MATLAB, desktop, launcher or installer is invoked.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import selectors
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zlib


SOURCE_ROOT = Path(__file__).resolve().parent.parent
HEX256 = re.compile(r"[0-9a-f]{64}\Z")
CLI_PATH = "Contents/MacOS/mathlab"
DESKTOP_PATH = "Contents/MacOS/mathlab-desktop"
RELEASE_PATH = "Contents/Resources/release.json"
CHECKSUM_PATH = "Contents/Resources/checksums.sha256"
STARTUP_CONFIG_PATH = "Contents/Resources/qualified-core-v2.json"
NATIVE_NAMES = ("mathlab_debug_bridge.oct", "mathlab_scope_bridge.oct",
                "mathlab_input_bridge.oct", "runtime-version.txt")
VALUE_NAME = "mathlab_value_bridge.oct"
VALUE_PATH = "Contents/Resources/native/" + VALUE_NAME
VALUE_CAPABILITY = "dense_complex_native_components.v1"
VALUE_REQUIRED_CAPABILITIES = {"typed_nd_page.v1", "atomic_range_edit.v1", VALUE_CAPABILITY}
VALUE_PROTOCOL = {"protocol": "mathlab-value", "version": 1, "dense_complex_components": True,
                  "max_cells": 2000, "max_rank": 32, "max_replace_leaf_bytes": 134217728}
REQUIRED_ADAPTER_PROFILE = {"schema_version": 1, "name": "four-adapter-value-v1"}
QUALIFIED_ADAPTER_PROFILE = {"schema_version": 1, "name": "qualified-core-v2"}
GRAPHICS_ADAPTER_PROFILE = {"schema_version": 1, "name": "qualified-graphics-v2"}
QUALIFIED_PROFILES = (QUALIFIED_ADAPTER_PROFILE, GRAPHICS_ADAPTER_PROFILE)
INTRINSIC_NAME = "mathlab_intrinsic_bridge.oct"
GRAPHICS_NAME = "mathlab_graphics_bridge.oct"
GRAPHICS_CAPABILITY = "graphics_registry.v1"
QUALIFIED_MODULE_NAMES = {name for name in NATIVE_NAMES if name.endswith(".oct")} | {VALUE_NAME, INTRINSIC_NAME}
GRAPHICS_MODULE_NAMES = QUALIFIED_MODULE_NAMES | {GRAPHICS_NAME}
GRAPHICS_PROTOCOL = {"protocol": "mathlab-graphics", "version": 1, "octave_version": "11.3.0",
    "octave_api_version": "api-v61", "native_object_lifetime": True,
    "bound_primitive_dispatcher": True, "max_entries": 4096}
INTRINSIC_PROTOCOL = {"protocol": "mathlab-intrinsics", "version": 1,
    "octave_version": "11.3.0", "octave_api_version": "api-v61",
    "actual_function_object": True, "native_base_queries": True, "unlocked_registry": True,
    "callback_binding_v2": True, "adapter_carrier_v2": True, "provider_shutdown_proxy": True}
OMITTED_ADAPTER_PROFILE = {"schema_version": 1, "name": "native-omitted-v1"}
OMITTED_VALUE_COMPONENT = {"status": "not_included", "reason": "native_profile_not_requested"}
VALUE_PROBE_REQUIRED_CASES = {"runtime_profile", "uint64_checked_read_write_undo_redo",
    "double_checked_read_write_undo_redo", "single_checked_read_write_undo_redo", "stale_identity_rejected",
    "restart_preserves_native_profile", "same_process_stop_recovers"}
VALUE_PROBE_UNAVAILABLE_CASES = {"runtime_profile", "uint64_checked_read_write_undo_redo",
    "complex_read_requires_native", "complex_write_requires_native"}
PROBE_ADAPTERS = {"value_adapter": ("MATHLAB_VALUE_BRIDGE", "mathlab_value_bridge.oct"),
                  "scope_adapter": ("MATHLAB_SCOPE_BRIDGE", "mathlab_scope_bridge.oct"),
                  "input_adapter": ("MATHLAB_INPUT_BRIDGE", "mathlab_input_bridge.oct"),
                  "debug_adapter": ("MATHLAB_DEBUG_BRIDGE", "mathlab_debug_bridge.oct")}
CHECKS = ("bundle_identity", "cli_version", "doctor", "missing_runtime_diagnostic",
          "numerical_original_script", "original_source_error", "persistent_repl",
          "graphics_png", "getting_started_example", "mat_self_roundtrip",
          "authentic_input", "packaged_value_api", "bundle_unchanged")
PENDING = (
    ("desktop_ui", "Actual desktop editing, variable inspection and figure interaction were not exercised."),
    ("stop_and_recovery", "No Stop, interruption or desktop recovery journey was exercised."),
    ("debugger", "No breakpoint, stepping, paused evaluation or debugger recovery was exercised."),
    ("packaged_value_read_edit_history", "Doctor only establishes adapter prerequisites; packaged checked reads, edits, Undo and Redo were not exercised."),
    ("packaged_portable_uint64_read_edit_history", "The packaged exact uint64 checked read/write/Undo/Redo probe was not exercised."),
    ("read_only_resources_and_relocation", "Bundle bytes were checked for changes; read-only installation and relocation were not exercised."),
    ("clean_machine_installation", "This uses an existing external runtime and does not establish clean-machine installation or dependency provisioning."),
    ("lifecycle", "Install, upgrade, rollback and uninstall were not exercised."),
)
NUMERICS = """% Original installed-CLI acceptance source, authored for this harness.
qa_A=[3 1;1 2]; qa_b=[9;8]; qa_x=qa_A\\qa_b;
qa_residual=norm(qa_A*qa_x-qa_b);
assert(norm(qa_x-[2;3],Inf)<1e-12 && qa_residual<1e-12);
assert(isequal(reshape(1:6,2,3),[1 3 5;2 4 6]));
qa_local_answer=qa_local(6); assert(qa_local_answer==42);
fprintf('NUMJINN_QA_NUMERICS %.17g %.17g %.17g %d\\n',qa_x(1),qa_x(2),qa_residual,qa_local_answer);
function value=qa_local(value)
  value=7*value;
end
"""
SOURCE_ERROR = "qa_before_error=7;\nerror('NumJinnQA:OriginalSource','NUMJINN_QA_ORIGINAL_SOURCE_ERROR');\n"
GRAPHICS = """qa_plot_t=[0 .5 1]; qa_plot_y=[0 .25 1];
qa_figure=figure('visible','off'); qa_line=plot(qa_plot_t,qa_plot_y);
assert(isequal(get(qa_line,'xdata'),qa_plot_t));
assert(isequal(get(qa_line,'ydata'),qa_plot_y));
fprintf('NUMJINN_QA_PLOT_DATA 0,0.5,1;0,0.25,1\\n');
fprintf('NUMJINN_QA_GSC %s\\n',getenv('GSC'));
"""
INPUT = """qa_name=input('NUMJINN_QA_NAME> ','s');
qa_base=40; qa_value=input('NUMJINN_QA_VALUE> ');
qa_empty=input('NUMJINN_QA_EMPTY> ','s');
assert(strcmp(qa_name,'  λ exact  ')); assert(qa_value==42); assert(isempty(qa_empty));
fprintf('NUMJINN_QA_INPUT_OK 42\\n');
"""
REPL = ("qa_session_seed=41; fprintf('NUMJINN_QA_REPL_FIRST %.0f\\n',qa_session_seed);\n"
        "qa_session_seed=qa_session_seed+1; assert(qa_session_seed==42); fprintf('NUMJINN_QA_REPL_SECOND %.0f\\n',qa_session_seed);\n"
        ":workspace\n:quit\n")


class QAFailure(Exception):
    pass


def require(condition, message):
    if not condition:
        raise QAFailure(message)


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def inside(path, parent):
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def relative_name(value):
    require(isinstance(value, str) and value and not any(ord(c) < 32 for c in value),
            "Invalid manifest path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and ".." not in path.parts and path.as_posix() == value
            and "\\" not in value, "Unsafe or noncanonical manifest path: " + value)
    return value


def read_json(path, maximum=32 * 1024 * 1024):
    require(path.is_file() and path.stat().st_size <= maximum, "Missing or oversized JSON: " + str(path))
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object,
                          parse_constant=invalid_json_constant)
    except (ValueError, UnicodeError) as error:
        raise QAFailure("Invalid JSON in " + str(path)) from error


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON field: " + key)
        result[key] = value
    return result


def invalid_json_constant(value):
    raise QAFailure("Non-JSON numeric constant: " + value)


def valid_sha256(value):
    return isinstance(value, str) and HEX256.fullmatch(value) is not None


def capability_names(value):
    require(isinstance(value, list) and all(isinstance(name, str) for name in value)
            and len(set(value)) == len(value), "Worker capabilities must be unique strings")
    return set(value)


def observed_absolute_path(value, basename=None):
    if not isinstance(value, str) or not value or any(ord(c) < 32 for c in value):
        return False
    path = Path(value)
    return (path.is_absolute() and ".." not in path.parts and str(path) == value
            and (basename is None or path.name == basename))


def probe_artifact_snapshot(path, expected_hash):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and not path.is_symlink() and path.is_file(),
            "Probe artifact is not a canonical regular installed file: " + str(path))
    info = path.stat()
    identity = {"path": str(path), "sha256": digest(path), "size": info.st_size,
                "mode": info.st_mode & 0o7777, "device": info.st_dev, "inode": info.st_ino}
    require(valid_sha256(expected_hash) and identity["sha256"] == expected_hash,
            "Probe artifact differs from its independently accepted identity: " + str(path))
    return identity


def require_path_observation(value, expected):
    require(isinstance(value, dict) and value.get("display") == expected and value.get("absolute") is True
            and value.get("path_utf8") is True and value.get("display_is_lossy") is False,
            "Probe path observation is missing, lossy or inconsistent")


def validate_value_probe(report, required, identities, version, project_root, qualified=False, graphics=False):
    """Consume the versioned CLI report without inferring execution from doctor."""
    require(not graphics or (qualified and required), "Graphics probe requires its explicit qualified profile")
    qualified_kinds = set(PROBE_ADAPTERS) | {"intrinsic_provider"} | ({"graphics_adapter"} if graphics else set())
    if qualified:
        require(set(identities) == {"cli_executable", "octave_runtime"} | qualified_kinds,
                "Qualified probe needs exactly its selected seven/eight artifact bindings")
    profile = "native-required" if required else "native-unavailable"
    require(isinstance(report, dict) and type(report.get("schema_version")) is int
            and report["schema_version"] == 1 and report.get("kind") == "mathlab.value-api-probe"
            and report.get("profile") == profile and report.get("status") == "passed"
            and "error" in report and report["error"] is None, "Value API probe did not report the required successful schema/profile")
    build = report.get("build")
    cli, runtime = identities["cli_executable"], identities["octave_runtime"]
    require(isinstance(build, dict) and build.get("package_version") == version
            and build.get("executable") == cli["path"] and build.get("binary_sha256") == cli["sha256"]
            and build.get("executable_identity_verified") is True and build.get("binary_identity_verified") is True
            and "executable_identity_error" in build and build["executable_identity_error"] is None
            and "binary_identity_error" in build and build["binary_identity_error"] is None
            and valid_sha256(build.get("source_sha256"))
            and all(isinstance(build.get(key), str) and build[key] for key in ("revision", "target")),
            "Value API probe CLI build/byte identity disagrees with the installed executable")
    require_path_observation(build.get("executable_observation"), cli["path"])
    require(report.get("runtime") == runtime["path"], "Value API probe used a different runtime")
    details = report.get("runtime_details")
    require(isinstance(details, dict) and details.get("canonical_identity_verified") is True
            and details.get("started") is True and details.get("temporary_workspace") is True
            and type(details.get("process_id")) is int and details["process_id"] > 0
            and isinstance(details.get("version"), str) and details["version"],
            "Value API probe did not establish a started isolated runtime")
    require_path_observation(details.get("requested_runtime"), runtime["path"])
    workspace = details.get("working_directory")
    require(observed_absolute_path(workspace) and inside(Path(workspace), project_root)
            and Path(workspace) != project_root / "project", "Value API probe workspace is outside the owned temporary root")
    require_path_observation(details.get("working_directory_observation"), workspace)
    capabilities = capability_names(report.get("worker_capabilities"))
    require({"typed_nd_page.v1", "atomic_range_edit.v1"}.issubset(capabilities), "Value API probe lacks negotiated checked-value capabilities")
    protocol = report.get("worker_protocol")
    require(isinstance(protocol, dict) and protocol.get("name") == "mathlab-worker"
            and type(protocol.get("version")) is int and protocol["version"] == 1
            and capability_names(protocol.get("capabilities")) == capabilities,
            "Value API probe worker protocol/capabilities disagree")
    adapter = report.get("adapter_details")
    value_path = identities["value_adapter"]["path"] if required else None
    require(isinstance(adapter, dict) and report.get("value_adapter") == value_path
            and "value_adapter" in report and adapter.get("path") == value_path
            and adapter.get("configured_startup_path" if qualified else "configured_environment_path") == value_path
            and adapter.get("capability") == VALUE_CAPABILITY and adapter.get("capability_observed") is required
            and (VALUE_CAPABILITY in capabilities) is required, "Value API probe native path/capability disagrees with its installed profile")
    if required:
        require_path_observation(adapter.get("path_observation"), value_path)
        require_path_observation(adapter.get("configured_startup_observation" if qualified else "configured_environment_observation"), value_path)
        if qualified:
            require(adapter.get("configured_environment_path") is None
                    and adapter.get("configured_environment_observation") is None,
                    "Qualified probe must bind its configured value path through startup selection")
    else:
        require(all(key in adapter and adapter[key] is None for key in
                ("path", "configured_environment_path", "path_observation", "configured_environment_observation")),
                "Unavailable value probe retained a configured native path")
    bindings = report.get("artifact_identities")
    require(isinstance(bindings, list) and len(bindings) == len(identities), "Value API probe artifact binding set is incomplete")
    by_kind = {}
    for binding in bindings:
        require(isinstance(binding, dict) and isinstance(binding.get("kind"), str)
                and binding["kind"] not in by_kind, "Value API probe has malformed or duplicate artifact bindings")
        by_kind[binding["kind"]] = binding
    require(set(by_kind) == set(identities), "Value API probe artifact binding set differs from selected installed inputs")
    for kind, identity in identities.items():
        binding = by_kind[kind]
        configured_by = ("startup_config" if qualified and kind in qualified_kinds
                         else PROBE_ADAPTERS[kind][0] if kind in PROBE_ADAPTERS else None)
        require(binding.get("path") == identity["path"] and binding.get("sha256_before") == identity["sha256"]
                and binding.get("sha256_after") == identity["sha256"] and binding.get("unchanged") is True
                and "error" in binding and binding["error"] is None
                and "configured_by" in binding and binding["configured_by"] == configured_by,
                "Value API probe artifact endpoint identity disagrees: " + kind)
    preflight = report.get("adapter_preflight")
    require(isinstance(preflight, list) and len(preflight) == (4 if required and not qualified else 0),
            "Value API probe adapter admission set differs from selected profile")
    by_variable = {}
    for item in preflight:
        require(isinstance(item, dict) and isinstance(item.get("environment_variable"), str)
                and item["environment_variable"] not in by_variable, "Value API probe has duplicate or malformed adapter admissions")
        by_variable[item["environment_variable"]] = item
    if required and not qualified:
        require(set(by_variable) == {item[0] for item in PROBE_ADAPTERS.values()}, "Value API probe omitted a selected installed adapter")
        for kind, (variable, _) in PROBE_ADAPTERS.items():
            item = by_variable[variable]
            require(item.get("identity_verified") is True and item.get("abi_or_protocol_qualified") is False
                    and item.get("canonical_path") == identities[kind]["path"], "Value API probe adapter admission identity disagrees")
            require_path_observation(item.get("requested_path"), identities[kind]["path"])
    if qualified:
        expected_modules = {Path(identity["path"]).name: {"path": identity["path"], "sha256": identity["sha256"]}
                            for kind, identity in identities.items() if kind in qualified_kinds}
        require(report.get("native_modules") == expected_modules, "Qualified probe module selection differs from installed bindings")
        validate_qualified_observation(report, {name: item["sha256"] for name, item in expected_modules.items()}, graphics=graphics)
    else:
        reject_qualified_observation(report)
    scope = report.get("scope")
    require(isinstance(scope, dict) and scope.get("public_engine_api_only") is True
            and scope.get("owned_temporary_workspace") is True and scope.get("native_precision_requested") is required
            and scope.get("artifact_hashes_are_endpoint_identity_not_abi_or_load_proof") is True,
            "Value API probe scope does not describe the selected public API route")
    limits = report.get("limits")
    limit_names = ("page_cells", "page_result_bytes", "edit_cells", "edit_request_bytes", "edit_receipt_bytes",
                   "history_records", "history_logical_patch_bytes", "fixture_cells_per_complex_leaf",
                   "fixture_execute_timeout_seconds", "checked_api_timeout_seconds", "stop_barrier_timeout_seconds",
                   "stop_settle_timeout_seconds")
    require(isinstance(limits, dict) and all(type(limits.get(key)) is int and limits[key] > 0 for key in limit_names)
            and isinstance(limits.get("meaning"), str) and limits["meaning"], "Value API probe limit observations are missing or malformed")
    cases = report.get("cases")
    require(isinstance(cases, list), "Value API probe has no case list")
    by_id = {}
    for case in cases:
        require(isinstance(case, dict) and set(case) == {"id", "status", "error_identifier", "details"}
                and isinstance(case["id"], str) and case["id"] not in by_id and case["status"] == "passed"
                and isinstance(case["details"], dict), "Value API probe has a duplicate, failed, skipped or malformed case")
        by_id[case["id"]] = case
    require(set(by_id) == (VALUE_PROBE_REQUIRED_CASES if required else VALUE_PROBE_UNAVAILABLE_CASES),
            "Value API probe did not execute exactly the required case set")
    expected_errors = {} if required else {"complex_read_requires_native": "MathLab:ValuePagePrecisionUnavailable",
                                           "complex_write_requires_native": "MathLab:ValueEditPrecisionUnavailable"}
    for case_id, case in by_id.items():
        require(case["error_identifier"] == expected_errors.get(case_id), "Value API probe case identifier differs: " + case_id)
    require(by_id["runtime_profile"]["details"].get("adapter") == adapter, "Runtime profile case differs from observed native support")
    integer = by_id["uint64_checked_read_write_undo_redo"]["details"]
    require(integer.get("class") == "uint64" and integer.get("journey") == "read-write-undo-redo"
            and integer.get("native_adapter_required") is False and integer.get("exact_terminal_integer_oracle") is True
            and integer.get("original_alias_preserved") is True and integer.get("untouched_cell_and_peer_preserved") is True
            and integer.get("old_values") == ["9007199254740993", "18446744073709551615", "17"]
            and integer.get("new_values") == ["18446744073709551615", "9007199254740995", "17"],
            "Value API probe did not retain its real exact uint64 read/write/history oracle")
    if required:
        for precision in ("double", "single"):
            detail = by_id[precision + "_checked_read_write_undo_redo"]["details"]
            journeys = detail.get("journeys")
            require(detail.get("class") == precision and isinstance(journeys, list) and len(journeys) == 6,
                    "Native precision case omitted an independent history journey")
            actual = []
            for journey in journeys:
                require(isinstance(journey, dict) and journey.get("exact_final_bit_oracle") is True
                        and type(journey.get("selected_rows")) is int and type(journey.get("selected_columns")) is int
                        and journey["selected_columns"] == 1 and isinstance(journey.get("journey"), str),
                        "Native precision journey lacks its exact final bit oracle")
                actual.append((journey["selected_rows"], journey["journey"]))
            require(len(set(actual)) == 6 and set(actual) == {(rows, name) for rows in (1, 2)
                    for name in ("write-only", "write-undo", "write-undo-redo")}, "Native precision history journeys are incomplete or duplicated")
        stale = by_id["stale_identity_rejected"]["details"]
        require(all(stale.get(key) == "MathLab:StaleBinding" for key in
                ("stale_page_error_identifier", "stale_write_error_identifier", "stale_binding_history_error_identifier"))
                and stale.get("retired_history_error_identifier") == "MathLab:ValueEditHistoryConflict"
                and stale.get("automatic_retry_performed_by_probe") is False, "Probe did not preserve stale/history rejection outcomes")
        restart = by_id["restart_preserves_native_profile"]["details"]
        require(restart.get("adapter") == adapter and restart.get("profile_reinstalled") is True
                and restart.get("workspace_cleared") is True and type(restart.get("fresh_workspace_variable_count")) is int
                and restart["fresh_workspace_variable_count"] == 0
                and restart.get("old_token_error_identifier") == "MathLab:StaleBinding", "Probe did not retain the checked restart outcome")
        stop = by_id["same_process_stop_recovers"]["details"]
        require(stop.get("adapter") == adapter and stop.get("observed_stop_identifier") == "MathLab:Interrupted"
                and type(stop.get("source_start_count")) is int and stop["source_start_count"] == 1
                and stop.get("continuation_absent") is True and stop.get("old_authority_rejected") is True
                and stop.get("commit_window_test") is False, "Probe did not retain its ordinary same-process Stop outcome")
    else:
        for case_id, identifier in expected_errors.items():
            detail = by_id[case_id]["details"]
            require(detail.get("expected_error_identifier") == identifier and detail.get("observed_error_identifier") == identifier
                    and detail.get("native_precision_success") is False, "Expected native-unavailable rejection was counted as precision success")
        require(by_id["complex_write_requires_native"]["details"].get("unchanged_values") is True
                and by_id["complex_write_requires_native"]["details"].get("exact_final_component_bits") is True,
                "Unavailable complex write did not preserve its original values")
    return {"profile": profile, "cases": sorted(by_id), "uint64_read_write_undo_redo": "passed",
            "native_complex_read_write_undo_redo": "passed" if required else "unavailable_expected_rejections_passed",
            "expected_unavailable_errors": expected_errors, "worker_capabilities": sorted(capabilities),
            "build": build, "runtime_details": details, "artifact_identities": bindings, "reported_limits": limits,
            "scope": "Selected packaged CLI public checked-value API cases; no GUI, clipboard, sparse precision, commit-window fault or general parity qualification"}


def qualified_text(value, maximum):
    if not isinstance(value, str) or not value:
        return False
    try:
        return (len(value.encode("utf-8")) <= maximum
                and not any(ord(character) < 32 or ord(character) == 127 for character in value))
    except UnicodeError:
        return False


def qualified_context(value):
    require(isinstance(value, dict) and set(value) == {"worker_id", "generation", "invocation", "revision"}
            and qualified_text(value["worker_id"], 128), "Invalid qualified session context")
    for key in ("generation", "invocation", "revision"):
        number = value[key]
        require(isinstance(number, str) and re.fullmatch(r"0|[1-9][0-9]{0,19}", number)
                and int(number) <= 18446744073709551615, "Invalid qualified uint64 context: " + key)


def qualified_path(value, basename=None):
    return qualified_text(value, 65536) and observed_absolute_path(value, basename)


def validate_qualified_observation(observation, artifacts, installed=None, graphics=False):
    """Bind accepted Ready to selected bytes; build paths need not still exist."""
    modules = observation.get("native_modules")
    require(isinstance(modules, dict) and set(modules) == (GRAPHICS_MODULE_NAMES if graphics else QUALIFIED_MODULE_NAMES),
            "Qualified observation needs exactly its selected five/six native module identities")
    for name, identity in modules.items():
        require(isinstance(identity, dict) and set(identity) == {"path", "sha256"}
                and qualified_path(identity["path"], name) and valid_sha256(identity["sha256"])
                and identity["sha256"] == artifacts[name], "Qualified native module identity differs: " + name)
        if installed is not None:
            path = installed / "Contents/Resources/native" / name
            require(identity["path"] == str(path), "Qualified doctor used a foreign installed module: " + name)
            probe_artifact_snapshot(path, identity["sha256"])
    status = observation.get("qualified_startup_status")
    require(isinstance(status, dict) and set(status) == {"profile", "context", "intrinsics"}
            and status["profile"] == (GRAPHICS_ADAPTER_PROFILE if graphics else QUALIFIED_ADAPTER_PROFILE)["name"],
            "Missing accepted qualified startup status")
    capabilities = capability_names(observation.get("worker_capabilities"))
    require((GRAPHICS_CAPABILITY in capabilities) == graphics,
            "Qualified graphics capability differs from the selected profile")
    qualified_context(status["context"])
    qualified_context(observation.get("session_context"))
    require(status["context"] == observation["session_context"], "Qualified startup/session contexts differ")
    intrinsic = status["intrinsics"]
    require(isinstance(intrinsic, dict) and set(intrinsic) == {
        "protocol", "version", "phase", "provider_file", "worker_file", "registry_file", "entry_alias",
        "octave_version", "octave_api_version", "initial_path_admitted", "shutdown_alias",
        "shutdown_registered", "adapters"}, "Malformed accepted intrinsic identity")
    require(intrinsic["protocol"] == "mathlab-intrinsics" and intrinsic["version"] == "1"
            and intrinsic["phase"] == "ready" and intrinsic["octave_version"] == "11.3.0"
            and intrinsic["octave_api_version"] == "api-v61" and intrinsic["initial_path_admitted"] is True
            and intrinsic["shutdown_registered"] is True, "Qualified intrinsic admission/runtime identity differs")
    require(intrinsic["provider_file"] == modules[INTRINSIC_NAME]["path"]
            and qualified_path(intrinsic["worker_file"]) and qualified_path(intrinsic["registry_file"])
            and intrinsic["worker_file"] != intrinsic["registry_file"],
            "Qualified provider/owned worker paths differ")
    for key, prefix in (("entry_alias", "__mlentry_"), ("shutdown_alias", "__mlshutdown_")):
        alias = intrinsic[key]
        require(qualified_text(alias, 128) and re.fullmatch(re.escape(prefix) + r"[A-Za-z0-9_]+", alias),
                "Invalid qualified intrinsic alias: " + key)
    require(intrinsic["entry_alias"][len("__mlentry_"):] == intrinsic["shutdown_alias"][len("__mlshutdown_"):],
            "Qualified intrinsic shutdown alias differs from entry")
    adapters = intrinsic["adapters"]
    ready_protocols = (("input", "mathlab-input", "1"), ("scope", "mathlab-scope", "2"),
                       ("value", "mathlab-value", "1"), ("debug", "mathlab-debugger", "4"))
    if graphics:
        ready_protocols += (("graphics", "mathlab-graphics", "1"),)
    require(isinstance(adapters, dict) and set(adapters) == {item[0] for item in ready_protocols},
            "Qualified startup needs exactly its selected admitted adapter roles")
    for role, protocol, version in ready_protocols:
        require(adapters[role] == {"path": modules["mathlab_" + role + "_bridge.oct"]["path"],
                                  "protocol": protocol, "version": version},
                "Qualified startup adapter identity differs: " + role)
    require(observation.get("value_adapter") == modules[VALUE_NAME]["path"],
            "Qualified observation value adapter differs from admitted module")


def reject_qualified_observation(value):
    require(value.get("qualified_startup_status") is None and value.get("intrinsics") is None,
            "Non-qualified profile unexpectedly claimed intrinsic startup status")
    modules = value.get("native_modules")
    require(modules is None or (isinstance(modules, dict)
            and not any(str(name).lower() in {INTRINSIC_NAME, GRAPHICS_NAME} for name in modules)),
            "Non-qualified profile unexpectedly declared a qualified native module")


def qualified_ghostscript_snapshot(pin):
    require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}
            and qualified_path(pin["path"]) and re.fullmatch(r"[A-Za-z0-9/_+.-]+", pin["path"])
            and valid_sha256(pin["sha256"]),
            "Invalid qualified Ghostscript pin")
    identity = probe_artifact_snapshot(Path(pin["path"]), pin["sha256"])
    require(os.access(pin["path"], os.X_OK), "Qualified Ghostscript is not executable")
    return identity


def read_qualified_startup(path, native):
    """Validate the explicit schema-2 selection without rebinding external pins."""
    document = read_json(path, maximum=1024 * 1024)
    profile = native.get("adapter_profile")
    require(profile in QUALIFIED_PROFILES, "Startup selection requires an explicit qualified native profile")
    graphics = profile == GRAPHICS_ADAPTER_PROFILE
    require(isinstance(document, dict) and set(document) == {"schema_version", "profile", "startup"}
            and type(document["schema_version"]) is int and document["schema_version"] == 2
            and document["profile"] == profile["name"], "Invalid qualified startup configuration header")
    startup = document["startup"]
    require(isinstance(startup, dict) and set(startup) == {"installation_root", "executable", "provider", "gnuplot", "ghostscript",
            "runtime_directories", "adapters", "octave_version", "octave_api_version"}
            and startup["octave_version"] == "11.3.0" and startup["octave_api_version"] == "api-v61"
            and qualified_path(startup["installation_root"]), "Invalid qualified startup selection")
    for key in ("executable", "provider", "gnuplot"):
        pin = startup[key]
        require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}
                and qualified_path(pin["path"], INTRINSIC_NAME if key == "provider" else None)
                and valid_sha256(pin["sha256"]), "Invalid qualified startup pin: " + key)
        if key == "gnuplot":
            require(re.fullmatch(r"[A-Za-z0-9/_+.-]+", pin["path"]),
                    "Qualified gnuplot needs a shell-safe ASCII executable path")
    directories = startup["runtime_directories"]
    require(isinstance(directories, list) and 0 < len(directories) <= 511
            and all(qualified_path(path) for path in directories) and len(set(directories)) == len(directories)
            and sum(len(path.encode("utf-8")) + 1 for path in directories) <= 65536,
            "Invalid qualified runtime directory allowlist")
    root = Path(startup["installation_root"])
    require(inside(Path(startup["executable"]["path"]), root)
            and all(inside(Path(path), root) for path in directories), "Qualified runtime selection is outside its installation root")
    require(startup["executable"]["sha256"] == native["runtime_executable_sha256"],
            "Qualified startup executable pin differs from the native runtime identity")
    adapters = startup["adapters"]
    expected_roles = {"input", "scope", "value", "debug"} | ({"graphics"} if graphics else set())
    require(isinstance(adapters, list) and len(adapters) == len(expected_roles), "Qualified startup needs its exact adapter selections")
    roles = set()
    for adapter in adapters:
        require(isinstance(adapter, dict) and set(adapter) == {"role", "file"}
                and isinstance(adapter["role"], str) and adapter["role"] in expected_roles
                and adapter["role"] not in roles, "Invalid or duplicate qualified adapter role")
        roles.add(adapter["role"])
        pin = adapter["file"]
        name = "mathlab_" + adapter["role"] + "_bridge.oct"
        require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}
                and qualified_path(pin["path"], name) and valid_sha256(pin["sha256"])
                and pin["sha256"] == native["artifact_sha256"][name], "Qualified adapter pin differs: " + name)
    require(startup["provider"]["sha256"] == native["artifact_sha256"][INTRINSIC_NAME],
            "Qualified provider pin differs from the installed artifact identity")
    ghostscript = startup["ghostscript"]
    qualified_ghostscript_snapshot(ghostscript)
    require(native.get("ghostscript") == ghostscript,
            "Qualified Ghostscript pin differs from the native build identity")
    selected = [startup[key]["path"] for key in ("executable", "provider", "gnuplot")]
    selected.extend(adapter["file"]["path"] for adapter in adapters)
    require(ghostscript["path"] not in selected, "Qualified Ghostscript must be a distinct selected executable")
    return document


def installed_startup_config(seed, bundle, native):
    """Preserve external selections; rebase only the exact five/six native pins."""
    document = read_qualified_startup(seed, native)
    embedded = read_qualified_startup(bundle / STARTUP_CONFIG_PATH, native)
    require(document["startup"]["ghostscript"] == embedded["startup"]["ghostscript"],
            "Selected Ghostscript differs from the embedded startup configuration")
    startup = document["startup"]
    require(not inside(Path(startup["ghostscript"]["path"]), bundle.resolve()),
            "Qualified Ghostscript must remain outside the installed bundle")
    for adapter in startup["adapters"]:
        adapter["file"]["path"] = str(bundle / "Contents/Resources/native" / ("mathlab_" + adapter["role"] + "_bridge.oct"))
    startup["provider"]["path"] = str(bundle / "Contents/Resources/native" / INTRINSIC_NAME)
    require(startup["ghostscript"]["path"] not in {startup["provider"]["path"],
            *(adapter["file"]["path"] for adapter in startup["adapters"])},
            "Qualified Ghostscript overlaps an installed native module")
    for name in (GRAPHICS_MODULE_NAMES if native["adapter_profile"] == GRAPHICS_ADAPTER_PROFILE else QUALIFIED_MODULE_NAMES):
        probe_artifact_snapshot(bundle / "Contents/Resources/native" / name, native["artifact_sha256"][name])
    return document


def inspect_value_component(bundle, actual, build):
    """Check the schema-1 additive contract without trusting a live helper import."""
    native = build["native"]
    artifacts, protocols = native.get("artifact_sha256", {}), native.get("protocols", {})
    require(isinstance(artifacts, dict) and isinstance(protocols, dict), "Malformed native artifact/protocol identity")
    value_paths = [path.relative_to(bundle).as_posix() for path in bundle.rglob("*")
                   if path.name.lower() == VALUE_NAME]
    graphics = native.get("adapter_profile") == GRAPHICS_ADAPTER_PROFILE
    qualified = native.get("adapter_profile") in QUALIFIED_PROFILES
    if not graphics:
        require(not any(path.name.lower() == GRAPHICS_NAME for path in bundle.rglob("*"))
                and not any(name.lower() == GRAPHICS_NAME for name in artifacts)
                and not any(name.lower() == "graphics" for name in protocols),
                "Non-graphics profile must not bundle or declare a graphics adapter")
    if not qualified:
        require(not any(path.name.lower() == INTRINSIC_NAME for path in bundle.rglob("*"))
                and not any(name.lower() == INTRINSIC_NAME for name in artifacts)
                and not any(name.lower() == "intrinsic" for name in protocols),
                "Non-qualified manifest must not bundle or declare an intrinsic provider")
        reject_qualified_observation(native)
        if isinstance(native.get("worker_observation"), dict):
            reject_qualified_observation(native["worker_observation"])
    if "adapter_profile" not in native:
        require(not value_paths and not any(name.lower() == VALUE_NAME for name in artifacts)
                and not any(name.lower() == "value" for name in protocols)
                and "value_component" not in native and "worker_observation" not in native,
                "Legacy adapter manifest must not bundle or declare a value adapter")
        return
    profile = native["adapter_profile"]
    require(isinstance(profile, dict) and set(profile) == {"schema_version", "name"}
            and type(profile["schema_version"]) is int and profile["schema_version"] == 1
            and profile["name"] in (REQUIRED_ADAPTER_PROFILE["name"], QUALIFIED_ADAPTER_PROFILE["name"],
                                    GRAPHICS_ADAPTER_PROFILE["name"], OMITTED_ADAPTER_PROFILE["name"]),
            "Unknown or malformed native adapter_profile declaration")
    modules = {name for name in actual if PurePosixPath(name).suffix.lower() == ".oct"}
    if profile == OMITTED_ADAPTER_PROFILE:
        require(native.get("status") == "not_included" and isinstance(native.get("reason"), str)
                and native["reason"].strip() and not artifacts and not protocols
                and "worker_observation" not in native and native.get("value_component") == OMITTED_VALUE_COMPONENT
                and not modules and not value_paths,
                "Omitted native profile must not bundle or declare native/value adapters")
        return
    require(native.get("status") == "enabled_protocol_load_passed", "Required value profile did not pass native protocol loading")
    required_names = set(NATIVE_NAMES) | {VALUE_NAME}
    if qualified:
        required_names.add(INTRINSIC_NAME)
    if graphics:
        required_names.add(GRAPHICS_NAME)
    require(value_paths == [VALUE_PATH] and modules == {"Contents/Resources/native/" + name
            for name in required_names if name.endswith(".oct")},
            "Required native profile needs exactly its canonical native modules")
    require(set(artifacts) == required_names and all(valid_sha256(value) for value in artifacts.values()),
            "Required native profile needs all selected module hashes and runtime-version hash")
    for name in required_names:
        path = "Contents/Resources/native/" + name
        require(path in actual and actual[path]["size"] > 0 and artifacts[name] == actual[path]["sha256"],
                "Required native artifact is missing or differs: " + name)
    require(native.get("value_component") == {"status": "required", "path": VALUE_PATH,
            "capability": VALUE_CAPABILITY, "sha256": artifacts[VALUE_NAME]},
            "Required value component declaration disagrees with its path/hash/capability")
    runtime_hash = native.get("runtime_executable_sha256")
    require(valid_sha256(runtime_hash), "Required value profile needs the selected runtime executable SHA-256")
    version = native.get("runtime_version")
    require(isinstance(version, str) and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[+~-][A-Za-z0-9.+_~-]*)?", version)
            and isinstance(native.get("sdk"), dict) and native["sdk"].get("OCTAVE_VERSION") == version
            and (bundle / "Contents/Resources/native/runtime-version.txt").read_text().strip() == version,
            "Required value profile runtime/SDK/version marker disagree")
    if qualified:
        require(version == "11.3.0" and native["sdk"].get("OCTAVE_API_VERSION") == "api-v61",
                "Qualified native profile requires Octave 11.3.0/api-v61")
        protocol = protocols.get("intrinsic")
        require(isinstance(protocol, dict) and set(protocol) == set(INTRINSIC_PROTOCOL)
                and all(type(protocol[key]) is type(expected) and protocol[key] == expected
                        for key, expected in INTRINSIC_PROTOCOL.items()), "Invalid intrinsic_protocol response")
    if graphics:
        protocol = protocols.get("graphics")
        require(isinstance(protocol, dict) and set(protocol) == set(GRAPHICS_PROTOCOL)
                and all(type(protocol[key]) is type(expected) and protocol[key] == expected
                        for key, expected in GRAPHICS_PROTOCOL.items()), "Invalid graphics protocol response")
    require(set(protocols) == {"debug", "scope", "input", "value"}
            | ({"intrinsic"} if qualified else set()) | ({"graphics"} if graphics else set()),
            "Required native profile observed protocol set differs")
    for name, protocol_name, expected_version in (("debug", "mathlab-debugger", 4), ("scope", "mathlab-scope", 2),
                                                  ("input", "mathlab-input", 1)):
        protocol = protocols[name]
        require(isinstance(protocol, dict) and protocol.get("protocol") == protocol_name
                and type(protocol.get("version")) is int and protocol["version"] == expected_version,
                "Required native protocol disagrees: " + name)
    value_protocol = protocols["value"]
    require(isinstance(value_protocol, dict) and set(value_protocol) == set(VALUE_PROTOCOL)
            and all(type(value_protocol[key]) is type(expected) and value_protocol[key] == expected
                    for key, expected in VALUE_PROTOCOL.items()),
            "Value adapter must report the exact mathlab-value v1 protocol and limit profile")
    links = build.get("linked_libraries")
    require(isinstance(links, dict) and all(isinstance(links.get(name), str) and links[name].strip()
            for name in required_names if name.endswith(".oct")),
            "Required native profile needs linked-library inspection for all selected modules")
    observation = native.get("worker_observation")
    observation_fields = {
        "method", "ready", "cli_executable_sha256", "value_adapter", "runtime", "worker_capabilities",
        "module_sha256", "runtime_executable_sha256"}
    if qualified:
        observation_fields |= {"native_modules", "qualified_startup_status", "session_context"}
    require(isinstance(observation, dict) and set(observation) == observation_fields,
        "Required value profile needs an identified worker observation")
    capabilities = capability_names(observation["worker_capabilities"])
    require(observation["method"] == "built-cli-doctor" and observation["ready"] is True
            and valid_sha256(observation["cli_executable_sha256"])
            and observation["cli_executable_sha256"] == build["executable_sha256"]["mathlab"]
            and observation["cli_executable_sha256"] == actual[CLI_PATH]["sha256"]
            and VALUE_REQUIRED_CAPABILITIES.issubset(capabilities)
            and observation["module_sha256"] == artifacts[VALUE_NAME]
            and observation["runtime_executable_sha256"] == runtime_hash,
            "Required value profile worker capability or runtime/module identity disagrees")
    require(observed_absolute_path(observation["value_adapter"], VALUE_NAME)
            and observed_absolute_path(observation["runtime"]),
            "Required value profile needs canonical absolute observed runtime/value paths")
    if qualified:
        validate_qualified_observation(observation, artifacts, graphics=graphics)
        read_qualified_startup(bundle / STARTUP_CONFIG_PATH, native)


def inventory(root):
    result = {}
    for path in root.rglob("*"):
        require(not path.is_symlink(), "Symlink in installed payload: " + str(path))
        if path.is_file():
            name = path.relative_to(root).as_posix()
            result[name] = {"path": name, "sha256": digest(path), "size": path.stat().st_size,
                            "executable": bool(path.stat().st_mode & 0o111)}
        else:
            require(path.is_dir(), "Special file in installed payload: " + str(path))
    return result


def records(value, label):
    require(isinstance(value, list) and value, label + " must be a nonempty list")
    result = {}
    for item in value:
        require(isinstance(item, dict), label + " contains a non-object")
        name = relative_name(item.get("path"))
        require(name not in result, label + " contains duplicate paths")
        require(isinstance(item.get("sha256"), str) and HEX256.fullmatch(item["sha256"]),
                label + " contains an invalid SHA-256")
        require(type(item.get("size")) is int and item["size"] >= 0
                and type(item.get("executable")) is bool, label + " lacks size/mode identity")
        result[name] = item
    return result


def inspect_bundle(bundle):
    require(bundle.is_dir(), "Bundle is not a directory")
    actual = inventory(bundle)
    require(RELEASE_PATH in actual and CHECKSUM_PATH in actual, "Bundle lacks release.json or final checksums")
    manifest = read_json(bundle / RELEASE_PATH)
    require(isinstance(manifest, dict) and manifest.get("schema_version") == 1,
            "Unsupported release manifest schema")
    require(isinstance(manifest.get("version"), str) and manifest["version"], "Missing release version")
    require(isinstance(manifest.get("profile"), str) and manifest["profile"], "Missing advertised profile")
    source, distribution, build = (manifest.get(key) for key in ("source", "distribution", "build"))
    require(all(isinstance(item, dict) for item in (source, distribution, build)), "Missing release identity objects")
    require(distribution.get("clean_candidate") is True and source.get("dirty") is False,
            "Qualification requires clean_candidate=true and source.dirty=false")
    require(isinstance(source.get("commit"), str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", source["commit"]),
            "Missing immutable source commit identity")
    require(source.get("frozen_export") is True, "Source is not identified as a frozen export")
    source_records = records(source.get("files"), "Source manifest")
    export_hash = hashlib.sha256(json.dumps(source["files"], sort_keys=True).encode()).hexdigest()
    require(source.get("export_sha256") == export_hash, "Source export identity disagrees with file records")
    prefix = "Contents/Resources/source/"
    expected_source = {prefix + name: {**item, "path": prefix + name} for name, item in source_records.items()}
    require({name: item for name, item in actual.items() if name.startswith(prefix)} == expected_source,
            "Installed corresponding source differs from its manifest")
    for name in ("Cargo.toml", "Cargo.lock", "LICENSE", "crates/mathlab-cli/src/main.rs",
                 "octave/mathlab_worker.m", "examples/getting_started.m"):
        require(name in source_records, "Corresponding source lacks " + name)
    payload = records(manifest.get("payload"), "Payload manifest")
    require(set(actual) == set(payload) | {RELEASE_PATH, CHECKSUM_PATH}, "Unmanifested or missing installed payload")
    require(all(actual.get(name) == item for name, item in payload.items()), "Installed payload hash/size/mode mismatch")
    checksum_text = (bundle / CHECKSUM_PATH).read_text(encoding="utf-8")
    checksums = {}
    for line in checksum_text.splitlines():
        require(len(line) > 66 and line[64:66] == "  " and HEX256.fullmatch(line[:64]), "Invalid checksum line")
        name = relative_name(line[66:])
        require(name not in checksums, "Duplicate final checksum path")
        checksums[name] = line[:64]
    require(checksums == {name: item["sha256"] for name, item in actual.items() if name != CHECKSUM_PATH},
            "Final checksums disagree with installed bytes")
    executables = build.get("executable_sha256")
    require(isinstance(executables, dict), "Build lacks executable hashes")
    for name, path in (("mathlab", CLI_PATH), ("mathlab-desktop", DESKTOP_PATH)):
        require(path in actual and actual[path]["executable"]
                and executables.get(name) == actual[path]["sha256"], "Built executable identity mismatch: " + name)
    example = "Contents/Resources/examples/getting_started.m"
    require(example in actual and actual[example]["sha256"] == source_records["examples/getting_started.m"]["sha256"],
            "Installed introductory example differs from corresponding source")
    native = build.get("native")
    require(isinstance(native, dict), "Native inclusion/omission is not explicit")
    included = native.get("status") != "not_included"
    native_prefix = "Contents/Resources/native/"
    if included:
        require(native.get("status") == "enabled_protocol_load_passed", "Unknown advertised native capability state")
        hashes = native.get("artifact_sha256")
        require(isinstance(hashes, dict), "Advertised native adapters lack artifact hashes")
        for name in NATIVE_NAMES:
            path = native_prefix + name
            require(path in actual and actual[path]["size"] > 0 and hashes.get(name) == actual[path]["sha256"],
                    "Missing or mismatched advertised native adapter: " + name)
    else:
        require(not any(name.startswith(native_prefix) for name in actual), "Undeclared native files in installed payload")
    inspect_value_component(bundle, actual, build)
    return manifest, actual, included


def kill_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
        return "group_killed"
    except ProcessLookupError:
        return "group_absent"


class Runner:
    """Bound output, lifetime, and descendants without invoking a shell."""
    def __init__(self, directory, project, environment, timeout, output_limit=2 * 1024 * 1024):
        self.directory, self.project, self.environment = directory, project, environment
        self.timeout, self.output_limit = timeout, output_limit
        self.commands = []

    def run(self, label, argv, stdin=None, on_stdout=None):
        require(os.name == "posix", "Process-group qualification currently requires a POSIX host")
        data = stdin.encode("utf-8") if stdin is not None else b""
        require(len(data) <= 4096, "QA stdin exceeds the bounded fixture size")
        stem = "%02d-%s" % (len(self.commands) + 1, label)
        stdout, stderr = (self.directory / (stem + suffix) for suffix in (".stdout.log", ".stderr.log"))
        entry = {"id": label, "argv": [str(arg) for arg in argv], "working_directory": str(self.project),
                 "timeout_seconds": self.timeout, "stdout_log": stdout.name, "stderr_log": stderr.name,
                 "stdin_bytes": len(data), "timed_out": False, "output_limit_exceeded": False,
                 "exit_code": None, "status": "failed", "cleanup_status": "not_run", "cleanup_errors": [],
                 "stdout_sha256": None, "stdout_bytes": None, "stderr_sha256": None, "stderr_bytes": None}
        self.commands.append(entry)
        started, process = time.monotonic(), None
        selection = selectors.DefaultSelector()

        def cleanup(stage, action):
            try:
                return action()
            except Exception as error:
                entry["cleanup_errors"].append({"stage": stage, "error_type": type(error).__name__,
                                                "errno": getattr(error, "errno", None), "message": str(error)})

        def cleanup_group(stage):
            result = cleanup(stage, lambda: kill_group(process))
            if result is None:
                entry["process_group_cleanup"] = "failed"
            else:
                # A later successful attempt cannot erase an earlier cleanup failure.
                entry.setdefault("process_group_cleanup", result)

        try:
            with stdout.open("wb") as out, stderr.open("wb") as err:
                process = subprocess.Popen(entry["argv"], cwd=self.project, env=self.environment,
                                           stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                entry["pid"] = process.pid
                if stdin is not None:
                    try:
                        process.stdin.write(data)
                        process.stdin.close()
                    except BrokenPipeError:
                        process.stdin.close()
                for stream, destination in ((process.stdout, out), (process.stderr, err)):
                    os.set_blocking(stream.fileno(), False)
                    selection.register(stream, selectors.EVENT_READ, destination)
                total, cleaned, drain_until = 0, False, None
                while selection.get_map() or process.poll() is None:
                    now = time.monotonic()
                    if process.poll() is None and not cleaned and now - started >= self.timeout:
                        entry["timed_out"] = True
                        cleanup_group("timeout_process_group")
                        cleaned, drain_until = True, now + 2
                    elif process.poll() is not None and not cleaned:
                        cleanup_group("exited_process_group")
                        cleaned, drain_until = True, now + 2
                    if drain_until is not None and now > drain_until:
                        break
                    for key, _ in selection.select(timeout=0.05):
                        try:
                            block = os.read(key.fileobj.fileno(), 65536)
                        except BlockingIOError:
                            continue
                        if not block:
                            selection.unregister(key.fileobj)
                            continue
                        remaining = max(0, self.output_limit - total)
                        key.data.write(block[:remaining])
                        if on_stdout is not None and key.data is out:
                            on_stdout(process, block[:remaining])
                        total += len(block)
                        if total > self.output_limit and not entry["output_limit_exceeded"]:
                            entry["output_limit_exceeded"] = True
                            cleanup_group("output_limit_process_group")
                            cleaned, drain_until = True, time.monotonic() + 2
                process.wait(timeout=3)
        except (OSError, subprocess.SubprocessError) as error:
            entry["reason"] = str(error)
        finally:
            if process is not None:
                cleanup_group("final_process_group")
                cleanup("final_process_wait", lambda: process.wait(timeout=3))
                entry["exit_code"] = process.returncode
                for name, stream in (("stdin", process.stdin), ("stdout", process.stdout), ("stderr", process.stderr)):
                    if stream is not None:
                        cleanup("close_" + name, stream.close)
            cleanup("close_selector", selection.close)
            entry["cleanup_status"] = "failed" if entry["cleanup_errors"] else ("passed" if process is not None else "not_run")
            entry["elapsed_seconds"] = round(time.monotonic() - started, 3)
            for name, path in (("stdout", stdout), ("stderr", stderr)):
                if path.exists():
                    entry[name + "_sha256"] = digest(path)
                    entry[name + "_bytes"] = path.stat().st_size
            entry["status"] = "passed" if entry["exit_code"] == 0 and not (
                entry["timed_out"] or entry["output_limit_exceeded"] or entry.get("reason") or entry["cleanup_errors"]) else "failed"
        return entry, stdout.read_text(encoding="utf-8", errors="replace"), stderr.read_text(encoding="utf-8", errors="replace")


def command_ok(command, expected_success=True):
    entry, stdout, stderr = command
    entry["expected_exit"] = "zero" if expected_success else "nonzero"
    require(not entry.get("cleanup_errors") and entry.get("cleanup_status") != "failed",
            "Command cleanup failed; retained command and log evidence must be reviewed")
    require(not entry["timed_out"] and not entry["output_limit_exceeded"]
            and not entry.get("reason") and entry["exit_code"] is not None,
            "Command did not finish within its process/output bounds")
    require((entry["exit_code"] == 0) == expected_success,
            "Unexpected CLI exit code: " + str(entry["exit_code"]))
    return stdout, stderr


def execution(value, project):
    require(isinstance(value, dict) and value.get("error") is None, "CLI reported an execution error")
    require(isinstance(value.get("output"), str) and isinstance(value.get("workspace"), list)
            and isinstance(value.get("figures"), list), "Invalid execution-result schema")
    require(value.get("output_truncated") is False and value.get("output_dropped_bytes") == 0
            and value.get("output_decoding_loss") is False, "Execution output was truncated or decoded lossily")
    require(Path(value.get("working_directory", "")).resolve() == project, "CLI executed outside the owned project")
    return value


def parse_json(text):
    try:
        return json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_json_constant)
    except ValueError as error:
        raise QAFailure("CLI stdout is not one clean JSON record") from error


def json_stream(text):
    decoder, values, offset = json.JSONDecoder(), [], 0
    while text[offset:].strip():
        offset += len(text[offset:]) - len(text[offset:].lstrip())
        try:
            value, offset = decoder.raw_decode(text, offset)
        except ValueError as error:
            raise QAFailure("REPL stdout is not a clean stream of JSON records") from error
        values.append(value)
    return values


def variable(value, name, dimensions):
    matches = [item for item in value.get("workspace", []) if isinstance(item, dict) and item.get("name") == name]
    require(len(matches) == 1 and matches[0].get("class_name") == "double"
            and matches[0].get("dimensions") == dimensions, "Unexpected class/shape for " + name)
    return matches[0]


def png_info(path):
    require(32 <= path.stat().st_size <= 16 * 1024 * 1024, "PNG size is outside QA bounds")
    data = path.read_bytes()
    require(data[:8] == b"\x89PNG\r\n\x1a\n", "Export is not a PNG")
    offset, dimensions, image_data, ended = 8, None, False, False
    while offset < len(data):
        require(offset + 12 <= len(data), "Truncated PNG chunk")
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        kind = data[offset + 4:offset + 8]
        end = offset + length + 12
        require(end <= len(data), "Truncated PNG payload")
        block = data[offset + 8:offset + 8 + length]
        checksum = struct.unpack(">I", data[offset + 8 + length:end])[0]
        require(zlib.crc32(kind + block) & 0xffffffff == checksum, "PNG chunk CRC mismatch")
        if dimensions is None:
            require(kind == b"IHDR" and length == 13, "PNG lacks an initial IHDR")
            width, height = struct.unpack(">II", block[:8])
            require(1 <= width <= 16384 and 1 <= height <= 16384, "PNG dimensions outside QA bounds")
            dimensions = {"width": width, "height": height}
        if kind == b"IDAT" and length:
            image_data = True
        if kind == b"IEND":
            require(length == 0 and end == len(data), "Invalid PNG end chunk")
            ended = True
        offset = end
    require(image_data and ended, "PNG lacks image data or end chunk")
    return dimensions


class Acceptance:
    def __init__(self, bundle, report_dir, octave=None, timeout=90, output_limit=2 * 1024 * 1024, startup_config=None):
        require(octave is None or startup_config is None, "--octave and --startup-config are mutually exclusive")
        self.bundle = Path(bundle).resolve()
        parent = Path(report_dir).resolve()
        require(not inside(parent, self.bundle), "Report directory must be outside the app bundle")
        require(0 < timeout <= 600, "Timeout must be greater than zero and at most 600 seconds")
        parent.mkdir(parents=True, exist_ok=True)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.directory = parent / ("installed-cli-" + stamp + "-" + uuid.uuid4().hex[:10])
        self.directory.mkdir(mode=0o700)
        self.octave = str(octave) if octave is not None else None
        self.startup_seed = Path(startup_config).resolve() if startup_config is not None else None
        self.resolved_startup_config = None
        if self.octave is not None:
            # Preserve symlink/launcher invocation semantics; canonical paths
            # are used only when comparing and hashing the reported runtime.
            self.octave = str(Path(shutil.which(self.octave) or self.octave).absolute())
        self.timeout, self.output_limit = timeout, output_limit
        self.report = {"schema_version": 1, "scope": "installed_cli_only", "status": "not_run",
                       "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                       "bundle": str(self.bundle), "report_directory": str(self.directory),
                       "tool_identity": {"script_path": str(Path(__file__).absolute()),
                                         "script_sha256": digest(Path(__file__)),
                                         "python_executable": sys.executable,
                                         "python_version": sys.version},
                       "checks": {name: {"status": "not_run", "required_for_cli_subset": name not in ("authentic_input", "packaged_value_api")}
                                  for name in CHECKS}, "commands": [], "artifacts": [],
                       "pending_release_journeys": [{"id": name, "status": "not_run", "required_for_release": True,
                                                     "reason": reason} for name, reason in PENDING],
                       "qualification": {"cli_subset": "not_run", "release_qualified": False,
                                         "native_input_coverage": "not_run",
                                         "packaged_value_read_edit_history": "not_run",
                                         "packaged_portable_uint64_read_edit_history": "not_run",
                                         "packaged_native_complex_read_edit_history": "not_run"},
                       "limitations": ["CLI success does not establish desktop/UI, Stop/debugger, read-only installation or clean-machine readiness.",
                                       "MAT evidence is an Octave writer/reader self-round-trip, not MATLAB file interchange.",
                                       "Integrity hashes are consistency checks, not a signature or independent source audit.",
                                       "Process cleanup covers the owned POSIX process group, not deliberately detached foreign processes."]}

    def check(self, name, action):
        try:
            details = action()
            self.report["checks"][name].update(status="passed", evidence=details or {})
            return True
        except (QAFailure, OSError, ValueError, KeyError, TypeError, struct.error) as error:
            self.report["checks"][name].update(status="failed", reason=str(error))
            return False

    def retain(self, path, name, allowed_root):
        path = Path(path)
        require(not path.is_symlink() and path.is_file() and inside(path.resolve(), allowed_root),
                "Output is outside the owned temporary project: " + str(path))
        require(path.stat().st_size <= 16 * 1024 * 1024, "Generated artifact exceeds QA bounds")
        destination = self.directory / "artifacts" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
        item = {"path": destination.relative_to(self.directory).as_posix(), "sha256": digest(destination),
                "bytes": destination.stat().st_size, "original_temporary_path": str(path)}
        self.report["artifacts"].append(item)
        return item

    def figures(self, value, name):
        require(not value.get("figure_warnings"), "Figure export emitted warnings")
        require(bool(value["figures"]), "Execution returned no figure")
        results = []
        for index, item in enumerate(value["figures"]):
            path = Path(item["path"])
            require(not path.is_symlink() and inside(path.resolve(), self.project / ".mathlab/figures"),
                    "Figure was not preserved in the owned project")
            info = png_info(path)
            results.append({**self.retain(path, name + "/figure-%d.png" % (index + 1), self.project), **info})
        return results

    def cli(self, label, action, stdin=None, octave_override=None, runner=None, startup_override=None):
        argv = [str(self.bundle / CLI_PATH), "--directory", str(self.project),
                "--timeout", str(max(1, math.ceil(self.timeout))), "--json"]
        if self.qualified_required:
            require(self.resolved_startup_config is not None, "Qualified CLI startup configuration was not resolved")
            selected = self.startup_document["startup"]["executable"]["path"]
            require(octave_override is None or str(Path(octave_override).resolve()) == selected,
                    "Qualified runtime override differs from the pinned startup executable")
            argv.extend(["--startup-config", str(startup_override or self.resolved_startup_config)])
        else:
            require(startup_override is None, "Non-qualified CLI cannot activate startup configuration")
            selected = octave_override if octave_override is not None else self.octave
            if selected is not None:
                argv.extend(["--octave", str(selected)])
        argv.extend(action)
        before = qualified_ghostscript_snapshot(self.startup_document["startup"]["ghostscript"]) if self.qualified_required else None
        command = (runner or self.runner).run(label, argv, stdin)
        if self.qualified_required:
            command[0]["ghostscript"] = {"before": before}
            after = qualified_ghostscript_snapshot(self.startup_document["startup"]["ghostscript"])
            command[0]["ghostscript"]["after"] = after
            require(before == after, "Selected external Ghostscript changed during the CLI command")
        return command

    def write_startup_config(self, document, name):
        encoded = (json.dumps(document, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
        require(len(encoded) <= 1024 * 1024, "Resolved qualified startup configuration exceeds one MiB")
        path = self.temporary_root / "startup-state" / name
        path.parent.mkdir(mode=0o700, exist_ok=True)
        with path.open("xb") as output:
            output.write(encoded)
        path.chmod(0o600)
        retained = self.retain(path, "startup-config/" + name, self.temporary_root)
        return path, retained

    def run_script(self, label, source, stdin=None):
        path = self.project / (label + ".m")
        path.write_text(source, encoding="utf-8")
        self.retain(path, "fixtures/" + path.name, self.project)
        stdout, _ = command_ok(self.cli(label, ["run", str(path), "--workspace"], stdin))
        return execution(parse_json(stdout), self.project), path

    def doctor(self):
        stdout, _ = command_ok(self.cli("doctor", ["doctor"]))
        value = parse_json(stdout)
        require(isinstance(value, dict) and value.get("ready") is True, "Packaged doctor is not ready")
        require(value.get("version") == self.manifest["version"], "Doctor version disagrees with release manifest")
        require(Path(value.get("working_directory", "")).resolve() == self.project, "Doctor did not use the owned project")
        runtime = Path(value.get("runtime", "")).resolve()
        require(runtime.is_file(), "Doctor did not identify an existing runtime executable")
        if self.octave is not None:
            require(runtime == Path(self.octave).resolve(), "Doctor used a different runtime than --octave")
        if self.qualified_required:
            require(str(runtime) == self.startup_document["startup"]["executable"]["path"],
                    "Doctor used a different runtime than the pinned startup configuration")
        identity = {"path": str(runtime), "sha256": digest(runtime), "version": value.get("runtime_version"),
                    "packages": value.get("packages", [])}
        capabilities = capability_names(value.get("worker_capabilities", []))
        if self.value_required:
            require(observed_absolute_path(value.get("runtime")), "Required value doctor did not identify an absolute runtime path")
            require(value.get("value_adapter") == str((self.bundle / VALUE_PATH).resolve()),
                    "Doctor did not activate the canonical installed value adapter")
            require(VALUE_REQUIRED_CAPABILITIES.issubset(capabilities),
                    "Doctor did not negotiate all required value worker capabilities")
        else:
            require(value.get("value_adapter") is None and VALUE_CAPABILITY not in capabilities,
                    "Omitted or legacy profile unexpectedly activated a value adapter/capability")
        if self.native:
            for key, name in (("input_adapter", "mathlab_input_bridge.oct"), ("scope_adapter", "mathlab_scope_bridge.oct")):
                require(Path(value.get(key) or "").resolve() == self.bundle / "Contents/Resources/native" / name,
                        "Doctor did not activate the bundled " + key)
            expected_runtime = self.manifest["build"]["native"].get("runtime_executable_sha256")
            if self.value_required:
                require(valid_sha256(expected_runtime), "Required value profile lacks its runtime executable fingerprint")
            if expected_runtime:
                require(identity["sha256"] == expected_runtime, "External runtime differs from the native build fingerprint")
        if self.qualified_required:
            validate_qualified_observation(value, self.manifest["build"]["native"]["artifact_sha256"], self.bundle,
                                           graphics=self.graphics_required)
        else:
            reject_qualified_observation(value)
        self.report["runtime"] = identity
        return value

    def missing_runtime(self):
        missing = self.project / "deliberately-missing-octave"
        require(not missing.exists(), "Negative-test executable unexpectedly exists")
        if self.qualified_required:
            document = json.loads(json.dumps(self.startup_document))
            document["startup"]["executable"]["path"] = str(missing)
            variant, _ = self.write_startup_config(document, "missing-executable.json")
            command = self.cli("missing-runtime", ["doctor"], startup_override=variant)
        else:
            command = self.cli("missing-runtime", ["doctor"], octave_override=missing)
        stdout, stderr = command_ok(command, False)
        combined = (stdout + stderr).lower()
        require("octave" in combined and any(text in combined for text in
                ("cannot start", "not found", "no such file", "install", "required", "does not exist")),
                "Missing-runtime failure has no recognizable diagnostic")
        if self.qualified_required:
            require(not any(text in combined for text in ("conflicts with", "cannot be used with", "argument conflict"))
                    and any(text in combined for text in ("no such file", "not found", "does not exist", "missing executable",
                                                         "cannot resolve", "cannot start")),
                    "Qualified missing-runtime case did not reach executable admission")
        if stdout.strip().startswith("{"):
            require(parse_json(stdout).get("ready") is not True, "Missing-runtime doctor falsely reported ready")
        return {"expected_nonzero_exit": True, "scope": (
            "Explicit missing executable in retained startup configuration; executable admission diagnostic required"
            if self.qualified_required else "Explicit missing --octave path; not a clean-machine autodiscovery test")}

    def packaged_value_api(self):
        required = self.value_required
        runtime = self.report["runtime"]
        expected = {"cli_executable": (self.bundle / CLI_PATH, self.manifest["build"]["executable_sha256"]["mathlab"]),
                    "octave_runtime": (Path(runtime["path"]), runtime["sha256"])}
        environment = self.runner.environment.copy()
        for variable, _ in PROBE_ADAPTERS.values():
            environment.pop(variable, None)
        if required:
            native = self.manifest["build"]["native"]
            require(runtime["sha256"] == native["runtime_executable_sha256"], "Required probe runtime differs from the native build")
            for kind, (variable, name) in PROBE_ADAPTERS.items():
                path = self.bundle / "Contents/Resources/native" / name
                if not self.qualified_required:
                    environment[variable] = str(path)
                expected[kind] = (path, native["artifact_sha256"][name])
            if self.qualified_required:
                expected["intrinsic_provider"] = (self.bundle / "Contents/Resources/native" / INTRINSIC_NAME,
                                                   native["artifact_sha256"][INTRINSIC_NAME])
            if self.graphics_required:
                expected["graphics_adapter"] = (self.bundle / "Contents/Resources/native" / GRAPHICS_NAME,
                                                native["artifact_sha256"][GRAPHICS_NAME])
        before = {kind: probe_artifact_snapshot(path, expected_hash) for kind, (path, expected_hash) in expected.items()}
        runner = Runner(self.directory, self.project, environment, self.timeout, self.output_limit)
        runner.commands = self.runner.commands
        action = ["probe-value-api"] + ([] if required else ["--expect-unavailable"])
        command = self.cli("packaged-value-api", action, octave_override=runtime["path"], runner=runner)
        after = {kind: probe_artifact_snapshot(path, expected_hash) for kind, (path, expected_hash) in expected.items()}
        require(before == after, "Installed probe CLI/runtime/module identity changed during execution")
        stdout, _ = command_ok(command)
        report = parse_json(stdout)
        evidence = validate_value_probe(report, required, before, self.manifest["version"], self.temporary_root,
                                        qualified=self.qualified_required, graphics=self.graphics_required)
        if self.qualified_required:
            evidence.update({key: report[key] for key in ("native_modules", "qualified_startup_status", "session_context")})
        if required:
            require(report["runtime_details"]["version"] == self.manifest["build"]["native"]["runtime_version"],
                    "Installed probe runtime version differs from the native build")
        evidence["independent_artifact_identities_before"] = before
        evidence["independent_artifact_identities_after"] = after
        evidence["command_id"] = "packaged-value-api"
        self.report["qualification"]["packaged_portable_uint64_read_edit_history"] = "passed"
        self.report["qualification"]["packaged_native_complex_read_edit_history"] = evidence["native_complex_read_write_undo_redo"]
        for journey in self.report["pending_release_journeys"]:
            if journey["id"] == "packaged_portable_uint64_read_edit_history":
                journey.update(status="passed", reason="The identified packaged CLI probe executed the exact uint64 read/write/Undo/Redo case.",
                               evidence_check="packaged_value_api")
            elif journey["id"] == "packaged_value_read_edit_history":
                if required:
                    journey.update(status="passed", reason=evidence["scope"], evidence_check="packaged_value_api")
                    self.report["qualification"]["packaged_value_read_edit_history"] = "passed"
                else:
                    journey["reason"] = "Native complex support is unavailable in this profile. Portable uint64 success and expected complex rejection are recorded separately."
            elif required and journey["id"] == "stop_and_recovery":
                journey["reason"] = "The packaged API probe exercised its ordinary same-process Stop case; desktop Stop and broader recovery journeys remain unrun."
        return evidence

    def numerics(self):
        value, path = self.run_script("qa-numerics", NUMERICS)
        match = re.search(r"NUMJINN_QA_NUMERICS\s+(\S+)\s+(\S+)\s+(\S+)\s+(\d+)", value["output"])
        require(match is not None, "Numerical assertion marker is missing")
        x0, x1, residual = map(float, match.group(1, 2, 3))
        require(all(math.isfinite(number) for number in (x0, x1, residual))
                and abs(x0 - 2) < 1e-12 and abs(x1 - 3) < 1e-12
                and 0 <= residual < 1e-12 and int(match.group(4)) == 42, "Numerical results differ from the original oracle")
        variable(value, "qa_x", [2, 1])
        return {"script": str(path), "x": [x0, x1], "residual": residual,
                "forward_local_function_answer": 42, "column_major_reshape_assertion": "executed"}

    def source_error(self):
        path = self.project / "qa-source-error.m"
        path.write_text(SOURCE_ERROR, encoding="utf-8")
        self.retain(path, "fixtures/" + path.name, self.project)
        stdout, _ = command_ok(self.cli("source-error", ["run", str(path)]), False)
        value = parse_json(stdout)
        error = value.get("error")
        require(isinstance(error, dict) and error.get("identifier") == "NumJinnQA:OriginalSource",
                "Expected original-source diagnostic was not returned")
        require(any(Path(item.get("file", "")).resolve() == path and item.get("line") == 2
                    for item in error.get("stack", [])), "Error did not map to original file line 2")
        return {"identifier": error["identifier"], "original_file": str(path), "line": 2}

    def repl(self):
        stdout, _ = command_ok(self.cli("persistent-repl", ["repl"], REPL))
        values = json_stream(stdout)
        require(len(values) == 3, "Persistent REPL returned an unexpected number of records")
        first, second = (execution(value, self.project) for value in values[:2])
        require("NUMJINN_QA_REPL_FIRST 41" in first["output"]
                and "NUMJINN_QA_REPL_SECOND 42" in second["output"], "REPL did not preserve and update the prior command's value")
        item = variable(values[2], "qa_session_seed", [1, 1])
        preview = item.get("preview")
        require(isinstance(preview, str), "REPL scalar preview is missing")
        preview = preview.strip()
        if preview.startswith("[") and preview.endswith("]"):
            preview = preview[1:-1].strip()
        require(re.fullmatch(r"[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?", preview)
                is not None, "REPL workspace preview is not one numeric scalar")
        number = float(preview)
        require(math.isfinite(number) and number == 42, "REPL workspace did not retain 42")
        return {"json_records": 3, "successive_values": [41, 42], "workspace_value": 42}

    def graphics(self):
        value, _ = self.run_script("qa-graphics", GRAPHICS)
        require("NUMJINN_QA_PLOT_DATA 0,0.5,1;0,0.25,1" in value["output"], "Plot data/property assertion marker is missing")
        variable(value, "qa_plot_t", [1, 3])
        variable(value, "qa_plot_y", [1, 3])
        evidence = {"xdata": [0, 0.5, 1], "ydata": [0, 0.25, 1], "pngs": self.figures(value, "graphics")}
        if self.qualified_required:
            pin = self.startup_document["startup"]["ghostscript"]
            observations = re.findall(r"^NUMJINN_QA_GSC (.*)$", value["output"], re.MULTILINE)
            require(observations == [pin["path"]], "Plotting worker GSC differs from the selected Ghostscript pin")
            evidence["ghostscript"] = {"selected": dict(pin), "observed_gsc": observations[0]}
        return evidence

    def example(self):
        installed = self.bundle / "Contents/Resources/examples/getting_started.m"
        path = self.project / "getting_started.m"
        # Keep the example's save/load assertions, but preserve its MAT output
        # in the project: the CLI removes its session-local tempname at exit.
        original = installed.read_bytes()
        assignment = b"\nmat_path = [tempname(), '.mat'];\n"
        require(original.count(assignment) == 1, "Bundled example MAT assignment changed")
        self.example_mat_path = self.project / "numjinn-qa-example.mat"
        require(not self.example_mat_path.exists() and not self.example_mat_path.is_symlink(),
                "QA MAT destination already exists")
        quoted_path = str(self.example_mat_path).replace("'", "''")
        replacement = ("\nmat_path = '" + quoted_path + "';\n").encode("utf-8")
        path.write_bytes(original.replace(assignment, replacement, 1))
        self.retain(path, "fixtures/getting_started.m", self.project)
        stdout, _ = command_ok(self.cli("getting-started", ["run", str(path), "--workspace"]))
        value = execution(parse_json(stdout), self.project)
        match = re.search(r"Solution: x = \[([^;]+); ([^\]]+)\], residual = (\S+)", value["output"])
        require(match is not None, "Bundled getting-started solution is missing")
        numbers = list(map(float, match.groups()))
        require(all(math.isfinite(number) for number in numbers)
                and abs(numbers[0] - 2) < 1e-12 and abs(numbers[1] - 3) < 1e-12
                and 0 <= numbers[2] < 1e-12, "Bundled example produced the wrong solution")
        variable(value, "x", [2, 1])
        variable(value, "t", [1, 501])
        variable(value, "y", [1, 501])
        self.example_value = value
        return {"installed_source": str(installed), "installed_source_sha256": digest(installed),
                "executed_copy": str(path), "executed_copy_sha256": digest(path),
                "source_adaptation": {"kind": "MAT_output_location_only",
                                      "expected_mat_path": str(self.example_mat_path),
                                      "original_assignment": assignment.decode("utf-8").strip(),
                                      "replacement_assignment": replacement.decode("utf-8").strip(),
                                      "save_load_assertions": "unchanged"},
                "solution": numbers[:2], "pngs": self.figures(value, "getting-started")}

    def mat(self):
        value = getattr(self, "example_value", None)
        require(value is not None, "Getting-started execution did not supply a MAT result")
        matches = re.findall(r"^Verified MAT round trip: (.+)$", value["output"], re.MULTILINE)
        require(len(matches) == 1, "Bundled example did not uniquely confirm its MAT equality assertions")
        expected = getattr(self, "example_mat_path", None)
        require(expected is not None and matches[0] == str(expected), "MAT output differs from the selected QA path")
        path = Path(matches[0])
        require(not path.is_symlink() and path.resolve() == expected and inside(path.resolve(), self.project),
                "MAT file is outside the owned temporary project or is a symlink")
        require(path.is_file(), "MAT file is missing after the example completed")
        with path.open("rb") as source:
            header = source.read(128)
        require(len(header) == 128 and header.startswith(b"MATLAB 5.0 MAT-file")
                and header[126:128] in (b"IM", b"MI") and path.stat().st_size > 128, "Expected a nonempty MAT5/v7 container")
        return {"artifact": self.retain(path, "getting-started.mat", self.project),
                "format_scope": "MAT5 container written with Octave -v7",
                "equality_scope": "Bundled example's unchanged same-runtime A/x/y save/load assertions; MAT output location adapted",
                "matlab_interchange_established": False}

    def authentic_input(self):
        value, _ = self.run_script("qa-input", INPUT, "  λ exact  \nqa_base+2\n\n")
        require("NUMJINN_QA_INPUT_OK 42" in value["output"], "Authentic input assertions did not finish")
        for exact in ("NUMJINN_QA_NAME>   λ exact  \n", "NUMJINN_QA_VALUE> qa_base+2\n", "NUMJINN_QA_EMPTY> \n"):
            require(value["output"].count(exact) == 1, "Native prompt/reply whitespace or echo count differs")
        variable(value, "qa_value", [1, 1])
        return {"adapter": str(self.bundle / "Contents/Resources/native/mathlab_input_bridge.oct"),
                "caller_scope_answer": 42, "exact_unicode_string": True, "empty_string_reply": True,
                "scope": "Packaged CLI piped stdin with the bundled input adapter; no TTY, EOF, Stop or debugger proof"}

    def run(self):
        before = None
        try:
            def identity():
                nonlocal before
                self.manifest, before, self.native = inspect_bundle(self.bundle)
                profile = self.manifest["build"]["native"].get("adapter_profile")
                self.graphics_required = profile == GRAPHICS_ADAPTER_PROFILE
                self.qualified_required = profile in QUALIFIED_PROFILES
                self.value_required = profile == REQUIRED_ADAPTER_PROFILE or self.qualified_required
                require(self.qualified_required == (self.startup_seed is not None),
                        "Qualified profile requires explicit --startup-config; non-qualified profiles must omit it")
                if self.qualified_required:
                    self.startup_document = installed_startup_config(
                        self.startup_seed, self.bundle, self.manifest["build"]["native"])
                    pin = self.startup_document["startup"]["ghostscript"]
                    self.report["ghostscript"] = {"selected": dict(pin), "before": qualified_ghostscript_snapshot(pin)}
                self.value_declared = "adapter_profile" in self.manifest["build"]["native"]
                self.report["checks"]["packaged_value_api"]["required_for_cli_subset"] = self.value_declared
                if not self.value_declared:
                    self.report["checks"]["packaged_value_api"]["reason"] = "Legacy profile: no new CLI probe or value support is required or inferred."
                self.report["provenance"] = {"product": self.manifest.get("product"), "profile": self.manifest["profile"],
                    "version": self.manifest["version"], "source_commit": self.manifest["source"]["commit"],
                    "source_export_sha256": self.manifest["source"]["export_sha256"],
                    "release_json_sha256": before[RELEASE_PATH]["sha256"],
                    "checksums_sha256": before[CHECKSUM_PATH]["sha256"],
                    "executable_sha256": self.manifest["build"]["executable_sha256"],
                    "native_included": self.native, "value_component_required": self.value_required,
                    "qualified_startup_required": self.qualified_required}
                for source_name, output_name in ((RELEASE_PATH, "release.json"), (CHECKSUM_PATH, "checksums.sha256")):
                    shutil.copyfile(self.bundle / source_name, self.directory / output_name)
                return self.report["provenance"]
            if self.check("bundle_identity", identity):
                base = next((Path(name).resolve() for name in (tempfile.gettempdir(), "/tmp")
                             if Path(name).is_dir() and not inside(Path(name).resolve(), SOURCE_ROOT)
                             and not inside(Path(name).resolve(), self.bundle)), None)
                require(base is not None, "No temporary directory outside checkout and bundle")
                with tempfile.TemporaryDirectory(prefix="numjinn-installed-qa-", dir=base) as temporary:
                    self.temporary_root = Path(temporary).resolve()
                    self.project = self.temporary_root / "project"
                    self.project.mkdir()
                    self.report["temporary_project"] = str(self.project)
                    if self.qualified_required:
                        self.resolved_startup_config, retained = self.write_startup_config(
                            self.startup_document, "installed.json")
                        self.report["startup_config"] = {"seed": str(self.startup_seed), "resolved": retained}
                    environment = os.environ.copy()
                    for key in ("MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE",
                                "MATHLAB_VALUE_BRIDGE", "MATHLAB_PROJECT_DIR"):
                        environment.pop(key, None)
                    if self.qualified_required:
                        for key in ("MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE", "GSC", "GNUPLOT"):
                            environment.pop(key, None)
                    for key, name in (("HOME", "home"), ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp"),
                                      ("XDG_CACHE_HOME", "cache"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data")):
                        path = self.temporary_root / name
                        path.mkdir(exist_ok=True)
                        environment[key] = str(path)
                    environment["OCTAVE_HISTFILE"] = str(self.temporary_root / "octave-history")
                    if self.native:
                        if not self.qualified_required:
                            environment["MATHLAB_SCOPE_BRIDGE"] = str(self.bundle / "Contents/Resources/native/mathlab_scope_bridge.oct")
                            environment["MATHLAB_INPUT_BRIDGE"] = str(self.bundle / "Contents/Resources/native/mathlab_input_bridge.oct")
                        self.report["checks"]["authentic_input"]["required_for_cli_subset"] = True
                    else:
                        self.report["checks"]["authentic_input"]["reason"] = "Unsupported in this installed profile: input adapter not included; no native execution claimed"
                    if self.value_required and not self.qualified_required:
                        environment["MATHLAB_VALUE_BRIDGE"] = str((self.bundle / VALUE_PATH).resolve())
                    self.runner = Runner(self.directory, self.project, environment, self.timeout, self.output_limit)
                    self.report["commands"] = self.runner.commands
                    def version():
                        stdout, _ = command_ok(self.runner.run("cli-version", [str(self.bundle / CLI_PATH), "--version"]))
                        require(stdout.strip() == "mathlab " + self.manifest["version"], "Packaged CLI version disagrees with manifest")
                        return {"reported_version": stdout.strip()}
                    version_ok = self.check("cli_version", version)
                    ready = self.check("doctor", self.doctor) if version_ok else False
                    self.check("missing_runtime_diagnostic", self.missing_runtime)
                    if ready and self.value_declared:
                        ready = self.check("packaged_value_api", self.packaged_value_api)
                    if ready:
                        for name, action in (("numerical_original_script", self.numerics), ("original_source_error", self.source_error),
                                             ("persistent_repl", self.repl), ("graphics_png", self.graphics),
                                             ("getting_started_example", self.example), ("mat_self_roundtrip", self.mat)):
                            self.check(name, action)
                        if self.native:
                            self.check("authentic_input", self.authentic_input)
                self.report["temporary_project_removed"] = not self.temporary_root.exists()
        except (QAFailure, OSError, ValueError) as error:
            self.report["failure"] = str(error)
        except KeyboardInterrupt:
            self.report["failure"] = "Interrupted; owned process group and temporary project cleanup requested"
        finally:
            if hasattr(self, "temporary_root"):
                self.report["temporary_project_removed"] = not self.temporary_root.exists()
            if before is not None:
                def unchanged():
                    require(inventory(self.bundle) == before, "Installed bundle changed during CLI checks")
                    if "ghostscript" in self.report:
                        selected = self.report["ghostscript"]
                        selected["after"] = qualified_ghostscript_snapshot(selected["selected"])
                        require(selected["before"] == selected["after"], "Selected external Ghostscript changed during CLI checks")
                self.check("bundle_unchanged", unchanged)
            for check in self.report["checks"].values():
                if check["status"] == "not_run":
                    check.setdefault("reason", "A required earlier prerequisite did not pass")
            mandatory = [check for check in self.report["checks"].values() if check["required_for_cli_subset"]]
            passed = not self.report.get("failure") and all(check["status"] == "passed" for check in mandatory)
            self.report["status"] = "passed" if passed else "failed"
            self.report["qualification"]["cli_subset"] = self.report["status"]
            self.report["qualification"]["native_input_coverage"] = self.report["checks"]["authentic_input"]["status"]
            self.report["qualification"]["release_qualified"] = False
            (self.directory / "report.json").write_text(json.dumps(self.report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        return self.report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--report-dir", required=True, type=Path,
                        help="Parent for a new owned run directory; existing reports are preserved")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--octave", help="Explicit external Octave executable; only the packaged CLI launches it")
    selection.add_argument("--startup-config", type=Path, help="Explicit qualified startup seed; native paths are resolved for this installed bundle")
    parser.add_argument("--timeout", type=float, default=90, help="Whole process-group deadline per command, seconds (maximum 600)")
    args = parser.parse_args()
    try:
        report = Acceptance(args.bundle, args.report_dir, args.octave, args.timeout, startup_config=args.startup_config).run()
    except (QAFailure, OSError) as error:
        print("Installed CLI QA could not start: " + str(error), file=sys.stderr)
        return 2
    print("CLI subset " + report["status"] + "; whole-release qualification remains pending.")
    print(str(Path(report["report_directory"]) / "report.json"))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
