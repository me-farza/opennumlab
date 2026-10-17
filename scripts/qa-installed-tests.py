#!/usr/bin/env python3
"""Synthetic harness tests only: no real Octave, MATLAB, Cargo or desktop.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import errno
import copy
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


FAKE_CLI = r'''import hashlib
import json
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
expected_value = Path(__file__).resolve().parents[1] / "Resources/native/mathlab_value_bridge.oct"
qualified = os.environ.get("QA_FAKE_EXPECT_QUALIFIED") == "required"
graphics = os.environ.get("QA_FAKE_EXPECT_GRAPHICS") == "required"
if not qualified and "MATHLAB_VALUE_BRIDGE" in os.environ:
    if os.environ.get("QA_FAKE_EXPECT_VALUE") != "required" or os.environ["MATHLAB_VALUE_BRIDGE"] != str(expected_value):
        print("Synthetic CLI rejected inherited MATHLAB_VALUE_BRIDGE", file=sys.stderr)
        sys.exit(97)
elif not qualified and os.environ.get("QA_FAKE_EXPECT_VALUE") == "required":
    print("Synthetic CLI did not receive the required installed value adapter", file=sys.stderr)
    sys.exit(98)
args = sys.argv[1:]
if args == ["--version"]:
    print("mathlab 0.1.0")
    sys.exit(0)
directory = Path(args[args.index("--directory") + 1])
startup = None
if qualified:
    assert "--startup-config" in args and "--octave" not in args, "qualified fixture requires exclusive config routing"
    assert all(key not in os.environ for key in ("MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE", "MATHLAB_DEBUG_BRIDGE",
                                                "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE", "MATHLAB_VALUE_BRIDGE",
                                                "GSC", "GNUPLOT"))
    config_path = Path(args[args.index("--startup-config") + 1])
    assert config_path.is_absolute() and config_path.resolve() == config_path
    assert not config_path.is_relative_to(expected_value.parents[3])
    document = json.loads(config_path.read_text())
    assert document["schema_version"] == 2 and document["profile"] == ("qualified-graphics-v2" if graphics else "qualified-core-v2")
    startup = document["startup"]
    external = json.loads(os.environ["QA_FAKE_STARTUP_EXTERNAL"])
    for key in ("installation_root", "gnuplot", "ghostscript", "runtime_directories", "octave_version", "octave_api_version"):
        assert startup[key] == external[key], "external startup selection was changed"
    runtime = Path(startup["executable"]["path"])
    assert startup["executable"]["sha256"] == external["executable"]["sha256"]
    assert str(runtime) == external["executable"]["path"] or (not runtime.exists() and runtime.name == "deliberately-missing-octave")
    for pin, name in [(startup["provider"], "mathlab_intrinsic_bridge.oct")] + [
            (adapter["file"], "mathlab_" + adapter["role"] + "_bridge.oct") for adapter in startup["adapters"]]:
        assert pin == {"path":str(expected_value.parent / name),
                       "sha256":hashlib.sha256((expected_value.parent / name).read_bytes()).hexdigest()}, "native pin was not rebased to installed bundle"
else:
    assert "--startup-config" not in args
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

def qualified_doctor():
    names = ("mathlab_intrinsic_bridge.oct", "mathlab_input_bridge.oct", "mathlab_scope_bridge.oct",
             "mathlab_value_bridge.oct", "mathlab_debug_bridge.oct")
    if graphics: names += ("mathlab_graphics_bridge.oct",)
    pins = [startup["provider"]] + [adapter["file"] for adapter in startup["adapters"]]
    modules = {Path(pin["path"]).name:dict(pin) for pin in pins}
    assert set(modules) == set(names)
    context = {"worker_id":"synthetic-installed-worker", "generation":"1", "invocation":"1", "revision":"0"}
    ready_protocols = (("input","mathlab-input","1"),("scope","mathlab-scope","2"),
                       ("value","mathlab-value","1"),("debug","mathlab-debugger","4"))
    if graphics: ready_protocols += (("graphics","mathlab-graphics","1"),)
    intrinsic = {"protocol":"mathlab-intrinsics", "version":"1", "phase":"ready",
        "provider_file":modules["mathlab_intrinsic_bridge.oct"]["path"],
        "worker_file":str(directory / "private-session/mathlab_worker.m"),
        "registry_file":str(directory / "private-session/registry/mathlab_scope_register.m"),
        "entry_alias":"__mlentry_synthetic", "shutdown_alias":"__mlshutdown_synthetic",
        "octave_version":"11.3.0", "octave_api_version":"api-v61",
        "initial_path_admitted":True, "shutdown_registered":True,
        "adapters":{role:{"path":modules["mathlab_" + role + "_bridge.oct"]["path"],
                          "protocol":protocol,"version":version}
                    for role,protocol,version in ready_protocols}}
    return {"native_modules":modules,"session_context":context,
            "qualified_startup_status":{"profile":"qualified-graphics-v2" if graphics else "qualified-core-v2",
                                        "context":dict(context),"intrinsics":intrinsic}}

if action == "doctor":
    if not runtime.exists():
        print("mathlab: Cannot start the selected Octave executable: No such file",file=sys.stderr)
        sys.exit(1)
    if mode == "doctor-nonzero":
        print(json.dumps({"ready":True}))
        sys.exit(9)
    if mode == "doctor-output-flood":
        # Exercise pipe output bounds independently of the interpreter's buffer size.
        print("x"*100000, flush=True)
        time.sleep(10)
    if mode == "doctor-timeout":
        child = "import pathlib,time; time.sleep(1); pathlib.Path("+repr(os.environ["QA_FAKE_CHILD_MARKER"])+").write_text('survived')"
        subprocess.Popen([sys.executable,"-c",child])
        time.sleep(10)
    capabilities = ["typed_nd_page.v1", "atomic_range_edit.v1"]
    if qualified or os.environ.get("MATHLAB_VALUE_BRIDGE") or mode == "undeclared-value-capability":
        capabilities.append("dense_complex_native_components.v1")
    if graphics: capabilities.append("graphics_registry.v1")
    doctor = {"application":"MathLab","version":"0.1.0","ready":True,"runtime":str(runtime),
          "runtime_version":"GNU Octave version 11.3.0","working_directory":str(directory),"packages":[],
          "scope_adapter":os.environ.get("MATHLAB_SCOPE_BRIDGE"),"input_adapter":os.environ.get("MATHLAB_INPUT_BRIDGE"),
          "value_adapter":os.environ.get("MATHLAB_VALUE_BRIDGE"),"worker_capabilities":capabilities}
    if qualified:
        for role in ("scope", "input", "value"):
            doctor[role + "_adapter"] = next(adapter["file"]["path"] for adapter in startup["adapters"] if adapter["role"] == role)
    if mode == "missing-value-path":
        doctor.pop("value_adapter")
    elif mode == "wrong-value-path":
        doctor["value_adapter"] = "/not-the-installed-bundle/mathlab_value_bridge.oct"
    elif mode == "noncanonical-value-path":
        doctor["value_adapter"] = str(expected_value.parent) + "/../native/mathlab_value_bridge.oct"
    elif mode == "missing-value-capabilities":
        doctor.pop("worker_capabilities")
    elif mode == "missing-runtime-path":
        doctor.pop("runtime")
    elif mode == "incomplete-value-capabilities":
        doctor["worker_capabilities"] = ["typed_nd_page.v1", "atomic_range_edit.v1"]
    elif mode == "duplicate-value-capabilities":
        doctor["worker_capabilities"].append(doctor["worker_capabilities"][0])
    elif mode == "invalid-value-capabilities":
        doctor["worker_capabilities"] = ["typed_nd_page.v1", 7, "dense_complex_native_components.v1"]
    elif mode == "relative-runtime-path":
        doctor["runtime"] = os.path.relpath(runtime, directory)
    elif mode == "legacy-doctor-fields":
        doctor.pop("value_adapter")
        doctor.pop("worker_capabilities")
    if qualified:
        doctor.update(qualified_doctor())
        if mode == "qualified-missing-status":
            doctor.pop("qualified_startup_status")
        elif mode == "qualified-missing-provider":
            doctor["native_modules"].pop("mathlab_intrinsic_bridge.oct")
        elif mode == "qualified-foreign-provider":
            foreign = "/foreign/native/mathlab_intrinsic_bridge.oct"
            doctor["native_modules"]["mathlab_intrinsic_bridge.oct"]["path"] = foreign
            doctor["qualified_startup_status"]["intrinsics"]["provider_file"] = foreign
        elif mode == "qualified-wrong-provider-hash":
            doctor["native_modules"]["mathlab_intrinsic_bridge.oct"]["sha256"] = "f" * 64
        elif mode == "qualified-context-mismatch":
            doctor["session_context"]["revision"] = "1"
        elif mode == "qualified-downgraded-status":
            doctor["qualified_startup_status"]["profile"] = "four-adapter-value-v1"
        elif mode == "qualified-extra-status":
            doctor["qualified_startup_status"]["extra"] = True
        elif mode == "qualified-foreign-adapter":
            doctor["qualified_startup_status"]["intrinsics"]["adapters"]["input"]["path"] = "/foreign/input.oct"
        elif mode == "qualified-alias-mismatch":
            doctor["qualified_startup_status"]["intrinsics"]["shutdown_alias"] = "__mlshutdown_other"
        elif mode == "qualified-foreign-runtime":
            doctor["runtime"] = startup["gnuplot"]["path"]
        elif mode == "graphics-missing-capability":
            capabilities.remove("graphics_registry.v1")
        elif mode == "graphics-missing-module":
            doctor["native_modules"].pop("mathlab_graphics_bridge.oct")
        elif mode == "graphics-ready-numeric-version":
            doctor["qualified_startup_status"]["intrinsics"]["adapters"]["graphics"]["version"] = 1
        elif mode == "graphics-foreign-adapter":
            doctor["qualified_startup_status"]["intrinsics"]["adapters"]["graphics"]["path"] = "/foreign/graphics.oct"
    if mode == "unexpected-qualified-status":
        doctor["qualified_startup_status"] = {"profile":"qualified-core-v2"}
    print(json.dumps(doctor))
elif action == "probe-value-api":
    probe_mode = os.environ.get("QA_FAKE_PROBE_MODE", "pass")
    if probe_mode == "must-not-run":
        raise RuntimeError("Legacy profile unexpectedly invoked the new probe")
    required = "--expect-unavailable" not in rest
    variables = {"value_adapter":("MATHLAB_VALUE_BRIDGE","mathlab_value_bridge.oct"),
                 "scope_adapter":("MATHLAB_SCOPE_BRIDGE","mathlab_scope_bridge.oct"),
                 "input_adapter":("MATHLAB_INPUT_BRIDGE","mathlab_input_bridge.oct"),
                 "debug_adapter":("MATHLAB_DEBUG_BRIDGE","mathlab_debug_bridge.oct")}
    if qualified:
        assert required
        assert all(variable not in os.environ for variable, _ in variables.values())
    elif required:
        for variable, filename in variables.values():
            assert os.environ.get(variable) == str(expected_value.parent / filename)
    else:
        assert all(variable not in os.environ for variable, _ in variables.values())
    def observation(path):
        return {"display":str(path),"absolute":True,"path_utf8":True,"display_is_lossy":False}
    def file_hash(path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    cli = Path(__file__).resolve()
    canonical_runtime = runtime.resolve()
    workspace = Path(os.environ["TMPDIR"]) / "synthetic-value-api-workspace"
    capability = "dense_complex_native_components.v1"
    capabilities = ["typed_nd_page.v1", "atomic_range_edit.v1"] + ([capability] if required else [])
    if graphics: capabilities.append("graphics_registry.v1")
    value_path = str(expected_value) if required else None
    adapter = {"path":value_path,"path_observation":observation(value_path) if required else None,
               "capability":capability,"capability_observed":required,"configured_environment_path":value_path,
               "configured_environment_observation":observation(value_path) if required else None}
    if qualified:
        adapter.update(configured_environment_path=None, configured_environment_observation=None,
                       configured_startup_path=value_path, configured_startup_observation=observation(value_path))
    configured = [("cli_executable",None,cli),("octave_runtime",None,canonical_runtime)]
    if required:
        configured += [(kind,variable,expected_value.parent/filename) for kind,(variable,filename) in variables.items()]
    if qualified:
        configured = [(kind, "startup_config" if variable else None, path) for kind,variable,path in configured]
        configured.append(("intrinsic_provider","startup_config",expected_value.parent/"mathlab_intrinsic_bridge.oct"))
        if graphics:
            configured.append(("graphics_adapter","startup_config",expected_value.parent/"mathlab_graphics_bridge.oct"))
    identities = [{"kind":kind,"configured_by":variable,"path":str(path),"sha256_before":file_hash(path),
                   "sha256_after":file_hash(path),"unchanged":True,"error":None} for kind,variable,path in configured]
    preflight = [{"environment_variable":variable,"canonical_path":str(path),"identity_verified":True,
                  "abi_or_protocol_qualified":False,"requested_path":observation(path)}
                 for kind,variable,path in configured if variable]
    if qualified:
        preflight = []
    cases = [{"id":"runtime_profile","status":"passed","error_identifier":None,"details":{"adapter":adapter}},
             {"id":"uint64_checked_read_write_undo_redo","status":"passed","error_identifier":None,
              "details":{"class":"uint64","journey":"read-write-undo-redo","native_adapter_required":False,
                         "exact_terminal_integer_oracle":True,"original_alias_preserved":True,
                         "untouched_cell_and_peer_preserved":True,
                         "old_values":["9007199254740993","18446744073709551615","17"],
                         "new_values":["18446744073709551615","9007199254740995","17"]}}]
    if required:
        for precision in ("double","single"):
            cases.append({"id":precision+"_checked_read_write_undo_redo","status":"passed","error_identifier":None,
                          "details":{"class":precision,"journeys":[{"selected_rows":rows,"selected_columns":1,
                              "journey":journey,"exact_final_bit_oracle":True} for rows in (1,2)
                              for journey in ("write-only","write-undo","write-undo-redo")]}})
        cases += [{"id":"stale_identity_rejected","status":"passed","error_identifier":None,"details":{
                   "stale_page_error_identifier":"MathLab:StaleBinding","stale_write_error_identifier":"MathLab:StaleBinding",
                   "stale_binding_history_error_identifier":"MathLab:StaleBinding",
                   "retired_history_error_identifier":"MathLab:ValueEditHistoryConflict","automatic_retry_performed_by_probe":False}},
                  {"id":"restart_preserves_native_profile","status":"passed","error_identifier":None,"details":{
                   "adapter":adapter,"profile_reinstalled":True,"workspace_cleared":True,
                   "fresh_workspace_variable_count":0,"old_token_error_identifier":"MathLab:StaleBinding"}},
                  {"id":"same_process_stop_recovers","status":"passed","error_identifier":None,"details":{
                   "adapter":adapter,"observed_stop_identifier":"MathLab:Interrupted","source_start_count":1,
                   "continuation_absent":True,"old_authority_rejected":True,"commit_window_test":False}}]
    else:
        for case_id,error in (("complex_read_requires_native","MathLab:ValuePagePrecisionUnavailable"),
                              ("complex_write_requires_native","MathLab:ValueEditPrecisionUnavailable")):
            cases.append({"id":case_id,"status":"passed","error_identifier":error,"details":{
                "expected_error_identifier":error,"observed_error_identifier":error,"native_precision_success":False,
                "unchanged_values":True,"exact_final_component_bits":True}})
    probe = {"schema_version":1,"kind":"mathlab.value-api-probe","profile":"native-required" if required else "native-unavailable",
             "status":"passed","error":None,"build":{"package_version":"0.1.0","revision":"synthetic",
             "source_sha256":"c"*64,"target":"synthetic-target","executable":str(cli),"binary_sha256":file_hash(cli),
             "executable_identity_verified":True,"binary_identity_verified":True,"executable_identity_error":None,
             "binary_identity_error":None,"executable_observation":observation(cli)},"runtime":str(canonical_runtime),
             "runtime_details":{"canonical_identity_verified":True,"started":True,"temporary_workspace":True,
             "process_id":123,"version":"11.3.0","requested_runtime":observation(canonical_runtime),
             "working_directory":str(workspace),"working_directory_observation":observation(workspace)},
             "value_adapter":value_path,"adapter_details":adapter,"adapter_preflight":preflight,
             "worker_capabilities":capabilities,"worker_protocol":{"name":"mathlab-worker","version":1,"capabilities":capabilities.copy()},
             "artifact_identities":identities,"cases":cases,
             "limits":{"page_cells":2000,"page_result_bytes":1048576,"edit_cells":2000,"edit_request_bytes":1048576,
             "edit_receipt_bytes":16384,"history_records":32,"history_logical_patch_bytes":2097152,
             "fixture_cells_per_complex_leaf":3,"fixture_execute_timeout_seconds":20,"checked_api_timeout_seconds":30,
             "stop_barrier_timeout_seconds":15,"stop_settle_timeout_seconds":25,
             "meaning":"Synthetic reported admission/request limits; not a whole-case or memory qualification."},
             "scope":{"public_engine_api_only":True,
             "owned_temporary_workspace":True,"native_precision_requested":required,
             "artifact_hashes_are_endpoint_identity_not_abi_or_load_proof":True}}
    if qualified:
        probe.update(qualified_doctor())
        if probe_mode == "qualified-missing-provider-binding":
            identities[:] = [item for item in identities if item["kind"] != "intrinsic_provider"]
        elif probe_mode == "qualified-missing-status":
            del probe["qualified_startup_status"]
        elif probe_mode == "graphics-missing-binding":
            identities[:] = [item for item in identities if item["kind"] != "graphics_adapter"]
        elif probe_mode == "graphics-foreign-binding":
            next(item for item in identities if item["kind"] == "graphics_adapter")["path"] = "/foreign/graphics.oct"
        elif probe_mode == "graphics-extra-binding":
            identities.append({**identities[-1], "kind":"unselected_adapter"})
        elif probe_mode == "graphics-wrong-configured-by":
            next(item for item in identities if item["kind"] == "graphics_adapter")["configured_by"] = "MATHLAB_GRAPHICS_BRIDGE"
        elif probe_mode == "graphics-missing-capability":
            capabilities.remove("graphics_registry.v1")
            probe["worker_protocol"]["capabilities"].remove("graphics_registry.v1")
        elif probe_mode == "graphics-ready-numeric-version":
            probe["qualified_startup_status"]["intrinsics"]["adapters"]["graphics"]["version"] = 1
    if probe_mode == "wrong-profile": probe["profile"] = "native-unavailable" if required else "native-required"
    elif probe_mode == "wrong-kind": probe["kind"] = "unrelated-diagnostic"
    elif probe_mode == "bool-schema": probe["schema_version"] = True
    elif probe_mode == "float-schema": probe["schema_version"] = 1.0
    elif probe_mode == "failed-status": probe["status"] = "failed"
    elif probe_mode == "missing-error": del probe["error"]
    elif probe_mode == "missing-limits": del probe["limits"]
    elif probe_mode == "bool-limit": probe["limits"]["page_cells"] = True
    elif probe_mode == "missing-case": cases.pop()
    elif probe_mode == "duplicate-case": cases.append(cases[0])
    elif probe_mode == "extra-case": cases.append({"id":"unrelated","status":"passed","error_identifier":None,"details":{}})
    elif probe_mode == "failed-case": cases[1]["status"] = "failed"
    elif probe_mode == "skipped-case": cases[1]["status"] = "skipped"
    elif probe_mode == "rounded-uint64": cases[1]["details"]["old_values"][0] = "9007199254740992"
    elif probe_mode == "uint64-oracle-false": cases[1]["details"]["exact_terminal_integer_oracle"] = False
    elif probe_mode == "missing-capability": capabilities.remove("typed_nd_page.v1")
    elif probe_mode == "duplicate-capability": capabilities.append(capabilities[0])
    elif probe_mode == "wrong-value-path": probe["value_adapter"] = "/outside/mathlab_value_bridge.oct"
    elif probe_mode == "wrong-cli-path": probe["build"]["executable"] = "/outside/mathlab"
    elif probe_mode == "wrong-runtime": probe["runtime"] = "/outside/octave"
    elif probe_mode == "wrong-workspace": probe["runtime_details"]["working_directory"] = str(directory)
    elif probe_mode == "wrong-binding-hash": identities[0]["sha256_after"] = "b"*64
    elif probe_mode == "duplicate-binding": identities.append(identities[0])
    elif probe_mode == "missing-binding": identities.pop()
    elif probe_mode == "missing-admission": preflight.pop()
    elif probe_mode == "duplicate-admission": preflight[-1] = preflight[0]
    elif probe_mode == "missing-native-capability": capabilities.remove(capability)
    elif probe_mode == "duplicate-journey": cases[2]["details"]["journeys"][-1] = cases[2]["details"]["journeys"][0]
    elif probe_mode == "wrong-unavailable-error": cases[-1]["error_identifier"] = "MathLab:Unrelated"
    elif probe_mode == "false-native-success": cases[-1]["details"]["native_precision_success"] = True
    elif probe_mode == "unexpected-native-capability": capabilities.append(capability)
    elif probe_mode.startswith("drift-"):
        changed = {"drift-cli":cli,"drift-runtime":canonical_runtime,"drift-module":expected_value}[probe_mode]
        with changed.open("a") as output: output.write("\n# changed during synthetic API probe\n")
    if probe_mode == "invalid-json": print("not a JSON report")
    elif probe_mode == "duplicate-json-key": print(json.dumps(probe).replace('"schema_version": 1', '"schema_version": 0, "schema_version": 1',1))
    elif probe_mode == "nan-json": print(json.dumps(probe)[:-1]+',"unexpected":NaN}')
    else: print(json.dumps(probe))
    if probe_mode == "nonzero-success": sys.exit(9)
elif action == "repl":
    text=sys.stdin.read()
    assert "qa_session_seed=41" in text and ":workspace" in text and ":quit" in text
    print(json.dumps(result("NUMJINN_QA_REPL_FIRST 41\n",[variable("qa_session_seed",[1,1],"41")])))
    error={"identifier":"Fake:REPL","message":"synthetic execution failure","stack":[]} if mode=="repl-error" else None
    print(json.dumps(result("NUMJINN_QA_REPL_SECOND 42\n",[variable("qa_session_seed",[1,1],"42")],error=error)))
    print(json.dumps({"workspace":[variable("qa_session_seed",[1,1],"[42]")]}))
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
        output="NUMJINN_QA_PLOT_DATA 0,0.5,1;0,0.25,1\n"
        if qualified:
            assert "getenv('GSC')" in source.read_text()
            observed = startup["ghostscript"]["path"]
            if mode == "qualified-wrong-gsc": observed = "/foreign/gs"
            elif mode == "qualified-empty-gsc": observed = ""
            if mode != "qualified-missing-gsc": output += "NUMJINN_QA_GSC " + observed + "\n"
            if mode == "qualified-ghostscript-drift":
                with Path(startup["ghostscript"]["path"]).open("a") as changed:
                    changed.write("changed during synthetic graphics command\n")
        print(json.dumps(result(output,
                                [variable("qa_plot_t",[1,3]),variable("qa_plot_y",[1,3])],png())))
    elif source.name == "getting_started.m":
        mat=directory/"numjinn-qa-example.mat"
        assert "mat_path = '"+str(mat).replace("'", "''")+"';" in source.read_text()
        header=b"MATLAB 5.0 MAT-file synthetic harness output".ljust(124,b" ")+b"\x00\x01IM"
        mat.write_bytes(header+b"synthetic same-runtime fixture result")
        output="Solution: x = [2.000000; 3.000000], residual = 0\nVerified MAT round trip: "+str(mat)+"\n"
        print(json.dumps(result(output,[variable("x",[2,1]),variable("t",[1,501]),variable("y",[1,501])],png())))
    elif source.name == "qa-input.m":
        if qualified:
            selected_input = next(adapter["file"]["path"] for adapter in startup["adapters"] if adapter["role"] == "input")
            assert selected_input == str(expected_value.parent / "mathlab_input_bridge.oct")
        else:
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
        self.write("Contents/Resources/examples/getting_started.m", "% synthetic bundled original example\nmat_path = [tempname(), '.mat'];\n")
        for name, data in (("Cargo.toml", "[workspace.package]\nversion=\"0.1.0\"\n"),
                           ("Cargo.lock", "# synthetic lock\n"), ("LICENSE", "synthetic notice\n"),
                           ("crates/mathlab-cli/src/main.rs", "// corresponding fake source\n"),
                           ("octave/mathlab_worker.m", "% corresponding fake worker\n"),
                           ("examples/getting_started.m", "% synthetic bundled original example\nmat_path = [tempname(), '.mat'];\n")):
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

    def required_value(self):
        self.native()
        self.write("Contents/Resources/native/mathlab_value_bridge.oct", "synthetic required value adapter")
        self.write("Contents/Resources/native/runtime-version.txt", "11.3.0\n")
        names = ("mathlab_debug_bridge.oct", "mathlab_scope_bridge.oct", "mathlab_input_bridge.oct",
                 "mathlab_value_bridge.oct", "runtime-version.txt")
        hashes = {name: qa.digest(self.bundle / "Contents/Resources/native" / name) for name in names}
        self.manifest["build"]["native"] = {
            "status": "enabled_protocol_load_passed",
            "adapter_profile": {"schema_version": 1, "name": "four-adapter-value-v1"},
            "value_component": {"status": "required", "path": "Contents/Resources/native/mathlab_value_bridge.oct",
                                "capability": "dense_complex_native_components.v1", "sha256": hashes["mathlab_value_bridge.oct"]},
            "artifact_sha256": hashes, "runtime_version": "11.3.0", "sdk": {"OCTAVE_VERSION": "11.3.0"},
            "runtime_executable_sha256": qa.digest(self.runtime),
            "protocols": {"debug": {"protocol": "mathlab-debugger", "version": 4},
                          "scope": {"protocol": "mathlab-scope", "version": 2},
                          "input": {"protocol": "mathlab-input", "version": 1},
                          "value": {"protocol": "mathlab-value", "version": 1, "dense_complex_components": True,
                                    "max_cells": 2000, "max_rank": 32, "max_replace_leaf_bytes": 134217728}},
            "worker_observation": {"method": "built-cli-doctor", "ready": True,
                                   "cli_executable_sha256": qa.digest(self.bundle / qa.CLI_PATH),
                                   "value_adapter": "/vanished-build/native/mathlab_value_bridge.oct",
                                   "runtime": "/vanished-build/runtime/octave",
                                   "worker_capabilities": ["typed_nd_page.v1", "atomic_range_edit.v1",
                                                           "dense_complex_native_components.v1", "future_extra.v1"],
                                   "module_sha256": hashes["mathlab_value_bridge.oct"],
                                   "runtime_executable_sha256": qa.digest(self.runtime)}}
        self.manifest["build"]["linked_libraries"] = {name: "synthetic library inspection" for name in names if name.endswith(".oct")}
        self.environment["QA_FAKE_EXPECT_VALUE"] = "required"
        self.seal()

    def omitted_value(self):
        self.manifest["build"]["native"] = {
            "status": "not_included", "adapter_profile": {"schema_version": 1, "name": "native-omitted-v1"},
            "value_component": {"status": "not_included", "reason": "native_profile_not_requested"},
            "reason": "Native profile was not requested", "artifact_sha256": {}}
        self.seal()

    def qualified_value(self, graphics=False):
        self.required_value()
        provider = self.write("Contents/Resources/native/mathlab_intrinsic_bridge.oct", "synthetic required intrinsic provider")
        native = self.manifest["build"]["native"]
        profile = "qualified-graphics-v2" if graphics else "qualified-core-v2"
        native["adapter_profile"] = {"schema_version": 1, "name": profile}
        native["artifact_sha256"][provider.name] = qa.digest(provider)
        native["sdk"]["OCTAVE_API_VERSION"] = "api-v61"
        native["protocols"]["intrinsic"] = {"protocol":"mathlab-intrinsics", "version":1,
            "octave_version":"11.3.0", "octave_api_version":"api-v61", "actual_function_object":True,
            "native_base_queries":True, "unlocked_registry":True, "callback_binding_v2":True,
            "adapter_carrier_v2":True, "provider_shutdown_proxy":True}
        ready_protocols = (("input","mathlab-input","1"),("scope","mathlab-scope","2"),
                           ("value","mathlab-value","1"),("debug","mathlab-debugger","4"))
        if graphics:
            module = self.write("Contents/Resources/native/mathlab_graphics_bridge.oct", "synthetic required graphics adapter")
            native["artifact_sha256"][module.name] = qa.digest(module)
            native["protocols"]["graphics"] = dict(qa.GRAPHICS_PROTOCOL)
            native["worker_observation"]["worker_capabilities"].append(qa.GRAPHICS_CAPABILITY)
            self.manifest["build"]["linked_libraries"][module.name] = "synthetic graphics library inspection"
            ready_protocols += (("graphics","mathlab-graphics","1"),)
        self.manifest["build"]["linked_libraries"][provider.name] = "synthetic provider library inspection"
        modules = {name: {"path":"/vanished-build/native/" + name, "sha256":value}
                   for name, value in native["artifact_sha256"].items() if name.endswith(".oct")}
        context = {"worker_id":"synthetic-build-worker", "generation":"1", "invocation":"1", "revision":"0"}
        intrinsic = {"protocol":"mathlab-intrinsics", "version":"1", "phase":"ready",
            "provider_file":modules[provider.name]["path"], "worker_file":"/vanished-build/session/mathlab_worker.m",
            "registry_file":"/vanished-build/session/registry/mathlab_scope_register.m",
            "entry_alias":"__mlentry_build", "shutdown_alias":"__mlshutdown_build",
            "octave_version":"11.3.0", "octave_api_version":"api-v61", "initial_path_admitted":True,
            "shutdown_registered":True,
            "adapters":{role:{"path":modules["mathlab_" + role + "_bridge.oct"]["path"],
                              "protocol":protocol,"version":version}
                        for role,protocol,version in ready_protocols}}
        native["worker_observation"].update(native_modules=modules, session_context=context,
            qualified_startup_status={"profile":profile, "context":dict(context), "intrinsics":intrinsic})
        self.environment["QA_FAKE_EXPECT_QUALIFIED"] = "required"
        if graphics: self.environment["QA_FAKE_EXPECT_GRAPHICS"] = "required"
        gnuplot = self.root / "external-fake-gnuplot"
        gnuplot.write_text("Synthetic pinned plotting tool; never executed\n")
        gnuplot.chmod(0o755)
        ghostscript = self.root / "external-fake-ghostscript"
        ghostscript.write_text("Synthetic pinned Ghostscript; never executed\n")
        ghostscript.chmod(0o755)
        native["ghostscript"] = {"path": str(ghostscript), "sha256": qa.digest(ghostscript)}
        startup = {"installation_root":str(self.root),
            "executable":{"path":str(self.runtime),"sha256":qa.digest(self.runtime)},
            "provider":dict(modules["mathlab_intrinsic_bridge.oct"]),
            "gnuplot":{"path":str(gnuplot),"sha256":qa.digest(gnuplot)},
            "ghostscript":dict(native["ghostscript"]),
            "runtime_directories":[str(self.root)],
            "adapters":[{"role":role,"file":dict(modules["mathlab_" + role + "_bridge.oct"])}
                        for role, _, _ in ready_protocols],
            "octave_version":"11.3.0","octave_api_version":"api-v61"}
        self.startup_seed = self.root / "startup-seed.json"
        document = {"schema_version":2,"profile":profile,"startup":startup}
        self.startup_seed.write_text(json.dumps(document))
        embedded = copy.deepcopy(document)
        for pin in [embedded["startup"]["provider"]] + [adapter["file"] for adapter in embedded["startup"]["adapters"]]:
            pin["path"] = str(self.bundle / "Contents/Resources/native" / Path(pin["path"]).name)
        self.write(qa.STARTUP_CONFIG_PATH, json.dumps(embedded))
        self.environment["QA_FAKE_STARTUP_EXTERNAL"] = json.dumps({key:value for key,value in startup.items()
                                                                   if key not in ("provider","adapters")})
        self.seal()

    def assert_identity_blocks_execution(self):
        report = self.run_qa()
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed", json.dumps(report, indent=2))
        self.assertEqual(report["commands"], [])

    def run_qa(self, mode="pass", timeout=5, output_limit=2 * 1024 * 1024, octave=None, startup_config=None, use_startup=True):
        environment = {**self.environment, "QA_FAKE_MODE": mode}
        seed = startup_config if startup_config is not None else getattr(self, "startup_seed", None) if use_startup else None
        selected = None if seed is not None and octave is None else str(octave or self.runtime)
        with patch.dict(os.environ, environment):
            acceptance = qa.Acceptance(self.bundle, self.reports, selected, timeout, output_limit, startup_config=seed)
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
                                MATHLAB_DEBUG_BRIDGE="/not-the-bundle/debug.oct",
                                MATHLAB_VALUE_BRIDGE="/not-the-bundle/value.oct")
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertIsNone(doctor["input_adapter"])
        self.assertIsNone(doctor["scope_adapter"])
        self.assertIsNone(doctor["value_adapter"])

    def test_legacy_native_profile_does_not_inherit_an_external_value_adapter(self):
        self.native()
        self.environment.update(MATHLAB_SCOPE_BRIDGE="/not-the-bundle/scope.oct",
                                MATHLAB_INPUT_BRIDGE="/not-the-bundle/input.oct",
                                MATHLAB_VALUE_BRIDGE="/not-the-bundle/value.oct")
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertEqual(doctor["scope_adapter"], str(self.bundle / "Contents/Resources/native/mathlab_scope_bridge.oct"))
        self.assertEqual(doctor["input_adapter"], str(self.bundle / "Contents/Resources/native/mathlab_input_bridge.oct"))
        self.assertIsNone(doctor["value_adapter"])
        self.assertEqual(report["qualification"]["native_input_coverage"], "passed")

    def test_required_value_profile_uses_only_its_installed_adapter_and_keeps_desktop_journeys_pending(self):
        self.required_value()
        self.environment["MATHLAB_VALUE_BRIDGE"] = "/external/untrusted/mathlab_value_bridge.oct"
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertEqual(doctor["value_adapter"], str(self.bundle / "Contents/Resources/native/mathlab_value_bridge.oct"))
        self.assertIn("dense_complex_native_components.v1", doctor["worker_capabilities"])
        self.assertTrue(report["provenance"]["value_component_required"])
        self.assertEqual(report["qualification"]["packaged_value_read_edit_history"], "passed")
        self.assertEqual(report["qualification"]["packaged_portable_uint64_read_edit_history"], "passed")
        self.assertEqual(report["qualification"]["packaged_native_complex_read_edit_history"], "passed")
        pending = {item["id"]: item for item in report["pending_release_journeys"]}
        self.assertEqual(pending["packaged_value_read_edit_history"]["status"], "passed")
        self.assertEqual(pending["desktop_ui"]["status"], "not_run")
        self.assertFalse(report["qualification"]["release_qualified"])

    def test_declared_omitted_profile_scrubs_external_value_and_reports_no_value_support(self):
        self.omitted_value()
        self.environment["MATHLAB_VALUE_BRIDGE"] = "/external/untrusted/mathlab_value_bridge.oct"
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        self.assertIsNone(report["checks"]["doctor"]["evidence"]["value_adapter"])
        self.assertNotIn("dense_complex_native_components.v1", report["checks"]["doctor"]["evidence"]["worker_capabilities"])
        self.assertFalse(report["provenance"]["value_component_required"])
        self.assertEqual(report["checks"]["packaged_value_api"]["status"], "passed")
        self.assertEqual(report["qualification"]["packaged_portable_uint64_read_edit_history"], "passed")
        self.assertEqual(report["qualification"]["packaged_native_complex_read_edit_history"], "unavailable_expected_rejections_passed")
        self.assertEqual(report["qualification"]["packaged_value_read_edit_history"], "not_run")

    def test_qualified_core_profile_binds_installed_five_and_reuses_only_the_scoped_value_probe(self):
        self.qualified_value()
        seed_bytes = self.startup_seed.read_bytes()
        relocated = self.root / "Relocated Ω" / self.bundle.name
        relocated.parent.mkdir()
        self.bundle.rename(relocated)
        self.bundle = relocated
        permissions = {path: path.stat().st_mode & 0o777 for path in [self.bundle, *self.bundle.rglob("*")]}
        def restore_permissions():
            for path, mode in permissions.items():
                path.chmod(mode)
        self.addCleanup(restore_permissions)
        for path, mode in permissions.items():
            path.chmod(mode & ~0o222)
        self.environment.update({key:"/stale/external/selection" for key in ("MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE",
            "MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE", "MATHLAB_VALUE_BRIDGE")})
        self.environment["GSC"] = "/stale/inherited/ghostscript"
        self.environment["GNUPLOT"] = "/stale/inherited/gnuplot"
        before = qa.inventory(self.bundle)
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertTrue(report["provenance"]["qualified_startup_required"])
        self.assertTrue(report["provenance"]["value_component_required"])
        self.assertEqual(doctor["qualified_startup_status"]["context"], doctor["session_context"])
        self.assertEqual(len(doctor["native_modules"]), 5)
        for name, identity in doctor["native_modules"].items():
            self.assertEqual(identity, {"path": str(self.bundle / "Contents/Resources/native" / name),
                                        "sha256": self.manifest["build"]["native"]["artifact_sha256"][name]})
            self.assertNotEqual(identity["path"], self.manifest["build"]["native"]["worker_observation"]["native_modules"][name]["path"])
        self.assertEqual(report["checks"]["packaged_value_api"]["status"], "passed")
        self.assertEqual(report["checks"]["packaged_value_api"]["evidence"]["profile"], "native-required")
        self.assertIn("intrinsic_provider", report["checks"]["packaged_value_api"]["evidence"]["independent_artifact_identities_before"])
        self.assertEqual(len(report["checks"]["packaged_value_api"]["evidence"]["independent_artifact_identities_before"]), 7)
        self.assertNotIn("ghostscript", report["checks"]["packaged_value_api"]["evidence"]["independent_artifact_identities_before"])
        graphics = report["checks"]["graphics_png"]["evidence"]
        pin = self.manifest["build"]["native"]["ghostscript"]
        self.assertEqual(graphics["ghostscript"], {"selected": pin, "observed_gsc": pin["path"]})
        self.assertTrue(graphics["pngs"])
        self.assertEqual(report["ghostscript"]["before"], report["ghostscript"]["after"])
        for command in report["commands"]:
            if command["id"] != "cli-version":
                self.assertIn("--startup-config", command["argv"])
                self.assertNotIn("--octave", command["argv"])
                self.assertEqual(command["ghostscript"]["before"], command["ghostscript"]["after"])
            self.assertNotIn("--packaged-startup", command["argv"])
        retained = Path(report["report_directory"]) / report["startup_config"]["resolved"]["path"]
        document = json.loads(retained.read_text())
        self.assertEqual(document["schema_version"], 2)
        self.assertEqual(document["startup"]["ghostscript"], pin)
        external = json.loads(self.environment["QA_FAKE_STARTUP_EXTERNAL"])
        self.assertEqual({key:value for key,value in document["startup"].items() if key not in ("provider","adapters")}, external)
        negative = json.loads((retained.parent / "missing-executable.json").read_text())
        self.assertEqual(negative["startup"]["executable"]["sha256"], document["startup"]["executable"]["sha256"])
        self.assertTrue(negative["startup"]["executable"]["path"].endswith("/deliberately-missing-octave"))
        self.assertEqual(report["checks"]["missing_runtime_diagnostic"]["status"], "passed")
        self.assertEqual(self.startup_seed.read_bytes(), seed_bytes)
        self.assertEqual(qa.inventory(self.bundle), before)
        self.assertFalse(report["qualification"]["release_qualified"])
        self.assertEqual(report["scope"], "installed_cli_only")
        pending = {item["id"]: item for item in report["pending_release_journeys"]}
        for journey in ("desktop_ui", "read_only_resources_and_relocation"):
            self.assertEqual(pending[journey]["status"], "not_run")

    def test_qualified_graphics_profile_rebases_six_and_binds_eight_probe_artifacts(self):
        self.qualified_value(graphics=True)
        self.assertEqual(self.manifest["build"]["native"]["protocols"]["graphics"], {
            "protocol":"mathlab-graphics", "version":1, "octave_version":"11.3.0", "octave_api_version":"api-v61",
            "native_object_lifetime":True, "bound_primitive_dispatcher":True, "max_entries":4096})
        seed_bytes = self.startup_seed.read_bytes()
        embedded_bytes = (self.bundle / qa.STARTUP_CONFIG_PATH).read_bytes()
        relocated = self.root / "Relocated Graphics Ω.app"
        self.bundle.rename(relocated)
        self.bundle = relocated
        before = qa.inventory(self.bundle)
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
        doctor = report["checks"]["doctor"]["evidence"]
        self.assertEqual(set(doctor["native_modules"]), qa.GRAPHICS_MODULE_NAMES)
        self.assertIn(qa.GRAPHICS_CAPABILITY, doctor["worker_capabilities"])
        status = doctor["qualified_startup_status"]
        self.assertEqual(status["profile"], "qualified-graphics-v2")
        for name, identity in doctor["native_modules"].items():
            self.assertEqual(identity, {"path": str(self.bundle / "Contents/Resources/native" / name),
                "sha256": self.manifest["build"]["native"]["artifact_sha256"][name]})
        self.assertEqual(status["intrinsics"]["adapters"]["graphics"], {
            "path": str(self.bundle / "Contents/Resources/native" / qa.GRAPHICS_NAME),
            "protocol": "mathlab-graphics", "version": "1"})
        probe = report["checks"]["packaged_value_api"]["evidence"]
        identities = probe["independent_artifact_identities_before"]
        self.assertEqual(set(identities), {"cli_executable", "octave_runtime", "intrinsic_provider",
            "debug_adapter", "scope_adapter", "input_adapter", "value_adapter", "graphics_adapter"})
        self.assertEqual(identities, probe["independent_artifact_identities_after"])
        self.assertEqual(probe["native_modules"], doctor["native_modules"])
        self.assertEqual(probe["qualified_startup_status"]["profile"], "qualified-graphics-v2")
        graphics = report["checks"]["graphics_png"]["evidence"]
        self.assertTrue(graphics["pngs"])
        self.assertEqual(graphics["ghostscript"]["selected"], self.manifest["build"]["native"]["ghostscript"])
        self.assertEqual(graphics["ghostscript"]["observed_gsc"], graphics["ghostscript"]["selected"]["path"])
        self.assertEqual(report["ghostscript"]["before"], report["ghostscript"]["after"])
        self.assertEqual(self.startup_seed.read_bytes(), seed_bytes)
        self.assertEqual((self.bundle / qa.STARTUP_CONFIG_PATH).read_bytes(), embedded_bytes)
        self.assertEqual(qa.inventory(self.bundle), before)
        self.assertFalse(report["qualification"]["release_qualified"])

    def test_qualified_graphics_manifest_and_schema2_selection_fail_closed(self):
        self.qualified_value(graphics=True)
        baseline = copy.deepcopy(self.manifest)
        changes = (
            (("protocols", "graphics", "version"), "1"),
            (("protocols", "graphics", "version"), 1.0),
            (("protocols", "graphics", "max_entries"), 4096.0),
            (("protocols", "graphics", "native_object_lifetime"), 1),
            (("protocols", "graphics", "extra"), True),
            (("protocols", "graphics"), None),
            (("worker_observation", "worker_capabilities"), sorted(qa.VALUE_REQUIRED_CAPABILITIES)),
            (("worker_observation", "native_modules", qa.GRAPHICS_NAME, "sha256"), "b" * 64),
            (("worker_observation", "qualified_startup_status", "profile"), "qualified-core-v2"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "graphics", "version"), 1),
        )
        for path, replacement in changes:
            with self.subTest(manifest_path=path, replacement=replacement):
                self.manifest = copy.deepcopy(baseline)
                target = self.manifest["build"]["native"]
                for key in path[:-1]: target = target[key]
                target[path[-1]] = replacement
                self.seal()
                self.assert_identity_blocks_execution()
        self.manifest = baseline
        self.seal()
        original = self.startup_seed.read_bytes()
        native = self.manifest["build"]["native"]
        for variant in ("schema1", "core-profile", "missing-graphics", "duplicate-graphics", "wrong-pin", "extra-role"):
            with self.subTest(selection=variant):
                document = json.loads(original)
                adapters = document["startup"]["adapters"]
                selected = next(adapter for adapter in adapters if adapter["role"] == "graphics")
                if variant == "schema1": document["schema_version"] = 1
                elif variant == "core-profile": document["profile"] = "qualified-core-v2"
                elif variant == "missing-graphics": adapters.remove(selected)
                elif variant == "duplicate-graphics": adapters[0] = copy.deepcopy(selected)
                elif variant == "wrong-pin": selected["file"]["sha256"] = "b" * 64
                elif variant == "extra-role": adapters.append({"role":"foreign", "file":dict(selected["file"])})
                self.startup_seed.write_text(json.dumps(document))
                with self.assertRaises(qa.QAFailure):
                    qa.installed_startup_config(self.startup_seed, self.bundle, native)
        self.startup_seed.write_bytes(original)
        module = self.bundle / "Contents/Resources/native" / qa.GRAPHICS_NAME
        data = module.read_bytes()
        module.unlink()
        self.seal()
        self.assert_identity_blocks_execution()
        module.write_bytes(data)
        extra = self.write("Contents/Resources/other/mathlab_graphics_bridge.oct", "unselected duplicate")
        self.seal()
        self.assert_identity_blocks_execution()
        extra.unlink()

    def test_core5_cannot_smuggle_graphics_module_protocol_or_ready(self):
        self.qualified_value()
        baseline = copy.deepcopy(self.manifest)
        for variant in ("artifact-declaration", "protocol-declaration", "ready-role", "status-downgrade", "capability-only"):
            with self.subTest(variant=variant):
                self.manifest = copy.deepcopy(baseline)
                native = self.manifest["build"]["native"]
                if variant == "artifact-declaration": native["artifact_sha256"][qa.GRAPHICS_NAME] = "b" * 64
                elif variant == "protocol-declaration": native["protocols"]["graphics"] = dict(qa.GRAPHICS_PROTOCOL)
                elif variant == "ready-role":
                    native["worker_observation"]["qualified_startup_status"]["intrinsics"]["adapters"]["graphics"] = {
                        "path":"/foreign/graphics.oct", "protocol":"mathlab-graphics", "version":"1"}
                elif variant == "status-downgrade": native["worker_observation"]["qualified_startup_status"]["profile"] = "qualified-graphics-v2"
                else: native["worker_observation"]["worker_capabilities"].append(qa.GRAPHICS_CAPABILITY)
                self.seal()
                if variant == "capability-only":
                    report = self.run_qa()
                    self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed")
                    self.assertIn("graphics capability differs from the selected profile",
                                  report["checks"]["bundle_identity"]["reason"])
                    self.assertEqual(report["commands"], [])
                else:
                    self.assert_identity_blocks_execution()
        self.manifest = baseline
        self.write("Contents/Resources/native/" + qa.GRAPHICS_NAME, "unclaimed graphics module")
        self.seal()
        self.assert_identity_blocks_execution()

    def test_qualified_graphics_live_and_probe_require_capability_and_sixth_binding(self):
        self.qualified_value(graphics=True)
        for mode in ("graphics-missing-capability", "graphics-missing-module", "graphics-ready-numeric-version", "graphics-foreign-adapter"):
            with self.subTest(doctor=mode):
                report = self.run_qa(mode)
                self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
                self.assertEqual(report["checks"]["doctor"]["status"], "failed")
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "not_run")
        for mode in ("graphics-missing-binding", "graphics-foreign-binding", "graphics-extra-binding",
                     "graphics-wrong-configured-by", "graphics-missing-capability", "graphics-ready-numeric-version"):
            with self.subTest(probe=mode):
                self.environment["QA_FAKE_PROBE_MODE"] = mode
                report = self.run_qa()
                self.assertEqual(report["checks"]["doctor"]["status"], "passed")
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed")
                self.assertEqual(report["checks"]["graphics_png"]["status"], "not_run")

    def test_qualified_manifest_missing_provider_and_accepted_status_prerequisites_block_execution(self):
        self.qualified_value()
        baseline = copy.deepcopy(self.manifest)
        for path in (("artifact_sha256", "mathlab_intrinsic_bridge.oct"), ("protocols", "intrinsic"),
                     ("sdk", "OCTAVE_API_VERSION"), ("worker_observation", "native_modules"),
                     ("worker_observation", "qualified_startup_status"), ("worker_observation", "session_context")):
            with self.subTest(path=path):
                self.manifest = copy.deepcopy(baseline)
                target = self.manifest["build"]["native"]
                for part in path[:-1]:
                    target = target[part]
                del target[path[-1]]
                self.seal()
                self.assert_identity_blocks_execution()
        self.manifest = baseline
        self.seal()
        report = self.run_qa(use_startup=False)
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed")
        self.assertEqual(report["commands"], [])
        with self.assertRaisesRegex(qa.QAFailure, "mutually exclusive"):
            self.run_qa(octave=self.runtime)
        original = self.startup_seed.read_bytes()
        for path in (("executable", "sha256"), ("provider", "sha256"), ("adapters", 0, "file", "sha256")):
            with self.subTest(config_pin=path):
                document = json.loads(original)
                target = document["startup"]
                for part in path[:-1]:
                    target = target[part]
                target[path[-1]] = "b" * 64
                self.startup_seed.write_text(json.dumps(document))
                self.assert_identity_blocks_execution()
        self.startup_seed.write_bytes(original)

    def test_qualified_schema2_ghostscript_pin_and_external_admission_fail_closed(self):
        self.qualified_value()
        native = self.manifest["build"]["native"]
        original = self.startup_seed.read_bytes()
        document = json.loads(original)
        selected = document["startup"]["ghostscript"]
        external = Path(selected["path"])
        alias = self.root / "ghostscript-alias"
        alias.symlink_to(external)
        unsafe = self.root / "unsafe auxiliary Ω"
        unsafe.write_bytes(external.read_bytes())
        unsafe.chmod(0o755)
        invalid = []
        for version in (1, True, 2.0):
            changed = copy.deepcopy(document)
            changed["schema_version"] = version
            invalid.append(("schema-" + str(version), changed))
        for kind, pin in (("missing", None), ("extra", {**selected, "extra": True}),
                          ("malformed-hash", {**selected, "sha256": "not-a-hash"}),
                          ("relative", {**selected, "path": "relative/gs"}),
                          ("symlink", {**selected, "path": str(alias)}),
                          ("noncanonical", {**selected, "path": str(self.root) + "/./external-fake-ghostscript"}),
                          ("unsafe-auxiliary-path", {**selected, "path": str(unsafe)}),
                          ("missing-executable", {**selected, "path": str(self.root / "missing-gs")}),
                          ("duplicate-executable", dict(document["startup"]["executable"]))):
            changed = copy.deepcopy(document)
            if pin is None:
                del changed["startup"]["ghostscript"]
            else:
                changed["startup"]["ghostscript"] = pin
            invalid.append((kind, changed))
        for name, changed in invalid:
            with self.subTest(selection=name):
                self.startup_seed.write_text(json.dumps(changed))
                changed_native = copy.deepcopy(native)
                if "ghostscript" in changed["startup"]:
                    changed_native["ghostscript"] = copy.deepcopy(changed["startup"]["ghostscript"])
                with self.assertRaises((qa.QAFailure, OSError)):
                    qa.read_qualified_startup(self.startup_seed, changed_native)
        changed = copy.deepcopy(document)
        changed["startup"]["gnuplot"] = {"path": str(unsafe), "sha256": qa.digest(unsafe)}
        self.startup_seed.write_text(json.dumps(changed))
        with self.assertRaisesRegex(qa.QAFailure, "shell-safe ASCII"):
            qa.read_qualified_startup(self.startup_seed, native)
        self.startup_seed.write_bytes(original)
        for kind in ("nonexecutable", "tampered"):
            data = external.read_bytes()
            with self.subTest(external=kind):
                try:
                    if kind == "nonexecutable": external.chmod(0o644)
                    else: external.write_bytes(data + b"tampered")
                    with self.assertRaises((qa.QAFailure, OSError)):
                        qa.installed_startup_config(self.startup_seed, self.bundle, native)
                finally:
                    external.write_bytes(data)
                    external.chmod(0o755)
        changed_native = copy.deepcopy(native)
        del changed_native["ghostscript"]
        with self.assertRaises(qa.QAFailure):
            qa.installed_startup_config(self.startup_seed, self.bundle, changed_native)
        changed_native = copy.deepcopy(native)
        changed_native["ghostscript"]["sha256"] = "b" * 64
        with self.assertRaises(qa.QAFailure):
            qa.installed_startup_config(self.startup_seed, self.bundle, changed_native)
        embedded = self.bundle / qa.STARTUP_CONFIG_PATH
        embedded_original = embedded.read_bytes()
        other = self.root / "other-ghostscript"
        other.write_bytes(external.read_bytes())
        other.chmod(0o755)
        for kind in ("schema1", "different-pin", "duplicate-field"):
            with self.subTest(embedded=kind):
                try:
                    changed = json.loads(embedded_original)
                    if kind == "schema1": changed["schema_version"] = 1
                    elif kind == "different-pin": changed["startup"]["ghostscript"]["path"] = str(other)
                    if kind == "duplicate-field":
                        text = json.dumps(changed).replace('"ghostscript": {', '"ghostscript": {}, "ghostscript": {', 1)
                    else: text = json.dumps(changed)
                    embedded.write_text(text)
                    with self.assertRaises(qa.QAFailure):
                        qa.installed_startup_config(self.startup_seed, self.bundle, native)
                finally:
                    embedded.write_bytes(embedded_original)

    def test_qualified_resealed_ghostscript_inside_bundle_is_rejected(self):
        self.qualified_value()
        ascii_bundle = self.root / "Core5.app"
        self.bundle.rename(ascii_bundle)
        self.bundle = ascii_bundle
        ghostscript = self.write("Contents/Resources/export/gs", "Synthetic executable; never run\n", executable=True)
        pin = {"path": str(ghostscript.resolve()), "sha256": qa.digest(ghostscript)}
        self.assertRegex(pin["path"], r"^[A-Za-z0-9/_+.-]+$")
        self.manifest["build"]["native"]["ghostscript"] = dict(pin)
        for path in (self.startup_seed, self.bundle / qa.STARTUP_CONFIG_PATH):
            document = json.loads(path.read_text())
            document["startup"]["ghostscript"] = dict(pin)
            path.write_text(json.dumps(document))
        self.seal()
        report = self.run_qa()
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed")
        self.assertIn("outside the installed bundle", report["checks"]["bundle_identity"]["reason"])
        self.assertEqual(report["commands"], [])

    def test_qualified_graphics_requires_selected_gsc_and_stable_external_bytes(self):
        self.qualified_value()
        ghostscript = Path(self.manifest["build"]["native"]["ghostscript"]["path"])
        original = ghostscript.read_bytes()
        for mode in ("qualified-wrong-gsc", "qualified-empty-gsc", "qualified-missing-gsc", "qualified-ghostscript-drift"):
            with self.subTest(mode=mode):
                try:
                    report = self.run_qa(mode)
                    self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
                    self.assertEqual(report["checks"]["packaged_value_api"]["status"], "passed")
                    self.assertEqual(report["checks"]["graphics_png"]["status"], "failed")
                    self.assertEqual(report["status"], "failed")
                    if mode == "qualified-ghostscript-drift":
                        self.assertEqual(report["checks"]["bundle_unchanged"]["status"], "failed")
                finally:
                    ghostscript.write_bytes(original)

    def test_qualified_manifest_rejects_rehashed_foreign_downgraded_and_malformed_status(self):
        self.qualified_value()
        baseline = copy.deepcopy(self.manifest)
        changes = (
            (("protocols", "intrinsic", "version"), 1.0),
            (("protocols", "intrinsic", "actual_function_object"), 1),
            (("protocols", "intrinsic", "extra"), True),
            (("sdk", "OCTAVE_API_VERSION"), "api-v60"),
            (("worker_observation", "native_modules", "mathlab_intrinsic_bridge.oct", "sha256"), "b" * 64),
            (("worker_observation", "native_modules", "mathlab_intrinsic_bridge.oct", "path"), "relative/mathlab_intrinsic_bridge.oct"),
            (("worker_observation", "qualified_startup_status", "profile"), "four-adapter-value-v1"),
            (("worker_observation", "qualified_startup_status", "extra"), True),
            (("worker_observation", "qualified_startup_status", "intrinsics", "version"), 1),
            (("worker_observation", "qualified_startup_status", "intrinsics", "provider_file"), "/foreign/mathlab_intrinsic_bridge.oct"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "input", "path"), "/foreign/input.oct"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "shutdown_alias"), "__mlshutdown_other"),
            (("worker_observation", "qualified_startup_status", "context", "worker_id"), "bad\nworker"),
            (("worker_observation", "qualified_startup_status", "context", "generation"), "18446744073709551616"),
            (("worker_observation", "session_context", "revision"), "00"),
            (("worker_observation", "session_context", "invocation"), True),
            (("worker_observation", "session_context", "revision"), "1"),
        )
        for path, replacement in changes:
            with self.subTest(path=path, replacement=replacement):
                self.manifest = copy.deepcopy(baseline)
                target = self.manifest["build"]["native"]
                for part in path[:-1]:
                    target = target[part]
                target[path[-1]] = replacement
                self.seal()
                self.assert_identity_blocks_execution()

    def test_qualified_live_doctor_requires_current_installed_provider_and_accepted_context(self):
        self.qualified_value()
        for mode in ("qualified-missing-status", "qualified-missing-provider", "qualified-foreign-provider",
                     "qualified-wrong-provider-hash", "qualified-context-mismatch", "qualified-downgraded-status",
                     "qualified-extra-status", "qualified-foreign-adapter", "qualified-alias-mismatch", "qualified-foreign-runtime"):
            with self.subTest(mode=mode):
                report = self.run_qa(mode)
                self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
                self.assertEqual(report["checks"]["doctor"]["status"], "failed", json.dumps(report, indent=2))
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "not_run")
                self.assertEqual(report["checks"]["numerical_original_script"]["status"], "not_run")
        for mode in ("qualified-missing-provider-binding", "qualified-missing-status"):
            with self.subTest(probe_mode=mode):
                self.environment["QA_FAKE_PROBE_MODE"] = mode
                report = self.run_qa()
                self.assertEqual(report["checks"]["doctor"]["status"], "passed")
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed")
                self.assertEqual(report["checks"]["numerical_original_script"]["status"], "not_run")

    def test_nonqualified_manifest_variants_cannot_smuggle_intrinsic_declarations_or_modules(self):
        for configure in (None, self.omitted_value, self.native, self.required_value):
            if configure is not None:
                configure()
            baseline = copy.deepcopy(self.manifest)
            for declaration in ("protocol", "artifact", "status", "module"):
                with self.subTest(configure=configure, declaration=declaration):
                    self.manifest = copy.deepcopy(baseline)
                    native = self.manifest["build"]["native"]
                    extra = None
                    if declaration == "protocol":
                        native.setdefault("protocols", {})["intrinsic"] = {"protocol":"mathlab-intrinsics"}
                    elif declaration == "artifact":
                        native.setdefault("artifact_sha256", {})["mathlab_intrinsic_bridge.oct"] = "a" * 64
                    elif declaration == "status":
                        native["qualified_startup_status"] = {"profile":"qualified-core-v2"}
                    else:
                        extra = self.write("Contents/Resources/other/MathLab_INTRINSIC_bridge.OCT", "undeclared provider")
                    self.seal()
                    self.assert_identity_blocks_execution()
                    if extra is not None:
                        extra.unlink()
            self.manifest = baseline
            self.seal()
        report = self.run_qa(startup_config=self.root / "unrequested-startup.json")
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "failed")
        self.assertEqual(report["commands"], [])

    def test_nonqualified_live_doctor_variants_cannot_claim_accepted_intrinsic_startup(self):
        for configure in (None, self.omitted_value, self.native, self.required_value):
            if configure is not None:
                configure()
            with self.subTest(configure=configure):
                report = self.run_qa("unexpected-qualified-status")
                self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
                self.assertEqual(report["checks"]["doctor"]["status"], "failed")
                self.assertIn("intrinsic startup status", report["checks"]["doctor"]["reason"])

    def test_historical_legacy_doctor_without_value_fields_remains_valid(self):
        for native in (False, True):
            with self.subTest(native=native):
                if native:
                    self.native()
                    # Historical three-adapter manifests did not require this fingerprint.
                    self.manifest["build"]["native"].pop("runtime_executable_sha256")
                    self.seal()
                report = self.run_qa("legacy-doctor-fields")
                self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=2))
                self.assertFalse(report["provenance"]["value_component_required"])

    def test_null_unknown_or_extra_key_adapter_profiles_block_before_execution(self):
        profiles = (None, [], {}, {"schema_version": 1, "name": "future-v2"},
                    {"schema_version": 2, "name": "native-omitted-v1"},
                    {"schema_version": True, "name": "native-omitted-v1"},
                    {"schema_version": 1.0, "name": "native-omitted-v1"},
                    {"schema_version": 1, "name": "native-omitted-v1", "extra": True})
        for profile in profiles:
            with self.subTest(profile=profile):
                self.omitted_value()
                self.manifest["build"]["native"]["adapter_profile"] = profile
                self.seal()
                self.assert_identity_blocks_execution()

    def test_legacy_and_omitted_reject_bundled_value_modules_anywhere_or_case_aliases(self):
        for declared in (False, True):
            if declared:
                self.omitted_value()
            for path in ("Contents/Resources/native/mathlab_value_bridge.oct",
                         "Contents/Resources/relocated/mathlab_value_bridge.oct",
                         "Contents/Resources/relocated/MathLab_VALUE_bridge.OCT"):
                with self.subTest(declared=declared, path=path):
                    module = self.write(path, "undeclared value adapter")
                    self.seal()
                    self.assert_identity_blocks_execution()
                    module.unlink()
                    self.seal()

    def test_legacy_rejects_value_declarations_even_without_a_module(self):
        baseline = copy.deepcopy(self.manifest)
        declarations = ({"value_component": {"status": "not_included"}},
                        {"worker_observation": {}}, {"protocols": {"VALUE": {}}},
                        {"artifact_sha256": {"MathLab_VALUE_bridge.OCT": "a" * 64}})
        for declaration in declarations:
            with self.subTest(declaration=declaration):
                self.manifest = copy.deepcopy(baseline)
                self.manifest["build"]["native"].update(declaration)
                self.seal()
                self.assert_identity_blocks_execution()

    def test_omitted_profile_rejects_malformed_or_extra_value_declarations(self):
        self.omitted_value()
        baseline = copy.deepcopy(self.manifest)
        changes = ({"value_component": None},
                   {"value_component": {"status": "not_included", "reason": "native_profile_not_requested", "extra": True}},
                   {"value_component": {"status": "required"}}, {"worker_observation": {}},
                   {"protocols": {"value": {}}}, {"artifact_sha256": {"mathlab_value_bridge.oct": "a" * 64}})
        for change in changes:
            with self.subTest(change=change):
                self.manifest = copy.deepcopy(baseline)
                self.manifest["build"]["native"].update(change)
                self.seal()
                self.assert_identity_blocks_execution()

    def test_required_profile_missing_manifest_prerequisites_block_before_execution(self):
        self.required_value()
        baseline = copy.deepcopy(self.manifest)
        paths = (("native", "adapter_profile"), ("native", "value_component"),
                 ("native", "runtime_executable_sha256"), ("native", "runtime_version"),
                 ("native", "sdk"), ("native", "protocols"), ("native", "worker_observation"),
                 ("linked_libraries",), ("native", "artifact_sha256", "mathlab_value_bridge.oct"),
                 ("native", "artifact_sha256", "runtime-version.txt"))
        for path in paths:
            with self.subTest(path=path):
                self.manifest = copy.deepcopy(baseline)
                target = self.manifest["build"]
                for part in path[:-1]:
                    target = target[part]
                del target[path[-1]]
                self.seal()
                self.assert_identity_blocks_execution()

    def test_required_profile_rejects_rehashed_component_protocol_and_observation_mismatches(self):
        self.required_value()
        baseline = copy.deepcopy(self.manifest)
        changes = (
            (("value_component", "sha256"), "b" * 64),
            (("value_component", "path"), "Contents/Resources/other/mathlab_value_bridge.oct"),
            (("value_component", "extra"), True),
            (("protocols", "value", "version"), 1.0),
            (("protocols", "value", "max_rank"), True),
            (("protocols", "value", "max_cells"), 2001),
            (("protocols", "value", "extra"), True),
            (("sdk", "OCTAVE_VERSION"), "11.2.0"),
            (("runtime_executable_sha256",), "A" * 64),
            (("worker_observation", "cli_executable_sha256"), "b" * 64),
            (("worker_observation", "module_sha256"), "b" * 64),
            (("worker_observation", "runtime_executable_sha256"), "b" * 64),
            (("worker_observation", "ready"), 1),
            (("worker_observation", "method"), "native-protocol-only"),
            (("worker_observation", "value_adapter"), "relative/mathlab_value_bridge.oct"),
            (("worker_observation", "value_adapter"), "/build/native/../native/mathlab_value_bridge.oct"),
            (("worker_observation", "runtime"), "relative/octave"),
            (("worker_observation", "worker_capabilities"), ["typed_nd_page.v1", "atomic_range_edit.v1"]),
            (("worker_observation", "worker_capabilities"), ["dense_complex_native_components.v1"] * 3),
            (("worker_observation", "worker_capabilities"), "dense_complex_native_components.v1"),
            (("worker_observation", "extra"), True),
        )
        for path, replacement in changes:
            with self.subTest(path=path, replacement=replacement):
                self.manifest = copy.deepcopy(baseline)
                target = self.manifest["build"]["native"]
                for part in path[:-1]:
                    target = target[part]
                target[path[-1]] = replacement
                self.seal()  # Regenerate payload and final hashes; contract consistency must still fail.
                self.assert_identity_blocks_execution()

    def test_required_profile_rehashed_changed_module_cli_or_version_marker_is_rejected(self):
        self.required_value()
        paths = ("Contents/Resources/native/mathlab_value_bridge.oct", qa.CLI_PATH,
                 "Contents/Resources/native/runtime-version.txt")
        for path in paths:
            with self.subTest(path=path):
                manifest_before = copy.deepcopy(self.manifest)
                file = self.bundle / path
                before = file.read_bytes()
                file.write_bytes(before + b"\nchanged after observed build\n")
                if path != qa.CLI_PATH:
                    name = Path(path).name
                    current_hash = qa.digest(file)
                    self.manifest["build"]["native"]["artifact_sha256"][name] = current_hash
                    if name == "mathlab_value_bridge.oct":
                        self.manifest["build"]["native"]["value_component"]["sha256"] = current_hash
                self.seal()
                self.assert_identity_blocks_execution()
                file.write_bytes(before)
                self.manifest = manifest_before
                self.seal()

    def test_required_profile_rejects_duplicate_or_relocated_value_modules_after_resealing(self):
        self.required_value()
        canonical = self.bundle / "Contents/Resources/native/mathlab_value_bridge.oct"
        for relocated in (False, True):
            with self.subTest(relocated=relocated):
                alias = self.write("Contents/Resources/other/MathLab_VALUE_bridge.OCT", canonical.read_text())
                original = canonical.read_bytes()
                if relocated:
                    canonical.unlink()
                self.seal()
                self.assert_identity_blocks_execution()
                alias.unlink()
                canonical.write_bytes(original)
                self.seal()

    def test_required_doctor_missing_or_inconsistent_observations_fail_prerequisites(self):
        self.required_value()
        for mode in ("missing-value-path", "wrong-value-path", "noncanonical-value-path",
                     "missing-value-capabilities", "missing-runtime-path", "incomplete-value-capabilities", "duplicate-value-capabilities",
                     "invalid-value-capabilities", "relative-runtime-path"):
            with self.subTest(mode=mode):
                report = self.run_qa(mode)
                self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
                self.assertEqual(report["checks"]["doctor"]["status"], "failed", json.dumps(report, indent=2))
                self.assertEqual(report["checks"]["numerical_original_script"]["status"], "not_run")
                self.assertFalse(report["qualification"]["release_qualified"])

    def test_required_profile_current_external_runtime_must_match_observed_build_hash(self):
        self.required_value()
        self.runtime.write_text("Different external runtime bytes; same path and reported version\n")
        report = self.run_qa()
        self.assertEqual(report["checks"]["bundle_identity"]["status"], "passed")
        self.assertEqual(report["checks"]["doctor"]["status"], "failed")
        self.assertIn("External runtime differs", report["checks"]["doctor"]["reason"])

    def test_legacy_and_omitted_doctor_must_not_claim_a_value_path_or_capability(self):
        for declared in (False, True):
            if declared:
                self.omitted_value()
            for mode in ("wrong-value-path", "undeclared-value-capability"):
                with self.subTest(declared=declared, mode=mode):
                    report = self.run_qa(mode)
                    self.assertEqual(report["checks"]["doctor"]["status"], "failed")
                    self.assertIn("unexpectedly activated", report["checks"]["doctor"]["reason"])

    def test_duplicate_manifest_json_fields_fail_even_with_fresh_final_checksums(self):
        self.omitted_value()
        path = self.bundle / qa.RELEASE_PATH
        text = path.read_text()
        text = text.replace('"adapter_profile": {', '"adapter_profile": null, "adapter_profile": {', 1)
        path.write_text(text)
        actual = qa.inventory(self.bundle)
        self.write(qa.CHECKSUM_PATH, "".join(actual[name]["sha256"] + "  " + name + "\n"
                                            for name in sorted(actual) if name != qa.CHECKSUM_PATH))
        self.assert_identity_blocks_execution()

    def test_legacy_profiles_never_require_or_invoke_the_new_checked_api_probe(self):
        self.environment["QA_FAKE_PROBE_MODE"] = "must-not-run"
        for native in (False, True):
            with self.subTest(native=native):
                if native:
                    self.native()
                report = self.run_qa()
                self.assertEqual(report["status"], "passed")
                self.assertFalse(report["checks"]["packaged_value_api"]["required_for_cli_subset"])
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "not_run")
                self.assertFalse(any(command["id"] == "packaged-value-api" for command in report["commands"]))

    def test_required_packaged_api_binds_all_installed_modules_and_promotes_only_its_scoped_journeys(self):
        self.required_value()
        report = self.run_qa()
        check = report["checks"]["packaged_value_api"]
        self.assertEqual(check["status"], "passed", json.dumps(report, indent=2))
        self.assertTrue(check["required_for_cli_subset"])
        evidence = check["evidence"]
        self.assertEqual(evidence["profile"], "native-required")
        self.assertEqual(len(evidence["cases"]), 7)
        self.assertEqual(evidence["independent_artifact_identities_before"], evidence["independent_artifact_identities_after"])
        bindings = evidence["independent_artifact_identities_before"]
        self.assertEqual(set(bindings), {"cli_executable", "octave_runtime", "value_adapter", "debug_adapter", "scope_adapter", "input_adapter"})
        self.assertEqual(bindings["cli_executable"]["sha256"], self.manifest["build"]["executable_sha256"]["mathlab"])
        for kind, filename in (("value_adapter", "mathlab_value_bridge.oct"), ("debug_adapter", "mathlab_debug_bridge.oct"),
                               ("scope_adapter", "mathlab_scope_bridge.oct"), ("input_adapter", "mathlab_input_bridge.oct")):
            self.assertEqual(bindings[kind]["path"], str(self.bundle / "Contents/Resources/native" / filename))
            self.assertEqual(bindings[kind]["sha256"], self.manifest["build"]["native"]["artifact_sha256"][filename])
        pending = {item["id"]: item["status"] for item in report["pending_release_journeys"]}
        self.assertEqual(pending["desktop_ui"], "not_run")
        self.assertEqual(pending["stop_and_recovery"], "not_run")
        self.assertEqual(pending["lifecycle"], "not_run")
        self.assertFalse(report["qualification"]["release_qualified"])

    def test_omitted_packaged_api_scrubs_every_adapter_and_distinguishes_real_uint64_from_complex_rejections(self):
        self.omitted_value()
        self.environment.update(MATHLAB_VALUE_BRIDGE="/outside/value.oct", MATHLAB_SCOPE_BRIDGE="/outside/scope.oct",
                                MATHLAB_INPUT_BRIDGE="/outside/input.oct", MATHLAB_DEBUG_BRIDGE="/outside/debug.oct")
        report = self.run_qa()
        self.assertEqual(report["status"], "passed", json.dumps(report, indent=2))
        evidence = report["checks"]["packaged_value_api"]["evidence"]
        self.assertEqual(evidence["profile"], "native-unavailable")
        self.assertEqual(len(evidence["cases"]), 4)
        self.assertEqual(set(evidence["independent_artifact_identities_before"]), {"cli_executable", "octave_runtime"})
        self.assertEqual(evidence["uint64_read_write_undo_redo"], "passed")
        self.assertEqual(evidence["native_complex_read_write_undo_redo"], "unavailable_expected_rejections_passed")
        self.assertEqual(evidence["expected_unavailable_errors"], {
            "complex_read_requires_native": "MathLab:ValuePagePrecisionUnavailable",
            "complex_write_requires_native": "MathLab:ValueEditPrecisionUnavailable"})
        command = next(item for item in report["commands"] if item["id"] == "packaged-value-api")
        self.assertEqual(command["argv"][-2:], ["probe-value-api", "--expect-unavailable"])

    def test_probe_consumer_rejects_invalid_headers_cases_capabilities_and_endpoint_bindings(self):
        for required in (False, True):
            if required:
                self.required_value()
            else:
                self.omitted_value()
            for mode in ("wrong-profile", "wrong-kind", "bool-schema", "float-schema", "failed-status", "missing-error", "missing-limits", "bool-limit",
                         "missing-case", "duplicate-case", "extra-case", "failed-case", "skipped-case", "rounded-uint64",
                         "uint64-oracle-false", "missing-capability", "duplicate-capability", "wrong-value-path",
                         "wrong-cli-path", "wrong-runtime", "wrong-workspace", "wrong-binding-hash", "duplicate-binding",
                         "missing-binding", "invalid-json", "duplicate-json-key", "nan-json", "nonzero-success"):
                with self.subTest(required=required, mode=mode):
                    self.environment["QA_FAKE_PROBE_MODE"] = mode
                    report = self.run_qa()
                    self.assertEqual(report["checks"]["doctor"]["status"], "passed")
                    self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed", json.dumps(report, indent=2))
                    self.assertEqual(report["checks"]["numerical_original_script"]["status"], "not_run")
                    self.assertEqual(report["qualification"]["packaged_portable_uint64_read_edit_history"], "not_run")
                    self.assertEqual(report["qualification"]["packaged_value_read_edit_history"], "not_run")
                    self.assertEqual(report["status"], "failed")
                    self.assertFalse(report["qualification"]["release_qualified"])

    def test_required_probe_rejects_missing_admissions_native_capability_or_complex_history_journeys(self):
        self.required_value()
        for mode in ("missing-admission", "duplicate-admission", "missing-native-capability", "duplicate-journey"):
            with self.subTest(mode=mode):
                self.environment["QA_FAKE_PROBE_MODE"] = mode
                report = self.run_qa()
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed")
                self.assertEqual(report["qualification"]["packaged_native_complex_read_edit_history"], "not_run")

    def test_unavailable_probe_rejects_wrong_expected_errors_or_claimed_native_precision_success(self):
        self.omitted_value()
        for mode in ("wrong-unavailable-error", "false-native-success", "unexpected-native-capability"):
            with self.subTest(mode=mode):
                self.environment["QA_FAKE_PROBE_MODE"] = mode
                report = self.run_qa()
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed")
                self.assertEqual(report["qualification"]["packaged_portable_uint64_read_edit_history"], "not_run")

    def test_probe_cannot_hide_actual_cli_runtime_or_module_drift_behind_unchanged_json_claims(self):
        self.required_value()
        paths = [self.bundle / qa.CLI_PATH, self.runtime, self.bundle / "Contents/Resources/native/mathlab_value_bridge.oct"]
        originals = {path: path.read_bytes() for path in paths}
        for mode in ("drift-cli", "drift-runtime", "drift-module"):
            with self.subTest(mode=mode):
                self.environment["QA_FAKE_PROBE_MODE"] = mode
                report = self.run_qa()
                self.assertEqual(report["checks"]["packaged_value_api"]["status"], "failed")
                self.assertIn("independently accepted identity", report["checks"]["packaged_value_api"]["reason"])
                self.assertEqual(report["qualification"]["packaged_value_read_edit_history"], "not_run")
                for path, data in originals.items():
                    path.write_bytes(data)

    def test_repl_execution_error_fails_even_when_the_process_exits_zero(self):
        report = self.run_qa("repl-error")
        self.assertEqual(report["checks"]["persistent_repl"]["status"], "failed")
        self.assertEqual(report["status"], "failed")

    def repair_acceptance(self, name):
        acceptance = qa.Acceptance(self.bundle, self.reports)
        acceptance.temporary_root = self.root / name
        acceptance.project = acceptance.temporary_root / "project's files"
        acceptance.project.mkdir(parents=True)
        return acceptance

    def repair_execution(self, acceptance, output, workspace=None):
        return {"error": None, "output": output, "workspace": workspace or [], "figures": [],
                "working_directory": str(acceptance.project), "output_truncated": False,
                "output_dropped_bytes": 0, "output_decoding_loss": False}

    def test_repl_accepts_scalar_previews_and_rejects_arrays_nonfinite_and_wrong_shapes(self):
        acceptance = self.repair_acceptance("repl-preview")
        valid = ("42", "[42]", " [ +4.2e1 ] ", "42.", ".42E+2")
        invalid = ("41", "[41]", "[42,43]", "[42 43]", "[42;43]", "[[42]]", "[42", "42]",
                   "NaN", "[Inf]", "[1e999]", "42 trailing", "[42] trailing", "４２", "", None, 42)
        cases = [(preview, "double", [1, 1], True) for preview in valid]
        cases += [(preview, "double", [1, 1], False) for preview in invalid]
        cases += [("[42]", "char", [1, 1], False), ("[42]", "double", [1, 2], False)]
        for preview, class_name, dimensions, accepted in cases:
            with self.subTest(preview=preview, class_name=class_name, dimensions=dimensions):
                records = [self.repair_execution(acceptance, "NUMJINN_QA_REPL_FIRST 41\n"),
                           self.repair_execution(acceptance, "NUMJINN_QA_REPL_SECOND 42\n"),
                           {"workspace": [{"name": "qa_session_seed", "class_name": class_name,
                                           "dimensions": dimensions, "preview": preview}]}]
                command = ({"exit_code": 0, "timed_out": False, "output_limit_exceeded": False},
                           "\n".join(json.dumps(record) for record in records), "")
                with patch.object(acceptance, "cli", return_value=command):
                    if accepted:
                        self.assertEqual(acceptance.repl()["workspace_value"], 42)
                    else:
                        with self.assertRaises(qa.QAFailure):
                            acceptance.repl()

    def test_mat_example_adapts_only_location_and_retains_container(self):
        acceptance = self.repair_acceptance("mat-preserved")
        installed = self.bundle / "Contents/Resources/examples/getting_started.m"
        original = (b"% synthetic example\nmat_path = [tempname(), '.mat'];\n"
                    b"save(mat_path, 'A', 'x', 'y', '-v7');\nrestored = load(mat_path);\n"
                    b"assert(isequal(restored.A, A));\nassert(isequal(restored.x, x));\n"
                    b"assert(isequal(restored.y, y));\n")
        installed.write_bytes(original)
        expected = acceptance.project / "numjinn-qa-example.mat"
        content = b"MATLAB 5.0 MAT-file synthetic QA fixture".ljust(124, b" ") + b"\x00\x01IM" + b"payload"
        def fake_command(*args):
            expected.write_bytes(content)
            workspace = [{"name": name, "class_name": "double", "dimensions": shape}
                         for name, shape in (("x", [2, 1]), ("t", [1, 501]), ("y", [1, 501]))]
            output = "Solution: x = [2.000000; 3.000000], residual = 0\nVerified MAT round trip: " + str(expected) + "\n"
            return ({"exit_code": 0, "timed_out": False, "output_limit_exceeded": False},
                    json.dumps(self.repair_execution(acceptance, output, workspace)), "")
        with patch.object(acceptance, "cli", side_effect=fake_command), patch.object(acceptance, "figures", return_value=[]):
            evidence = acceptance.example()
        replacement = ("mat_path = '" + str(expected).replace("'", "''") + "';").encode()
        adapted = (acceptance.project / "getting_started.m").read_bytes()
        self.assertEqual(adapted, original.replace(b"mat_path = [tempname(), '.mat'];", replacement))
        self.assertEqual(installed.read_bytes(), original)
        self.assertEqual(evidence["source_adaptation"]["kind"], "MAT_output_location_only")
        self.assertEqual(evidence["installed_source_sha256"], hashlib.sha256(original).hexdigest())
        self.assertEqual(evidence["executed_copy_sha256"], hashlib.sha256(adapted).hexdigest())
        retained = acceptance.mat()
        self.assertEqual((acceptance.directory / retained["artifact"]["path"]).read_bytes(), content)
        self.assertFalse(retained["matlab_interchange_established"])

    def test_mat_adaptation_rejects_changed_assignment_and_existing_destination(self):
        installed = self.bundle / "Contents/Resources/examples/getting_started.m"
        assignment = b"\nmat_path = [tempname(), '.mat'];\n"
        for index, source in enumerate((b"% changed example\n", assignment + assignment)):
            acceptance = self.repair_acceptance("changed-assignment-" + str(index))
            installed.write_bytes(source)
            with patch.object(acceptance, "cli") as command, self.assertRaisesRegex(qa.QAFailure, "assignment changed"):
                acceptance.example()
            command.assert_not_called()
        for kind in ("file", "symlink"):
            acceptance = self.repair_acceptance("existing-mat-" + kind)
            installed.write_bytes(b"% example" + assignment)
            path = acceptance.project / "numjinn-qa-example.mat"
            if kind == "file":
                path.write_bytes(b"existing")
            else:
                path.symlink_to(acceptance.project / "missing-target")
            with patch.object(acceptance, "cli") as command, self.assertRaisesRegex(qa.QAFailure, "already exists"):
                acceptance.example()
            command.assert_not_called()

    def test_mat_rejects_wrong_path_missing_symlink_and_invalid_container(self):
        header = b"MATLAB 5.0 MAT-file synthetic fixture".ljust(124, b" ") + b"\x00\x01IM"
        cases = ("foreign", "sibling", "duplicate-marker", "missing", "symlink", "directory", "bad-header", "empty-container")
        for kind in cases:
            with self.subTest(kind=kind):
                acceptance = self.repair_acceptance("invalid-mat-" + kind)
                expected = acceptance.project / "numjinn-qa-example.mat"
                acceptance.example_mat_path = expected
                printed = expected
                if kind in ("foreign", "sibling"):
                    printed = (self.root if kind == "foreign" else acceptance.project) / "unexpected.mat"
                    printed.write_bytes(header + b"payload")
                elif kind == "symlink":
                    target = acceptance.project / "target.mat"
                    target.write_bytes(header + b"payload")
                    expected.symlink_to(target)
                elif kind == "directory":
                    expected.mkdir()
                elif kind != "missing":
                    expected.write_bytes(b"invalid" if kind == "bad-header" else header if kind == "empty-container" else header + b"payload")
                output = "Verified MAT round trip: " + str(printed) + "\n"
                acceptance.example_value = {"output": output * (2 if kind == "duplicate-marker" else 1)}
                with self.assertRaises(qa.QAFailure):
                    acceptance.mat()
                self.assertFalse(acceptance.report["artifacts"])

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
        diagnostic = json.dumps(command, indent=2)
        self.assertTrue(command["output_limit_exceeded"], diagnostic)
        self.assertFalse(command["timed_out"], diagnostic)
        self.assertEqual(command["cleanup_status"], "passed", diagnostic)
        self.assertLessEqual(command["stdout_bytes"] + command["stderr_bytes"], 4096)
        self.assertEqual(report["checks"]["doctor"]["status"], "failed")
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
