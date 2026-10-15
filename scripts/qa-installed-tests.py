#!/usr/bin/env python3
"""Synthetic harness tests only: no real Octave, MATLAB, Cargo or desktop.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("qa_installed", Path(__file__).with_name("qa-installed.py"))
qa = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qa)


FAKE_CLI = r'''import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time
import zlib

mode = os.environ.get("QA_FAKE_MODE", "pass")
if os.environ.get("QA_FAKE_LAUNCH_MARKER"):
    Path(os.environ["QA_FAKE_LAUNCH_MARKER"]).write_text("fake CLI launched")
args = sys.argv[1:]
if args == ["--version"]:
    print("mathlab 0.1.0")
    sys.exit(0)
directory = Path(args[args.index("--directory") + 1])
runtime = Path(args[args.index("--octave") + 1]) if "--octave" in args else Path(os.environ["QA_FAKE_RUNTIME"])
index = 0
while args[index].startswith("--"):
    index += 1 if args[index] == "--json" else 2
action, rest = args[index], args[index + 1:]

def variable(name, shape, preview=""):
    return {"name":name,"class_name":"double","dimensions":shape,"bytes":8,"preview":preview}

def result(output="", workspace=None, figures=None, error=None):
    return {"output":output,"output_truncated":False,"output_dropped_bytes":0,"output_decoding_loss":False,
            "error":error,"workspace":workspace or [],"figures":figures or [],
            "working_directory":str(directory),"duration_ms":1,"figure_warnings":[]}

def png():
    target = directory / ".mathlab/figures/figure-1.png"
    target.parent.mkdir(parents=True, exist_ok=True)
    def chunk(kind, data):
        return struct.pack(">I",len(data))+kind+data+struct.pack(">I",zlib.crc32(kind+data)&0xffffffff)
    data=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",2,2,8,2,0,0,0))
    data+=chunk(b"IDAT",zlib.compress((b"\x00"+b"\x00\x80\xff"*2)*2))+chunk(b"IEND",b"")
    target.write_bytes(data if mode != "bad-png" else b"not a PNG")
    return [{"path":str(target),"title":"synthetic image"}]

if action == "doctor":
    if not runtime.exists():
        print("mathlab: Cannot start the selected Octave executable: No such file",file=sys.stderr)
        sys.exit(1)
    if mode == "doctor-nonzero":
        print(json.dumps({"ready":True}))
        sys.exit(9)
    if mode == "doctor-output-flood":
        print("x"*100000)
        time.sleep(10)
    if mode == "doctor-timeout":
        child = "import pathlib,time; time.sleep(1); pathlib.Path("+repr(os.environ["QA_FAKE_CHILD_MARKER"])+").write_text('survived')"
        subprocess.Popen([sys.executable,"-c",child])
        time.sleep(10)
    print(json.dumps({"application":"MathLab","version":"0.1.0","ready":True,"runtime":str(runtime),
          "runtime_version":"GNU Octave version 11.3.0","working_directory":str(directory),"packages":[],
          "scope_adapter":os.environ.get("MATHLAB_SCOPE_BRIDGE"),"input_adapter":os.environ.get("MATHLAB_INPUT_BRIDGE")}))
elif action == "repl":
    text=sys.stdin.read()
    assert "qa_session_seed=41" in text and ":workspace" in text and ":quit" in text
    print(json.dumps(result("NUMJINN_QA_REPL_FIRST 41\n",[variable("qa_session_seed",[1,1],"41")])))
    error={"identifier":"Fake:REPL","message":"synthetic execution failure","stack":[]} if mode=="repl-error" else None
    print(json.dumps(result("NUMJINN_QA_REPL_SECOND 42\n",[variable("qa_session_seed",[1,1],"42")],error=error)))
    print(json.dumps({"workspace":[variable("qa_session_seed",[1,1],"42")]}))
elif action == "run":
    source=Path(rest[0])
    assert source.parent == directory and source.is_file()
    if source.name == "qa-numerics.m":
        answer="NUMJINN_QA_NUMERICS 2 4 0 42\n" if mode=="wrong-numerics" else "NUMJINN_QA_NUMERICS 2 3 0 42\n"
        print(json.dumps(result(answer,[variable("qa_x",[2,1])])) )
    elif source.name == "qa-source-error.m":
        print(json.dumps(result(error={"identifier":"NumJinnQA:OriginalSource","message":"NUMJINN_QA_ORIGINAL_SOURCE_ERROR",
                                      "stack":[{"file":str(source),"name":"qa-source-error","line":2}]})))
        sys.exit(1)
    elif source.name == "qa-graphics.m":
        print(json.dumps(result("NUMJINN_QA_PLOT_DATA 0,0.5,1;0,0.25,1\n",
                                [variable("qa_plot_t",[1,3]),variable("qa_plot_y",[1,3])],png())))
    elif source.name == "getting_started.m":
        mat=Path(os.environ["TMPDIR"])/"synthetic-example.mat"
        header=b"MATLAB 5.0 MAT-file synthetic harness output".ljust(124,b" ")+b"\x00\x01IM"
        mat.write_bytes(header+b"synthetic same-runtime fixture result")
        output="Solution: x = [2.000000; 3.000000], residual = 0\nVerified MAT round trip: "+str(mat)+"\n"
        print(json.dumps(result(output,[variable("x",[2,1]),variable("t",[1,501]),variable("y",[1,501])],png())))
    elif source.name == "qa-input.m":
        assert os.environ.get("MATHLAB_INPUT_BRIDGE")
        assert sys.stdin.read()=="  λ exact  \nqa_base+2\n\n"
        output="NUMJINN_QA_NAME>   λ exact  \nNUMJINN_QA_VALUE> qa_base+2\nNUMJINN_QA_EMPTY> \nNUMJINN_QA_INPUT_OK 42\n"
        print(json.dumps(result(output,[variable("qa_value",[1,1])])) )
    else:
        raise RuntimeError("Unknown synthetic source")
else:
    raise RuntimeError("Unknown synthetic command")
'''


class InstalledQATests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="numjinn-installed-harness-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.bundle = self.root / "Installed fake app.app"
        self.reports = self.root / "reports"
        self.runtime = self.root / "external-fake-octave"
        self.runtime.write_text("Synthetic runtime identity only; never executed\n")
        self.runtime.chmod(0o755)
        self.write(qa.CLI_PATH, "#!" + sys.executable + "\n" + FAKE_CLI, executable=True)
        self.write(qa.DESKTOP_PATH, "Never launch this synthetic desktop\n", executable=True)
        self.write("Contents/Resources/examples/getting_started.m", "% synthetic bundled original example\n")
        for name, data in (("Cargo.toml", "[workspace.package]\nversion=\"0.1.0\"\n"),
                           ("Cargo.lock", "# synthetic lock\n"), ("LICENSE", "synthetic notice\n"),
                           ("crates/mathlab-cli/src/main.rs", "// corresponding fake source\n"),
                           ("octave/mathlab_worker.m", "% corresponding fake worker\n"),
                           ("examples/getting_started.m", "% synthetic bundled original example\n")):
            self.write("Contents/Resources/source/" + name, data)
        self.environment = {"QA_FAKE_MODE": "pass", "QA_FAKE_RUNTIME": str(self.runtime)}
        self.manifest = {"schema_version": 1, "product": "NumJinn", "profile": "Development alpha", "version": "0.1.0",
                         "source": {"commit": "a" * 40, "dirty": False, "frozen_export": True},
                         "distribution": {"clean_candidate": True},
                         "build": {"native": {"status": "not_included"}}}
        self.seal()

    def write(self, name, data, executable=False):
        path = self.bundle / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data, encoding="utf-8")
        if executable:
            path.chmod(0o755)
        return path

    def seal(self):
        for name in (qa.RELEASE_PATH, qa.CHECKSUM_PATH):
            path = self.bundle / name
            if path.exists():
                path.unlink()
        source = qa.inventory(self.bundle / "Contents/Resources/source")
        self.manifest["source"]["files"] = [source[name] for name in sorted(source)]
        self.manifest["source"]["export_sha256"] = hashlib.sha256(
            json.dumps(self.manifest["source"]["files"], sort_keys=True).encode()).hexdigest()
        self.manifest["build"]["executable_sha256"] = {
            "mathlab": qa.digest(self.bundle / qa.CLI_PATH), "mathlab-desktop": qa.digest(self.bundle / qa.DESKTOP_PATH)}
        payload = qa.inventory(self.bundle)
        self.manifest["payload"] = [payload[name] for name in sorted(payload)]
        self.write(qa.RELEASE_PATH, json.dumps(self.manifest, indent=2) + "\n")
        actual = qa.inventory(self.bundle)
        self.write(qa.CHECKSUM_PATH, "".join(actual[name]["sha256"] + "  " + name + "\n" for name in sorted(actual)))

    def native(self, omit=None):
        hashes = {}
        for name in qa.NATIVE_NAMES:
            if name != omit:
                path = self.write("Contents/Resources/native/" + name, "synthetic adapter " + name)
                hashes[name] = qa.digest(path)
        self.manifest["build"]["native"] = {"status": "enabled_protocol_load_passed", "artifact_sha256": hashes,
                                              "runtime_executable_sha256": qa.digest(self.runtime)}
        self.seal()

    def run_qa(self, mode="pass", timeout=5, output_limit=2 * 1024 * 1024, octave=None):
        environment = {**self.environment, "QA_FAKE_MODE": mode}
        with patch.dict(os.environ, environment):
            acceptance = qa.Acceptance(self.bundle, self.reports, str(octave or self.runtime), timeout, output_limit)
            report = acceptance.run()
        self.assertTrue((acceptance.directory / "report.json").is_file())
        return report

    def test_cli_subset_pass_keeps_release_and_native_coverage_pending(self):
        before = qa.inventory(self.bundle)
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        self.assertEqual(report["scope"], "installed_cli_only")
        self.assertFalse(report["qualification"]["release_qualified"])
        self.assertEqual(report["qualification"]["native_input_coverage"], "not_run")
        self.assertTrue(all(item["status"] == "not_run" for item in report["pending_release_journeys"]))
        self.assertFalse(report["checks"]["mat_self_roundtrip"]["evidence"]["matlab_interchange_established"])
        project = Path(report["temporary_project"])
        self.assertFalse(project.exists())
        self.assertFalse(qa.inside(project, self.bundle))
        self.assertFalse(qa.inside(project, qa.SOURCE_ROOT))
        self.assertTrue(report["temporary_project_removed"])
        self.assertEqual(qa.inventory(self.bundle), before)
        self.assertTrue(all(command["argv"][0] == str(self.bundle / qa.CLI_PATH) for command in report["commands"]))
        self.assertTrue(all(command["working_directory"] == str(project) for command in report["commands"]))
        self.assertEqual(report["checks"]["graphics_png"]["evidence"]["pngs"][0]["width"], 2)
        self.assertEqual(report["checks"]["persistent_repl"]["evidence"]["successive_values"], [41, 42])
        expected_tool = {"script_path": str(Path(qa.__file__).absolute()),
                         "script_sha256": qa.digest(Path(qa.__file__)),
                         "python_executable": sys.executable, "python_version": sys.version}
        self.assertEqual(report["tool_identity"], expected_tool)
        stored = json.loads((Path(report["report_directory"]) / "report.json").read_text())
        self.assertEqual(stored["tool_identity"], expected_tool)
        for command in report["commands"]:
            for kind in ("stdout", "stderr"):
                path = Path(report["report_directory"]) / command[kind + "_log"]
                self.assertEqual(qa.digest(path), command[kind + "_sha256"])

    def test_supplied_octave_symlink_is_forwarded_without_dereferencing(self):
        alias = self.root / "external-octave-launcher"
        alias.symlink_to(self.runtime)
        report = self.run_qa(octave=alias)
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        for command in report["commands"]:
            if command["id"] not in ("cli-version", "missing-runtime"):
                index = command["argv"].index("--octave")
                self.assertEqual(command["argv"][index + 1], str(alias))
        self.assertEqual(report["checks"]["doctor"]["evidence"]["runtime"], str(alias))
        self.assertEqual(report["runtime"]["path"], str(self.runtime.resolve()))
        self.assertEqual(report["runtime"]["sha256"], qa.digest(self.runtime))

    def test_included_input_is_required_and_uses_the_bundled_adapter(self):
        self.native()
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        self.assertTrue(report["checks"]["authentic_input"]["required_for_cli_subset"])
        self.assertEqual(report["qualification"]["native_input_coverage"], "passed")
        self.assertFalse(report["qualification"]["release_qualified"])

    def test_dirty_and_missing_source_identities_block_all_execution(self):
        self.manifest["source"]["dirty"] = True
        self.manifest["distribution"]["clean_candidate"] = False
        self.seal()
        report = self.run_qa()
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed")
        self.assertEqual(report["commands"], [])
        self.manifest["source"].update(dirty=False, commit="unavailable")
        self.manifest["distribution"]["clean_candidate"] = True
        self.seal()
        report = self.run_qa()
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["commands"], [])

    def test_payload_or_final_checksum_tampering_blocks_execution(self):
        self.write("Contents/Resources/source/Cargo.lock", "tampered")
        report = self.run_qa()
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["commands"], [])
        self.seal()
        self.write(qa.CHECKSUM_PATH, "0" * 64 + "  Contents/MacOS/mathlab\n")
        report = self.run_qa()
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["commands"], [])

    def test_missing_advertised_adapter_blocks_execution(self):
        self.native(omit="mathlab_input_bridge.oct")
        report = self.run_qa()
        self.assertEqual(report["status"], "failed")
        self.assertIn("Missing or mismatched advertised native adapter", report["checks"]["bundle_identity"]["reason"])
        self.assertEqual(report["commands"], [])

    def test_nonzero_doctor_cannot_pass_with_a_ready_true_payload(self):
        report = self.run_qa("doctor-nonzero")
        self.assertEqual(report["checks"]["doctor"]["status"], "failed")
        self.assertEqual(report["checks"]["numerical_original_script"]["status"], "not_run")
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["checks"]["missing_runtime_diagnostic"]["status"], "passed")

    def test_wrong_numerical_oracle_fails_despite_zero_exit(self):
        report = self.run_qa("wrong-numerics")
        self.assertEqual(report["checks"]["numerical_original_script"]["status"], "failed")
        self.assertEqual(report["status"], "failed")

    def test_external_native_adapter_environment_cannot_leak_into_an_omitted_profile(self):
        self.environment.update(MATHLAB_SCOPE_BRIDGE="/not-the-bundle/scope.oct",
                                MATHLAB_INPUT_BRIDGE="/not-the-bundle/input.oct",
                                MATHLAB_DEBUG_BRIDGE="/not-the-bundle/debug.oct")
        report = self.run_qa()
        self.assertEqual(report["status"], "passed")
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertIsNone(doctor["input_adapter"])
        self.assertIsNone(doctor["scope_adapter"])

    def test_repl_execution_error_fails_even_when_the_process_exits_zero(self):
        report = self.run_qa("repl-error")
        self.assertEqual(report["checks"]["persistent_repl"]["status"], "failed")
        self.assertEqual(report["status"], "failed")

    def test_invalid_png_fails_graphics_and_installed_example(self):
        report = self.run_qa("bad-png")
        self.assertEqual(report["checks"]["graphics_png"]["status"], "failed")
        self.assertEqual(report["checks"]["getting_started_example"]["status"], "failed")
        self.assertEqual(report["status"], "failed")

    def test_timeout_kills_the_complete_owned_child_process_group(self):
        marker = self.root / "child-survived"
        self.environment["QA_FAKE_CHILD_MARKER"] = str(marker)
        report = self.run_qa("doctor-timeout", timeout=0.3)
        self.assertEqual(report["checks"]["doctor"]["status"], "failed", json.dumps(report, indent=2))
        command = next(item for item in report["commands"] if item["id"] == "doctor")
        self.assertTrue(command["timed_out"])
        time.sleep(1.05)
        self.assertFalse(marker.exists(), "the spawned child survived its process-group deadline")
        self.assertFalse(Path(report["temporary_project"]).exists())

    def test_output_flood_is_bounded_and_cannot_qualify(self):
        report = self.run_qa("doctor-output-flood", output_limit=4096)
        command = next(item for item in report["commands"] if item["id"] == "doctor")
        self.assertTrue(command["output_limit_exceeded"])
        self.assertLessEqual(command["stdout_bytes"] + command["stderr_bytes"], 4096)
        self.assertEqual(report["status"], "failed")

    def test_cleanup_failure_retains_exit_and_log_evidence_and_fails_qualification(self):
        real_cleanup = qa.kill_group
        for error_number in (errno.EPERM, errno.EIO):
            with self.subTest(errno=error_number):
                def failed_cleanup(process):
                    result = real_cleanup(process)
                    if process.poll() is not None:
                        raise OSError(error_number, "synthetic process-group cleanup failure")
                    return result

                # Real groups are still cleaned; the injected error must also reach finalization.
                with patch.object(qa, "kill_group", side_effect=failed_cleanup):
                    report = self.run_qa()
                self.assertEqual(report["status"], "failed")
                self.assertEqual(report["qualification"]["cli_subset"], "failed")
                self.assertFalse(report["qualification"]["release_qualified"])
                for name in ("cli_version", "missing_runtime_diagnostic"):
                    self.assertEqual(report["checks"][name]["status"], "failed")
                    self.assertIn("Command cleanup failed", report["checks"][name]["reason"])
                self.assertTrue(report["temporary_project_removed"])
                directory = Path(report["report_directory"])
                stored = json.loads((directory / "report.json").read_text())
                self.assertEqual(stored["commands"], report["commands"])
                self.assertEqual({command["id"] for command in report["commands"]}, {"cli-version", "missing-runtime"})
                for command in report["commands"]:
                    self.assertEqual(command["exit_code"], 0 if command["id"] == "cli-version" else 1)
                    self.assertEqual(command["status"], "failed")
                    self.assertEqual(command["cleanup_status"], "failed")
                    self.assertEqual(command["process_group_cleanup"], "failed")
                    self.assertFalse(command["timed_out"])
                    self.assertFalse(command["output_limit_exceeded"])
                    self.assertGreaterEqual(command["elapsed_seconds"], 0)
                    final_errors = [error for error in command["cleanup_errors"] if error["stage"] == "final_process_group"]
                    self.assertEqual(len(final_errors), 1)
                    self.assertEqual(final_errors[0]["errno"], error_number)
                    self.assertEqual(final_errors[0]["error_type"], "PermissionError" if error_number == errno.EPERM else "OSError")
                    self.assertIn("synthetic process-group cleanup failure", final_errors[0]["message"])
                    for kind in ("stdout", "stderr"):
                        path = directory / command[kind + "_log"]
                        self.assertEqual(command[kind + "_sha256"], qa.digest(path))
                        self.assertEqual(command[kind + "_bytes"], path.stat().st_size)
                    with self.assertRaises(ProcessLookupError):
                        os.killpg(command["pid"], 0)

    def test_reports_preserve_existing_files_and_cannot_be_inside_bundle(self):
        self.reports.mkdir()
        unrelated = self.reports / "keep-me"
        unrelated.write_text("unrelated user data")
        first, second = self.run_qa(), self.run_qa()
        self.assertNotEqual(first["report_directory"], second["report_directory"])
        self.assertEqual(unrelated.read_text(), "unrelated user data")
        before = qa.inventory(self.bundle)
        with self.assertRaisesRegex(qa.QAFailure, "outside the app bundle"):
            qa.Acceptance(self.bundle, self.bundle / "reports")
        self.assertEqual(qa.inventory(self.bundle), before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
