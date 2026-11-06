#!/usr/bin/env python3
"""Exercise a relocated normal Rust CLI package, retaining bounded QA evidence.

No Cargo, GUI or external interpreter discovery. Python is a QA-tool prerequisite,
not a dependency of the packaged Rust executable. No publication occurs.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import signal
import struct
import sys

SCRIPTS = Path(__file__).resolve().parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release = load("headless_release_policy", "release-package.py")
installed = load("existing_installed_runner", "qa-installed.py")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def rows(text):
    result = [json.loads(line, object_pairs_hook=release.unique_object, parse_constant=installed.invalid_json_constant) for line in text.splitlines()]
    require(len(result) == 2 and result[0].get("event") == "ready" and result[1].get("event") == "outcome", "Expected exact ready/outcome pair")
    for row in result:
        require(row.get("schema_version") == 4 and row.get("profile") == "class-v1" and row.get("backend") == "rust"
                and row.get("source_graph_complete") is True, "Incorrect explicit Rust schema/profile")
    for row in result:
        key = row.get("key")
        require(isinstance(key, dict) and set(key) == {"session_id", "epoch", "request_id"}
                and all(type(value) is int and value > 0 for value in key.values()), "Missing or malformed request key")
        require(release.valid_sha256(row.get("source_graph_sha256")), "Missing source graph hash")
    require(result[0].get("key") == result[1].get("key") and result[0].get("source_graph_sha256") == result[1].get("source_graph_sha256"), "Stale or mismatched outcome identity")
    return result


def binding(outcome, name):
    values = [row["value"] for row in outcome.get("workspace", []) if row.get("name") == name]
    require(len(values) == 1, "Missing/duplicate workspace binding: " + name)
    return values[0]


def numeric(value, shape, expected):
    require(value.get("storage") == "dense" and value.get("shape") == shape
            and value.get("window_offset") == 0 and value.get("payload_complete") is True,
            "Wrong or clipped numeric descriptor")
    components = value.get("binary64_components")
    require(isinstance(components, list) and len(components) == len(expected), "Wrong numeric word count")
    for actual, desired in zip(components, expected):
        desired = desired if isinstance(desired, tuple) else (desired,)
        require(len(actual) == len(desired), "Wrong real/complex representation")
        for word, target in zip(actual, desired):
            require(isinstance(word, str) and len(word) == 16, "Not a lossless word")
            number = struct.unpack("<d", bytes.fromhex(word)[::-1])[0]
            require(math.isfinite(number) and abs(number - target) <= 1e-12 * max(1, abs(target)), "Numerical result differs")


def healthy(outcome):
    stats = outcome.get("stats", {})
    require(stats.get("ledger_poisoned") is False and outcome.get("events_complete") is True, "Incomplete or poisoned outcome")
    require(stats.get("primitive_steps", 10**12) <= 1_000_000_000 and stats.get("transitions", 10**12) <= 5_000_000, "Fuel escaped profile")
    require(stats.get("peak_live_payload_bytes", 10**12) <= 64 << 20 and stats.get("peak_metadata_bytes", 10**12) <= 16 << 20, "Storage escaped profile")


def checked_command(command, expected_exit=0, stderr_required=False):
    entry, stdout, stderr = command
    entry["expected_exit_code"] = expected_exit
    entry["expectation_status"] = "failed"
    # Reuse the established infrastructure/timeout/output/reason checks. A real
    # expected negative CLI exit remains different from a runner infrastructure error.
    installed.command_ok(command, expected_success=expected_exit == 0)
    require(entry.get("cleanup_status") == "passed", "Installed command cleanup did not complete")
    require(entry.get("status") == ("passed" if expected_exit == 0 else "failed"), "Inconsistent raw Runner status")
    require(entry["exit_code"] == expected_exit and (bool(stderr) if stderr_required else not stderr),
            "Unexpected exact CLI exit or diagnostic")
    entry["expectation_status"] = "passed"
    return stdout, stderr


class ReadyObservation:
    """Bounded hook for existing Runner; does not simulate execution/cancellation."""
    def __init__(self, entry, replacement=None, stop=False):
        self.entry, self.replacement, self.stop = entry, replacement, stop
        self.pending, self.ready = bytearray(), None

    def __call__(self, process, block):
        if self.ready is not None:
            return
        require(len(self.pending) + len(block) <= 32768, "Oversized first ready observation")
        self.pending.extend(block)
        if b"\n" not in self.pending:
            return
        first = bytes(self.pending).split(b"\n", 1)[0]
        self.ready = json.loads(first, object_pairs_hook=release.unique_object)
        require(self.ready.get("event") == "ready" and self.ready.get("schema_version") == 4, "Bad actual ready event")
        if self.replacement is not None:
            self.entry.write_text(self.replacement)
        if self.stop:
            # Runner is the sole synchronous owner/reaper. A live poll leaves
            # this child PID reserved until that owner next polls/waits.
            require(process.poll() is None, "Observed ready belongs to an already exited child")
            os.kill(process.pid, signal.SIGINT)


def accept(bundle, output, timeout=120):
    require(os.name == "posix", "Installed signal/process-group QA currently requires POSIX")
    bundle, requested_output = Path(bundle).resolve(), Path(output).absolute()
    # Resolve all ancestors before any mkdir/report write. Reject aliases rather
    # than following a lexical path whose ancestor can point inside the package.
    output = requested_output.resolve()
    require(output == requested_output, "QA output must be canonical without ancestor aliases")
    require(not output.exists() and not output.is_symlink() and not output.parent.is_symlink(), "QA output must be a fresh owned directory")
    require(not output.is_relative_to(bundle) and not bundle.is_relative_to(output), "QA output must not overlap installed package")
    require(5 <= timeout <= 180, "QA command timeout must be5..180 seconds")
    output.mkdir(parents=True)
    report = {"schema_version": 1, "kind": "headless-installed-qa", "status": "failed", "checks": [], "commands": [],
              "claims": "This exact normal installed CLI journey only; no GUI/general MATLAB/release/publication qualification",
              "pending": ["Hosted CI/platform matrix", "Public-source/security/publication review", "General MATLAB/desktop/full roadmap"]}
    runner = None
    try:
        manifest = release.validate_headless(bundle)
        report.update(source_commit=manifest["source"]["commit"], target=manifest["build"]["target"], binary_sha256=release.sha(bundle / "bin/mathlab"))
        original_inventory = release.inventory(bundle)
        relocated = output / "installed with spaces"
        shutil.copytree(bundle, relocated)
        for path in relocated.rglob("*"):
            path.chmod(0o555 if path.is_dir() or path.stat().st_mode & 0o111 else 0o444)
        relocated.chmod(0o555)
        release.validate_headless(relocated)
        project = output / "owned project"
        project.mkdir()
        home, temp, sentinels, logs = [output / name for name in ("empty-home", "owned-tmp", "sentinels", "logs")]
        for path in (home, temp, sentinels, logs):
            path.mkdir()
        marker = project / "EXTERNAL_INTERPRETER_CALLED"
        for name in ("octave", "octave-cli", "matlab"):
            path = sentinels / name
            path.write_text("#!/bin/sh\nprintf forbidden > '" + str(marker).replace("'", "'\\''") + "'\nexit 91\n")
            path.chmod(0o755)
        environment = {"PATH": str(sentinels), "HOME": str(home), "TMPDIR": str(temp), "XDG_CONFIG_HOME": str(home / "config"),
                       "XDG_CACHE_HOME": str(home / "cache"), "LANG": "C", "LC_ALL": "C", "PYTHONNOUSERSITE": "1",
                       "PYTHONDONTWRITEBYTECODE": "1", "PYTHONUTF8": "1", "MATHLAB_OCTAVE": str(sentinels / "octave-cli")}
        runner = installed.Runner(logs, project, environment, timeout, output_limit=1024 * 1024)
        cli = relocated / "bin/mathlab"

        def command(label, argv, exit_code=0, observer=None):
            stdout, _ = checked_command(runner.run(label, argv, on_stdout=observer), exit_code)
            return stdout

        def program(label, text, modules=(), extras=(), exit_code=0, observer=None):
            entry = project / (label + ".m")
            entry.write_text(text)
            argv = [cli, "--json", "run", "--rust-program", "--profile", "class-v1", "--workspace"]
            for name, content in modules:
                module = project / name
                module.write_text(content)
                argv += ["--module-file", module]
            argv += [*extras, entry]
            pair = rows(command(label, argv, exit_code, observer(entry) if observer else None))
            out = pair[1]
            healthy(out)
            report["checks"].append({"id": label, "exit_code": exit_code, "source_sha256": pair[0]["source"]["sha256"],
                                     "status": out.get("status"), "reason": out.get("reason")})
            return pair, entry

        require(command("version", [cli, "--version"]).strip() == "mathlab " + manifest["version"], "Installed version mismatch")
        require("--rust-program" in command("help", [cli, "run", "--help"]), "Installed help lacks explicit Rust route")
        checked_command(runner.run("invalid-option", [cli, "--definitely-not-an-option"]), 2, stderr_required=True)
        value = "classdef QValue\nproperties\nReading=4\nend\nmethods\nfunction out=changed(obj)\nobj.Reading=10;\nout=obj;\nend\nend\nend\n"
        handle = "classdef QHandle < handle\nproperties\nReading=4\nend\nend\n"
        base = "classdef QBase\nproperties\nReading=4\nend\nmethods\nfunction out=reading(obj)\nout=obj.Reading;\nend\nend\nend\n"
        derived = "classdef QChild < QBase\nmethods\nfunction out=reading(obj)\nout=2*obj.Reading;\nend\nend\nend\n"
        text = ("v=QValue;w=v;w.Reading=9;returned=w.changed();h=QHandle;alias=h;alias.Reading=12;seen=[v.Reading,w.Reading,returned.Reading,h.Reading];base=QBase;child=QChild;dispatch=[base.reading(),child.reading()];"
                "Z=complex([1,2],[3,4]);T=reshape(1:8,[2,2,2]);S=sparse([1,2],[1,2],[3,4],2,2);SX=S*[2;3];"
                "A=[1,0;0,1;1,1];B=[1;2;4];X=A\\B;L=[2,0;0,4]\\[2;8];done=1;")
        pair, _ = program("numeric-class", text, [("QValue.m", value), ("QHandle.m", handle), ("QBase.m", base), ("QChild.m", derived)])
        out = pair[1]
        require(out["status"] == "success", "Installed numerical/class script failed")
        numeric(binding(out, "seen"), [1, 4], [4, 9, 10, 12])
        numeric(binding(out, "dispatch"), [1, 2], [4, 8])
        numeric(binding(out, "Z"), [1, 2], [(1, 3), (2, 4)])
        numeric(binding(out, "T"), [2, 2, 2], list(range(1, 9)))
        require(binding(out, "S")["storage"] == "csc" and binding(out, "h")["storage"] == "object", "CSC/object metadata lost")
        numeric(binding(out, "SX"), [2, 1], [6, 12])
        numeric(binding(out, "X"), [2, 1], [4/3, 7/3])
        numeric(binding(out, "L"), [2, 1], [1, 2])
        for label, text in (("rank-refusal", "prefix=17;bad=[1,2;2,4;3,6]\\[1;2;3];after=1;"), ("wide-refusal", "prefix=17;bad=[1,0,0;0,1,0]\\[1;2];after=1;")):
            refused, _ = program(label, text, exit_code=1)
            require(refused[1]["status"] == "error", "Unsupported solve was not refused")
            numeric(binding(refused[1], "prefix"), [1, 1], [17])
            require(not any(row["name"] in ("bad", "after") for row in refused[1]["workspace"]), "Refused solve published output")
        observer = lambda entry: ReadyObservation(entry, replacement="edited file: original error must remain source-bound\n")
        pair, entry = program("original-error", "prefix=17;missing;after=1;", exit_code=1, observer=observer)
        require(pair[1]["status"] == "error" and pair[1]["error"]["location"]["source_id"] == 1, "Original error location missing")
        require(pair[0]["source"]["sha256"] != hashlib.sha256(entry.read_bytes()).hexdigest(), "Mutable file not replaced after actual ready")
        numeric(binding(pair[1], "prefix"), [1, 1], [17])
        require(not any(row["name"] == "after" for row in pair[1]["workspace"]), "Failed leaf published suffix")
        pair, _ = program("sigint", "seed=17;while 1,seed=seed+1;end", exit_code=130,
                          observer=lambda entry: ReadyObservation(entry, replacement="unexecuted edit\n", stop=True))
        require(pair[1]["reason"] == "UserStop", "SIGINT not mapped to real engine cancellation")
        pair, _ = program("deadline", "seed=17;while 1,seed=seed+1;end", extras=["--timeout", "1"], exit_code=124)
        require(pair[1]["reason"] == "Deadline", "Original deadline not mapped to engine cancellation")
        pair, _ = program("recovery", "recovered=31;")
        numeric(binding(pair[1], "recovered"), [1, 1], [31])
        destination = project / "existing.array"
        destination.write_bytes(b"KEEP USER FILE")
        pair, _ = program("no-clobber", "x=7;", extras=["--export-binding", "x", "--export-file", destination], exit_code=1)
        require(destination.read_bytes() == b"KEEP USER FILE" and pair[1]["export"]["code"] == "DestinationExists", "Export clobbered an existing file")
        pair, _ = program("missing-parent", "x=7;", extras=["--export-binding", "x", "--export-file", project / "absent/output.array"], exit_code=1)
        require(pair[1]["export"]["code"] == "ExportIo" and not (project / "absent").exists(), "Export unexpectedly created parent")
        # Complete installed numerical output, independently checked. No partial
        # workspace preview is accepted as a full fit certificate.
        fit_source = relocated / "share/examples/opennumlab-cli/rust-least-squares.m"
        source_pin = next(row["sha256"] for row in manifest["source"]["files"] if row["path"] == release.HEADLESS_EXAMPLE)
        export = project / "fit.array"
        fit_text = fit_source.read_text()
        pair, fit_entry = program("full-fit", fit_text, extras=["--export-binding", "ls_validation", "--export-file", export])
        require(pair[0]["source"]["sha256"] == source_pin, "Installed example source differs")
        outcome_file = logs / runner.commands[-1]["stdout_log"]  # Exact raw CLI bytes, not reconstructed JSON.
        oracle = SCRIPTS.parent / "crates/mathlab-cli/tests/least_squares_oracle.py"
        oracle_pin = next(row["sha256"] for row in manifest["source"]["files"] if row["path"] == "crates/mathlab-cli/tests/least_squares_oracle.py")
        require(release.sha(oracle) == oracle_pin, "QA must use the package's independently reviewed oracle bytes")
        checked = json.loads(command("full-fit-oracle", [sys.executable, "-B", oracle, "--source", fit_entry,
                     "--expected-source-sha256", source_pin, "--outcome", outcome_file, "--export", export]))
        require(checked.get("status") == "passed" and checked.get("complex_words_checked") == 196864, "Full installed fit oracle failed")
        report["full_fit_oracle"] = checked
        # A second location/cwd proves independence from the initial install path.
        second = output / "second relocated install"
        relocated.rename(second)
        cli = second / "bin/mathlab"
        other = output / "other cwd"
        other.mkdir()
        runner.project = other
        require(command("second-version", [cli, "--version"]).strip() == "mathlab " + manifest["version"], "Second relocation failed")
        require(not marker.exists(), "Explicit Rust route invoked an external interpreter")
        require(release.inventory(bundle) == original_inventory, "Original installed package changed")
        release.validate_headless(second)
        report["checks"].append({"id": "relocation-minimal-env-original-readonly", "status": "passed"})
        report["status"] = "passed"
    except Exception as error:
        report["error"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        if runner is not None:
            report["commands"] = runner.commands
        (output / "acceptance.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New owned evidence directory, outside package")
    parser.add_argument("--timeout", type=int, default=120)
    args = parser.parse_args()
    result = accept(args.bundle, args.output, args.timeout)
    print(json.dumps({key: result.get(key) for key in ("status", "source_commit", "target", "error")}, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError, ValueError) as error:
        raise SystemExit("Headless QA refused: " + str(error))
