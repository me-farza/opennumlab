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
NATIVE_NAMES = ("mathlab_debug_bridge.oct", "mathlab_scope_bridge.oct",
                "mathlab_input_bridge.oct", "runtime-version.txt")
CHECKS = ("bundle_identity", "cli_version", "doctor", "missing_runtime_diagnostic",
          "numerical_original_script", "original_source_error", "persistent_repl",
          "graphics_png", "getting_started_example", "mat_self_roundtrip",
          "authentic_input", "bundle_unchanged")
PENDING = (
    ("desktop_ui", "Actual desktop editing, variable inspection and figure interaction were not exercised."),
    ("stop_and_recovery", "No Stop, interruption or desktop recovery journey was exercised."),
    ("debugger", "No breakpoint, stepping, paused evaluation or debugger recovery was exercised."),
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
        return json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, UnicodeError) as error:
        raise QAFailure("Invalid JSON in " + str(path)) from error


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

    def run(self, label, argv, stdin=None):
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
        return json.loads(text)
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
    def __init__(self, bundle, report_dir, octave=None, timeout=90, output_limit=2 * 1024 * 1024):
        self.bundle = Path(bundle).resolve()
        parent = Path(report_dir).resolve()
        require(not inside(parent, self.bundle), "Report directory must be outside the app bundle")
        require(0 < timeout <= 600, "Timeout must be greater than zero and at most 600 seconds")
        parent.mkdir(parents=True, exist_ok=True)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.directory = parent / ("installed-cli-" + stamp + "-" + uuid.uuid4().hex[:10])
        self.directory.mkdir(mode=0o700)
        self.octave = str(octave) if octave is not None else None
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
                       "checks": {name: {"status": "not_run", "required_for_cli_subset": name != "authentic_input"}
                                  for name in CHECKS}, "commands": [], "artifacts": [],
                       "pending_release_journeys": [{"id": name, "status": "not_run", "required_for_release": True,
                                                     "reason": reason} for name, reason in PENDING],
                       "qualification": {"cli_subset": "not_run", "release_qualified": False,
                                         "native_input_coverage": "not_run"},
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

    def cli(self, label, action, stdin=None, octave_override=None):
        argv = [str(self.bundle / CLI_PATH), "--directory", str(self.project),
                "--timeout", str(max(1, math.ceil(self.timeout))), "--json"]
        selected = octave_override if octave_override is not None else self.octave
        if selected is not None:
            argv.extend(["--octave", str(selected)])
        argv.extend(action)
        return self.runner.run(label, argv, stdin)

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
        identity = {"path": str(runtime), "sha256": digest(runtime), "version": value.get("runtime_version"),
                    "packages": value.get("packages", [])}
        if self.native:
            for key, name in (("input_adapter", "mathlab_input_bridge.oct"), ("scope_adapter", "mathlab_scope_bridge.oct")):
                require(Path(value.get(key) or "").resolve() == self.bundle / "Contents/Resources/native" / name,
                        "Doctor did not activate the bundled " + key)
            expected_runtime = self.manifest["build"]["native"].get("runtime_executable_sha256")
            if expected_runtime:
                require(identity["sha256"] == expected_runtime, "External runtime differs from the native build fingerprint")
        self.report["runtime"] = identity
        return value

    def missing_runtime(self):
        missing = self.project / "deliberately-missing-octave"
        require(not missing.exists(), "Negative-test executable unexpectedly exists")
        stdout, stderr = command_ok(self.cli("missing-runtime", ["doctor"], octave_override=missing), False)
        combined = (stdout + stderr).lower()
        require("octave" in combined and any(text in combined for text in
                ("cannot start", "not found", "no such file", "install", "required", "does not exist")),
                "Missing-runtime failure has no recognizable diagnostic")
        if stdout.strip().startswith("{"):
            require(parse_json(stdout).get("ready") is not True, "Missing-runtime doctor falsely reported ready")
        return {"expected_nonzero_exit": True, "scope": "Explicit missing --octave path; not a clean-machine autodiscovery test"}

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
        require(float(item.get("preview", "nan").strip()) == 42, "REPL workspace did not retain 42")
        return {"json_records": 3, "successive_values": [41, 42], "workspace_value": 42}

    def graphics(self):
        value, _ = self.run_script("qa-graphics", GRAPHICS)
        require("NUMJINN_QA_PLOT_DATA 0,0.5,1;0,0.25,1" in value["output"], "Plot data/property assertion marker is missing")
        variable(value, "qa_plot_t", [1, 3])
        variable(value, "qa_plot_y", [1, 3])
        return {"xdata": [0, 0.5, 1], "ydata": [0, 0.25, 1], "pngs": self.figures(value, "graphics")}

    def example(self):
        installed = self.bundle / "Contents/Resources/examples/getting_started.m"
        path = self.project / "getting_started.m"
        shutil.copyfile(installed, path)
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
                "executed_copy": str(path), "solution": numbers[:2], "pngs": self.figures(value, "getting-started")}

    def mat(self):
        value = getattr(self, "example_value", None)
        require(value is not None, "Getting-started execution did not supply a MAT result")
        match = re.search(r"^Verified MAT round trip: (.+)$", value["output"], re.MULTILINE)
        require(match is not None, "Bundled example did not confirm its MAT equality assertions")
        path = Path(match.group(1).strip())
        require(not path.is_symlink() and path.is_file() and inside(path.resolve(), self.temporary_root),
                "MAT file is outside the owned temporary project/runtime area")
        with path.open("rb") as source:
            header = source.read(128)
        require(len(header) == 128 and header.startswith(b"MATLAB 5.0 MAT-file")
                and header[126:128] in (b"IM", b"MI") and path.stat().st_size > 128, "Expected a nonempty MAT5/v7 container")
        return {"artifact": self.retain(path, "getting-started.mat", self.temporary_root),
                "format_scope": "MAT5 container written with Octave -v7",
                "equality_scope": "Bundled example's same-runtime A/x/y save/load equality assertions",
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
                self.report["provenance"] = {"product": self.manifest.get("product"), "profile": self.manifest["profile"],
                    "version": self.manifest["version"], "source_commit": self.manifest["source"]["commit"],
                    "source_export_sha256": self.manifest["source"]["export_sha256"],
                    "release_json_sha256": before[RELEASE_PATH]["sha256"],
                    "checksums_sha256": before[CHECKSUM_PATH]["sha256"],
                    "executable_sha256": self.manifest["build"]["executable_sha256"],
                    "native_included": self.native}
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
                    environment = os.environ.copy()
                    for key in ("MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE", "MATHLAB_PROJECT_DIR"):
                        environment.pop(key, None)
                    for key, name in (("HOME", "home"), ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp"),
                                      ("XDG_CACHE_HOME", "cache"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data")):
                        path = self.temporary_root / name
                        path.mkdir(exist_ok=True)
                        environment[key] = str(path)
                    environment["OCTAVE_HISTFILE"] = str(self.temporary_root / "octave-history")
                    if self.native:
                        environment["MATHLAB_SCOPE_BRIDGE"] = str(self.bundle / "Contents/Resources/native/mathlab_scope_bridge.oct")
                        environment["MATHLAB_INPUT_BRIDGE"] = str(self.bundle / "Contents/Resources/native/mathlab_input_bridge.oct")
                        self.report["checks"]["authentic_input"]["required_for_cli_subset"] = True
                    else:
                        self.report["checks"]["authentic_input"]["reason"] = "Unsupported in this installed profile: input adapter not included; no native execution claimed"
                    self.runner = Runner(self.directory, self.project, environment, self.timeout, self.output_limit)
                    self.report["commands"] = self.runner.commands
                    def version():
                        stdout, _ = command_ok(self.runner.run("cli-version", [str(self.bundle / CLI_PATH), "--version"]))
                        require(stdout.strip() == "mathlab " + self.manifest["version"], "Packaged CLI version disagrees with manifest")
                        return {"reported_version": stdout.strip()}
                    version_ok = self.check("cli_version", version)
                    ready = self.check("doctor", self.doctor) if version_ok else False
                    self.check("missing_runtime_diagnostic", self.missing_runtime)
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
                self.check("bundle_unchanged", lambda: require(inventory(self.bundle) == before, "Installed bundle changed during CLI checks"))
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
    parser.add_argument("--octave", help="Explicit external Octave executable; only the packaged CLI launches it")
    parser.add_argument("--timeout", type=float, default=90, help="Whole process-group deadline per command, seconds (maximum 600)")
    args = parser.parse_args()
    try:
        report = Acceptance(args.bundle, args.report_dir, args.octave, args.timeout).run()
    except (QAFailure, OSError) as error:
        print("Installed CLI QA could not start: " + str(error), file=sys.stderr)
        return 2
    print("CLI subset " + report["status"] + "; whole-release qualification remains pending.")
    print(str(Path(report["report_directory"]) / "report.json"))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
