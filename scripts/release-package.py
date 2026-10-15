#!/usr/bin/env python3
"""Build an identified macOS Development alpha with external Octave prerequisites.

No prebuilt/skip-build mode: provenance describes this invocation's builds.
Tests use synthetic repositories and injected builders.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import ctypes
import datetime
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import uuid

ROOT_FILES = {"Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "THIRD_PARTY.md"}
SCRIPT_FILES = {"build-debugger.sh", "build-value-bridge.sh", "verify-value-provisioning-test.sh",
                "fetch-upstream-licenses.py", "generate-notices.py",
                "license_support.py", "package-macos.sh", "setup.sh", "setup-runtime.sh", "verify.sh",
                "qa-installed.py", "qa-installed-tests.py"}
DOC_FILES = {"docs/CONSOLE_PROTOCOL.md", "docs/KERNEL_READINESS.md", "docs/qa/INSTALLED_ACCEPTANCE.md",
             "docs/WORKSPACE_PROTOCOL.md", "docs/VALUE_PAGE_PROTOCOL.md", "docs/VALUE_EDIT_PROTOCOL.md",
             "ROADMAP.md", "roadmap.json",
             "DEVELOPMENT_PLAN.md", "FUNCTIONALITY_CATALOG.md", "INTEGRATION_STRATEGY.md"}
BRAND_FILES = {"assets/opennumlab-logo.png"}
# Legacy format-1 verification only; new source selection never admits these.
LEGACY_BRAND_FILES = {"assets/numjinn-logo.png"}
FORBIDDEN_PARTS = {"artifacts", "target", "dist", "vendor", ".git", ".mathlab", "__pycache__",
                   "research", "evidence", "private", "local", "node_modules"}
BINARY_SUFFIXES = {".mat", ".slx", ".mdl", ".mex", ".mexmaci64", ".mexmaca64", ".oct",
                   ".o", ".a", ".so", ".dylib", ".dll", ".exe", ".p", ".pyc"}
NATIVE_MODULES = tuple("mathlab_" + name + "_bridge.oct" for name in ("debug", "scope", "input", "value"))
NATIVE_FILES = NATIVE_MODULES + ("runtime-version.txt",)
VALUE_PATH = "Contents/Resources/native/mathlab_value_bridge.oct"
VALUE_CAPABILITY = "dense_complex_native_components.v1"
VALUE_REQUIRED_CAPABILITIES = {"typed_nd_page.v1", "atomic_range_edit.v1", VALUE_CAPABILITY}
VALUE_PROTOCOL = {"protocol": "mathlab-value", "version": 1, "dense_complex_components": True,
                  "max_cells": 2000, "max_rank": 32, "max_replace_leaf_bytes": 134217728}
NATIVE_REQUIRED_PROFILE = {"schema_version": 1, "name": "four-adapter-value-v1"}
NATIVE_OMITTED_PROFILE = {"schema_version": 1, "name": "native-omitted-v1"}
VALUE_OMITTED_COMPONENT = {"status": "not_included", "reason": "native_profile_not_requested"}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def valid_sha256(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON field: " + key)
        result[key] = value
    return result


def validate_value_protocol(protocol):
    require(isinstance(protocol, dict) and set(protocol) == set(VALUE_PROTOCOL)
            and all(type(protocol[key]) is type(expected) and protocol[key] == expected
                    for key, expected in VALUE_PROTOCOL.items()),
            "Value adapter must report the exact mathlab-value v1 protocol and limit profile")


def allowed_source(name):
    """Tracked-source allowlist with overriding binary/local-evidence denial."""
    p = PurePosixPath(name)
    if any(ord(character) < 32 or ord(character) == 127 for character in name):
        return False
    if not p.parts or p.is_absolute() or ".." in p.parts or any(x.lower() in FORBIDDEN_PARTS for x in p.parts):
        return False
    if p.suffix.lower() in BINARY_SUFFIXES or any(x.startswith(".") for x in p.parts):
        return False
    if name in ROOT_FILES or name in DOC_FILES or name in BRAND_FILES:
        return True
    if p.parts[0] == "crates":
        return p.suffix in {".rs", ".toml", ".md", ".m"}
    if p.parts[0] in {"octave", "examples"}:
        return p.suffix == ".m"
    if p.parts[:2] == ("tests", "fixtures"):
        return p.suffix in {".m", ".json"}
    if p.parts[0] == "compatibility":
        if name == "compatibility/conformance/build_oracles.py":
            return True
        return (len(p.parts) == 2 and p.suffix in {".json", ".md"}) or (
            p.parts[:2] == ("compatibility", "conformance") and p.suffix in {".m", ".json", ".md"})
    if p.parts[0] == "native":
        return len(p.parts) == 2 and p.suffix in {".cc", ".h", ".md"}
    if p.parts[0] == "scripts" and len(p.parts) == 2:
        return p.name in SCRIPT_FILES or (p.name.startswith("release-") and p.suffix in {".py", ".sh", ".json"})
    if p.parts[:2] == ("docs", "release"):
        return len(p.parts) == 3 and p.suffix == ".md"
    if p.parts[:2] == ("third-party", "licenses"):
        return p.suffix.lower() in {".json", ".txt", ".md", ".toml"} or p.name.startswith(("LICENSE", "COPYING", "NOTICE"))
    return False


def run(args, cwd, timeout=120, env=None):
    command = [str(x) for x in args]
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                               text=True, start_new_session=True)
    try:
        output, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise
    if process.returncode:
        raise subprocess.CalledProcessError(process.returncode, command, output=output)
    return output.strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(root):
    result = []
    # Git/source manifests sort complete POSIX names. Path's component ordering
    # differs for siblings such as conformance.rs and conformance/reference.rs.
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        if path.is_symlink():
            raise RuntimeError("Symlinks are forbidden in release payload: " + str(path))
        if path.is_file():
            result.append({"path": path.relative_to(root).as_posix(), "sha256": sha(path),
                           "size": path.stat().st_size, "executable": bool(path.stat().st_mode & 0o111)})
    return result


def source_snapshot(repo):
    names = run(["git", "ls-files", "-z"], repo).split("\0")
    paths = sorted(name for name in names if name and allowed_source(name))
    if not ROOT_FILES.issubset(paths):
        raise RuntimeError("Missing tracked source root files")
    # Cargo compiles untracked Rust modules/build scripts too. Commit intended
    # additions so corresponding source cannot silently omit those inputs.
    untracked = run(["git", "ls-files", "--others", "--exclude-standard", "-z"], repo).split("\0")
    missing = [name for name in untracked if name and allowed_source(name)]
    if missing:
        raise RuntimeError("Commit intended release source before packaging: " + ", ".join(missing))
    files = []
    for name in paths:
        path = repo / name
        if path.is_symlink() or any((repo / parent).is_symlink() for parent in path.relative_to(repo).parents):
            raise RuntimeError("Source symlink is forbidden: " + name)
        if path.is_file():
            files.append({"path": name, "sha256": sha(path), "size": path.stat().st_size,
                          "executable": bool(path.stat().st_mode & 0o111)})
    return {"commit": run(["git", "rev-parse", "HEAD"], repo),
            "dirty": bool(run(["git", "status", "--porcelain", "--untracked-files=normal"], repo)),
            "files": files}


def copy_source(repo, destination, snapshot, from_commit=False):
    if from_commit:
        # Read a fixed commit, never a moving HEAD or shared worktree. Compare
        # against the captured inventory to reject races and export-subst rules.
        names = {r["path"] for r in snapshot["files"]}
        data = subprocess.check_output(["git", "archive", "--format=tar", snapshot["commit"], "--", *sorted(names)], cwd=repo, timeout=120)
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            for member in archive:
                if member.isdir():
                    continue
                if not member.isfile() or member.name not in names:
                    raise RuntimeError("Unexpected commit export member: " + member.name)
                output = destination / member.name
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(archive.extractfile(member).read())
                output.chmod(0o755 if member.mode & 0o111 else 0o644)
    else:
        for record in snapshot["files"]:
            output = destination / record["path"]
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(repo / record["path"], output)
    if inventory(destination) != snapshot["files"]:
        raise RuntimeError("Source changed while copying")


def frozen_git_identity(repo, frozen, commit):
    """Give build scripts the frozen revision, isolated from live ancestor HEAD.

    This local metadata is never copied into the distributed source. Objects
    remain in the existing checkout; no source/evidence history is recopied.
    """
    run(["git", "init", "--quiet", "--template=", frozen], repo)
    objects = Path(run(["git", "rev-parse", "--git-path", "objects"], repo))
    if not objects.is_absolute():
        objects = repo / objects
    (frozen / ".git/objects/info/alternates").write_text(str(objects.resolve()) + "\n")
    (frozen / ".git/HEAD").write_text(commit + "\n")
    if run(["git", "rev-parse", "HEAD"], frozen) != commit:
        raise RuntimeError("Frozen build revision disagrees")


def native_preflight(repo):
    octave = os.environ.get("MATHLAB_OCTAVE") or os.environ.get("OCTAVE_EXECUTABLE") or shutil.which("octave-cli") or shutil.which("octave")
    mkoctfile = os.environ.get("MATHLAB_MKOCTFILE") or (
        str(Path(octave).parent / "mkoctfile") if octave and (Path(octave).parent / "mkoctfile").is_file() else shutil.which("mkoctfile"))
    if not octave or not mkoctfile:
        raise RuntimeError("Native value profile requires Octave and matching mkoctfile/development headers; set MATHLAB_OCTAVE and MATHLAB_MKOCTFILE")
    octave = os.path.abspath(shutil.which(octave) or octave)
    mkoctfile = os.path.abspath(shutil.which(mkoctfile) or mkoctfile)
    for name, path in (("Octave", octave), ("mkoctfile", mkoctfile)):
        require(Path(path).is_file() and os.access(path, os.X_OK), "Native prerequisite is missing or not executable: " + name + " " + path)
    selected = run([octave, "--no-init-file", "--no-site-file", "--quiet", "--eval", "printf('%s',version());"], repo)
    sdk = run([mkoctfile, "-p", "OCTAVE_VERSION"], repo)
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[+~-][A-Za-z0-9.+_~-]*)?", selected) and sdk == selected,
            "Native runtime/SDK mismatch: runtime=" + selected + " SDK=" + sdk + "; select matching MATHLAB_OCTAVE and MATHLAB_MKOCTFILE")
    expected = os.environ.get("MATHLAB_EXPECT_OCTAVE_VERSION")
    require(not expected or selected == expected, "Expected Octave " + str(expected) + ", found " + selected)
    return {"octave": octave, "mkoctfile": mkoctfile, "version": selected,
            "runtime_executable_sha256": sha(Path(octave).resolve())}


def native_build(repo, tools=None):
    tools = tools or native_preflight(repo)
    octave, mkoctfile, selected = tools["octave"], tools["mkoctfile"], tools["version"]
    native_env = dict(os.environ, MATHLAB_OCTAVE=octave, MATHLAB_MKOCTFILE=mkoctfile)
    require(not any((repo / "native" / name).exists() for name in NATIVE_FILES),
            "Native package build requires empty frozen native outputs; refusing preexisting adapters")
    run(["bash", "scripts/build-debugger.sh"], repo, timeout=600, env=native_env)
    built = (repo / "native/runtime-version.txt").read_text().strip()
    if selected != built:
        raise RuntimeError("Octave runtime/SDK version mismatch: " + selected + " / " + built)
    protocols = {}
    for short, version in (("debug", 4), ("scope", 2), ("input", 1), ("value", 1)):
        module = "mathlab_" + short + "_bridge"
        path = repo / "native" / (module + ".oct")
        require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0,
                "Native build did not produce a fresh nonempty module: " + path.name)
        module_path = str(path.resolve()).replace("'", "''")
        arguments = "'protocol'" if short == "value" else "'protocol',0"
        expression = "autoload('{0}','{1}'); p={0}({2}); assert(p.version=={3}); disp(jsonencode(p));".format(module, module_path, arguments, version)
        protocols[short] = json.loads(run([octave, "--no-init-file", "--no-site-file", "--quiet", "--eval", expression], repo, timeout=45), object_pairs_hook=unique_object)
        if short == "value":
            validate_value_protocol(protocols[short])
        else:
            protocol_name = "mathlab-debugger" if short == "debug" else "mathlab-" + short
            require(isinstance(protocols[short], dict) and protocols[short].get("protocol") == protocol_name
                    and type(protocols[short].get("version")) is int and protocols[short]["version"] == version,
                    "Native adapter protocol disagrees: " + short)
    config = {key: run([mkoctfile, "-p", key], repo) for key in ("OCTAVE_VERSION", "CANONICAL_HOST_TYPE", "CXX", "CXXFLAGS", "ALL_LDFLAGS")}
    config["compiler_version"] = run(shlex.split(config["CXX"]) + ["--version"], repo)
    require(config["OCTAVE_VERSION"] == selected and sha(Path(octave).resolve()) == tools["runtime_executable_sha256"],
            "Native runtime/SDK identity changed during compilation")
    return {"status": "enabled_protocol_load_passed", "adapter_profile": dict(NATIVE_REQUIRED_PROFILE),
            "runtime_version": selected, "sdk": config,
            "runtime_executable_sha256": tools["runtime_executable_sha256"], "protocols": protocols,
            "runtime_condition": "External Octave must match this OS, architecture, version and native ABI; protocol load is not the full native regression suite."}


def observe_value_capability(repo, cli, tools, native):
    """Require an actual Engine handshake from this invocation's identified CLI.

    Doctor fields are a CLI-owner integration prerequisite. Until supplied, a
    native package fails here rather than inferring capabilities from a path.
    """
    environment = dict(os.environ, MATHLAB_OCTAVE=tools["octave"])
    cli_hash = sha(cli)
    for setting in ("DEBUG", "SCOPE", "INPUT", "VALUE"):
        environment["MATHLAB_" + setting + "_BRIDGE"] = str((repo / "native" / ("mathlab_" + setting.lower() + "_bridge.oct")).resolve())
    with tempfile.TemporaryDirectory(prefix="opennumlab-value-probe-") as project:
        doctor = json.loads(run([cli, "--json", "--octave", tools["octave"], "--directory", project, "doctor"],
                               repo, timeout=60, env=environment), object_pairs_hook=unique_object)
    require(isinstance(doctor, dict) and doctor.get("ready") is True,
            "Native value capability probe did not report a ready worker")
    capabilities = doctor.get("worker_capabilities")
    require(isinstance(capabilities, list) and all(isinstance(item, str) for item in capabilities)
            and len(set(capabilities)) == len(capabilities) and VALUE_REQUIRED_CAPABILITIES.issubset(capabilities),
            "Required value profile needs CLI doctor worker_capabilities from the loaded worker; coordinate the CLI owner (protocol/path alone is insufficient)")
    module = (repo / "native/mathlab_value_bridge.oct").resolve()
    require(isinstance(doctor.get("value_adapter"), str) and doctor["value_adapter"] == str(module),
            "CLI doctor did not load the freshly built value_adapter")
    require(isinstance(doctor.get("runtime"), str) and Path(doctor["runtime"]).is_absolute()
            and Path(doctor["runtime"]).resolve() == Path(tools["octave"]).resolve(),
            "CLI doctor used a different runtime for the value capability probe")
    require(sha(module) == native["artifact_sha256"][module.name]
            and sha(Path(tools["octave"]).resolve()) == native["runtime_executable_sha256"] and sha(cli) == cli_hash,
            "CLI/runtime/module identity changed during the value capability probe")
    return {"method": "built-cli-doctor", "ready": True, "cli_executable_sha256": cli_hash,
            "value_adapter": str(module), "runtime": doctor["runtime"],
            "worker_capabilities": capabilities, "module_sha256": sha(module),
            "runtime_executable_sha256": native["runtime_executable_sha256"]}


def build(repo, with_debugger, target_dir=None, cargo_home=None):
    env = dict(os.environ)
    if cargo_home:
        env["CARGO_HOME"] = str(cargo_home)
    if target_dir:
        env["CARGO_TARGET_DIR"] = str(target_dir)
    tools = native_preflight(repo) if with_debugger else None
    output = run(["cargo", "build", "--release", "--locked", "--message-format=json-render-diagnostics",
                  "-p", "mathlab-desktop", "-p", "mathlab-cli"], repo, timeout=1800, env=env)
    executables = {}
    for line in output.splitlines():
        item = json.loads(line)
        if (item.get("reason") == "compiler-artifact" and item.get("executable")
                and item["target"]["name"] in {"mathlab-desktop", "mathlab"}):
            executables[item["target"]["name"]] = Path(item["executable"])
    if not {"mathlab-desktop", "mathlab"}.issubset(executables):
        raise RuntimeError("Cargo did not identify both release executables")
    native = native_build(repo, tools) if with_debugger else {
        "status": "not_included", "adapter_profile": dict(NATIVE_OMITTED_PROFILE),
        "value_component": dict(VALUE_OMITTED_COMPONENT),
        "reason": "--with-debugger was not selected; exact dense-complex native support is unavailable"}
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"], repo, env=env))
    versions = {p["version"] for p in metadata["packages"] if p["name"] in {"mathlab-desktop", "mathlab-cli"}}
    if len(versions) != 1:
        raise RuntimeError("Desktop and CLI versions disagree")
    version = versions.pop()
    if run([executables["mathlab"], "--version"], repo) != "mathlab " + version:
        raise RuntimeError("Built CLI version disagrees with Cargo")
    inputs = {name: sha(path) for name, path in executables.items()}
    native_paths = [repo / "native" / name for name in NATIVE_FILES] if with_debugger else []
    native["artifact_sha256"] = {path.name: sha(path) for path in native_paths}
    if with_debugger:
        native["worker_observation"] = observe_value_capability(repo, executables["mathlab"], tools, native)
        native["value_component"] = {"status": "required", "path": VALUE_PATH, "capability": VALUE_CAPABILITY,
                                     "sha256": native["artifact_sha256"]["mathlab_value_bridge.oct"]}
    linked = {name: run(["otool", "-L", path], repo) for name, path in executables.items()}
    linked.update({p.name: run(["otool", "-L", p], repo) for p in native_paths if p.suffix == ".oct"})
    return {"executables": executables, "version": version, "native": native,
            "executable_sha256": inputs, "linked_libraries": linked,
            "rustc": run(["rustc", "-vV"], repo), "cargo": run(["cargo", "--version"], repo),
            "sdk": run(["xcrun", "--show-sdk-version"], repo), "platform": run(["sw_vers", "-productVersion"], repo),
            "architecture": run(["uname", "-m"], repo),
            "cargo_command": "cargo build --release --locked --message-format=json-render-diagnostics -p mathlab-desktop -p mathlab-cli",
            "configuration": {k: v for k, v in env.items() if k.startswith("CARGO_PROFILE_RELEASE_") or k in (
                "CARGO_BUILD_TARGET", "CARGO_INCREMENTAL", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "MACOSX_DEPLOYMENT_TARGET", "SDKROOT", "CC", "CXX")}}


def notices(repo, destination, cargo_home=None):
    env = dict(os.environ)
    if cargo_home:
        env["CARGO_HOME"] = str(cargo_home)
    run([sys.executable, "scripts/generate-notices.py", destination, "--require-complete"], repo, timeout=300, env=env)


def validate_native(bundle, build_identity):
    """Validate additive adapter declarations; old manifests never imply value support."""
    require(isinstance(build_identity, dict), "Malformed build identity")
    native = build_identity.get("native")
    require(isinstance(native, dict), "Missing native component identity")
    value_paths = [path.relative_to(bundle).as_posix() for path in bundle.rglob("*")
                   if path.name.lower() == "mathlab_value_bridge.oct"]
    module_paths = [path.relative_to(bundle).as_posix() for path in bundle.rglob("*") if path.suffix.lower() == ".oct"]
    artifacts = native.get("artifact_sha256", {})
    protocols = native.get("protocols", {})
    require(isinstance(artifacts, dict) and isinstance(protocols, dict), "Malformed native artifact/protocol identity")
    if "adapter_profile" not in native:
        require(not value_paths and not any(name.lower() == "mathlab_value_bridge.oct" for name in artifacts)
                and not any(name.lower() == "value" for name in protocols)
                and "worker_observation" not in native and "value_component" not in native,
                "Legacy adapter manifest must not bundle or declare a value adapter")
        return
    profile = native["adapter_profile"]
    require(isinstance(profile, dict) and set(profile) == {"schema_version", "name"}
            and type(profile["schema_version"]) is int and profile["schema_version"] == 1
            and profile["name"] in (NATIVE_REQUIRED_PROFILE["name"], NATIVE_OMITTED_PROFILE["name"]),
            "Unknown or malformed native adapter_profile declaration")
    if profile == NATIVE_OMITTED_PROFILE:
        require(native.get("status") == "not_included" and isinstance(native.get("reason"), str) and native["reason"].strip()
                and not artifacts and not protocols and "worker_observation" not in native
                and native.get("value_component") == VALUE_OMITTED_COMPONENT
                and not module_paths,
                "Omitted native profile must not bundle or declare native/value adapters")
        return
    require(native.get("status") == "enabled_protocol_load_passed", "Required value profile did not pass native protocol loading")
    require(value_paths == [VALUE_PATH], "Required value profile needs exactly the canonical installed value module")
    require(set(module_paths) == {"Contents/Resources/native/" + name for name in NATIVE_MODULES},
            "Required value profile must contain exactly four canonical native modules")
    require(set(artifacts) == set(NATIVE_FILES) and all(valid_sha256(value) for value in artifacts.values()),
            "Required value profile needs all four module hashes and runtime-version hash")
    for name in NATIVE_FILES:
        path = bundle / "Contents/Resources/native" / name
        require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0 and sha(path) == artifacts[name],
                "Required native artifact is missing or differs: " + name)
    runtime_hash = native.get("runtime_executable_sha256")
    require(valid_sha256(runtime_hash), "Required value profile needs the selected runtime executable SHA-256")
    require(native.get("value_component") == {"status": "required", "path": VALUE_PATH,
            "capability": VALUE_CAPABILITY, "sha256": artifacts["mathlab_value_bridge.oct"]},
            "Required value component declaration disagrees with its path/hash/capability")
    version = native.get("runtime_version")
    require(isinstance(version, str) and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[+~-][A-Za-z0-9.+_~-]*)?", version)
            and isinstance(native.get("sdk"), dict) and native["sdk"].get("OCTAVE_VERSION") == version
            and (bundle / "Contents/Resources/native/runtime-version.txt").read_text().strip() == version,
            "Required value profile runtime/SDK/version marker disagree")
    require(set(protocols) == {"debug", "scope", "input", "value"}, "Required value profile needs four observed protocols")
    for name, protocol_name, version in (("debug", "mathlab-debugger", 4), ("scope", "mathlab-scope", 2), ("input", "mathlab-input", 1)):
        protocol = protocols[name]
        require(isinstance(protocol, dict) and protocol.get("protocol") == protocol_name
                and type(protocol.get("version")) is int and protocol["version"] == version,
                "Required native protocol disagrees: " + name)
    validate_value_protocol(protocols["value"])
    links = build_identity.get("linked_libraries")
    require(isinstance(links, dict) and all(isinstance(links.get(name), str) and links[name].strip() for name in NATIVE_MODULES),
            "Required value profile needs linked-library inspection for all four modules")
    observation = native.get("worker_observation")
    require(isinstance(observation, dict) and set(observation) == {
        "method", "ready", "cli_executable_sha256", "value_adapter", "runtime", "worker_capabilities",
        "module_sha256", "runtime_executable_sha256"},
        "Required value profile needs an identified worker observation")
    capabilities = observation["worker_capabilities"]
    require(observation["method"] == "built-cli-doctor"
            and observation["ready"] is True and valid_sha256(observation["cli_executable_sha256"])
            and isinstance(build_identity.get("executable_sha256"), dict)
            and observation["cli_executable_sha256"] == build_identity["executable_sha256"].get("mathlab")
            and observation["cli_executable_sha256"] == sha(bundle / "Contents/MacOS/mathlab")
            and isinstance(capabilities, list) and all(isinstance(item, str) for item in capabilities)
            and len(set(capabilities)) == len(capabilities) and VALUE_REQUIRED_CAPABILITIES.issubset(capabilities)
            and observation["module_sha256"] == artifacts["mathlab_value_bridge.oct"]
            and observation["runtime_executable_sha256"] == runtime_hash,
            "Required value profile worker capability or runtime/module identity disagrees")
    require(isinstance(observation["value_adapter"], str) and Path(observation["value_adapter"]).is_absolute()
            and Path(observation["value_adapter"]).name == "mathlab_value_bridge.oct"
            and isinstance(observation["runtime"], str) and Path(observation["runtime"]).is_absolute(),
            "Required value profile needs absolute observed runtime/value paths")


def validate(bundle):
    resources = bundle / "Contents/Resources"
    manifest = json.loads((resources / "release.json").read_text(), object_pairs_hook=unique_object)
    plist = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
    if (plist["CFBundleShortVersionString"] != manifest["version"].split("-")[0].split("+")[0]
            or plist["CFBundleVersion"] != manifest["bundle_version"]):
        raise RuntimeError("Version metadata disagrees")
    if inventory(resources / "source") != manifest["source"]["files"]:
        raise RuntimeError("Corresponding source hashes disagree")
    if any(not allowed_source(f["path"]) for f in manifest["source"]["files"]):
        raise RuntimeError("Unsafe corresponding source path")
    validate_native(bundle, manifest["build"])
    current = {r["path"]: r for r in inventory(bundle)}
    for record in manifest["payload"]:
        path = bundle / record["path"]
        if current.get(record["path"]) != record:
            raise RuntimeError("Payload hash mismatch: " + record["path"])
    expected = {r["path"] for r in manifest["payload"]} | {"Contents/Resources/release.json", "Contents/Resources/checksums.sha256"}
    records = inventory(bundle)
    if expected != {r["path"] for r in records}:
        raise RuntimeError("Unmanifested payload")
    records = [r for r in records if r["path"] != "Contents/Resources/checksums.sha256"]
    if (resources / "checksums.sha256").read_text() != "".join(r["sha256"] + "  " + r["path"] + "\n" for r in records):
        raise RuntimeError("Final checksums disagree")


def atomic_exchange(first, second):
    """Exchange same-filesystem directories without a missing-destination window.

    macOS SDK sys/stdio.h declares renamex_np and RENAME_SWAP=2. Linux's
    renameat2(RENAME_EXCHANGE=2) supports the portable synthetic CI tests.
    Unsupported filesystems fail with the installed app still in place.
    """
    library = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        operation = library.renamex_np
        operation.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        arguments = (os.fsencode(first), os.fsencode(second), 2)
    elif sys.platform.startswith("linux"):
        operation = library.renameat2
        operation.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        arguments = (-100, os.fsencode(first), -100, os.fsencode(second), 2)
    else:
        raise RuntimeError("Atomic app exchange unsupported on this host")
    operation.restype = ctypes.c_int
    if operation(*arguments) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))


def publish(candidate, destination):
    """Atomically replace the app and retain its predecessor outside staging."""
    if destination.exists() or destination.is_symlink():
        backup = destination.with_name(destination.name + ".previous-" + uuid.uuid4().hex[:12])
        # Move the validated candidate out of temporary staging BEFORE swapping.
        # An interruption after the swap must never let staging cleanup delete
        # the previous app. Until the swap succeeds destination remains usable.
        candidate.rename(backup)
        try:
            atomic_exchange(backup, destination)
        except OSError:
            backup.rename(candidate)
            raise
        return backup
    candidate.rename(destination)
    return None


def package(repo, destination, with_debugger=False, builder=build, notice_builder=notices, allow_dirty=False):
    repo, destination = repo.resolve(), destination.absolute()
    if destination.suffix != ".app" or destination.parent.is_symlink():
        raise RuntimeError("Destination must be an .app in a real directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    before = source_snapshot(repo)
    if before["dirty"] and not allow_dirty:
        raise RuntimeError("Release candidates require a clean checkout; --allow-dirty creates an unqualified local build only")
    before["export_sha256"] = hashlib.sha256(json.dumps(before["files"], sort_keys=True).encode()).hexdigest()
    before["frozen_export"] = True
    with tempfile.TemporaryDirectory(prefix=".opennumlab-stage-", dir=destination.parent) as work, tempfile.TemporaryDirectory(prefix="opennumlab-build-") as build_work:
        # Outside checkout ancestry: no live .cargo config or ancestor .git.
        frozen = Path(build_work) / "source"
        copy_source(repo, frozen, before, from_commit=not before["dirty"])
        frozen_git_identity(repo, frozen, before["commit"])
        cargo_home = Path(build_work) / "cargo-home"
        cargo_home.mkdir()
        original_cargo = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
        for name in ("registry", "git"):
            if (original_cargo / name).exists():
                (cargo_home / name).symlink_to((original_cargo / name).resolve(), target_is_directory=True)
        # Build only the frozen export into a private target. A shared target's
        # binaries can be replaced after Cargo releases its lock, before hashing.
        if builder is build:
            target = Path(build_work) / "cargo-target"
            identity = builder(frozen, with_debugger, target_dir=target, cargo_home=cargo_home)
        else:
            identity = builder(frozen, with_debugger)
        expected_profile = NATIVE_REQUIRED_PROFILE if with_debugger else NATIVE_OMITTED_PROFILE
        require(identity.get("native", {}).get("adapter_profile") == expected_profile,
                "New package assembly requires the explicit requested adapter_profile")
        for record in before["files"]:
            if sha(frozen / record["path"]) != record["sha256"]:
                raise RuntimeError("Frozen source changed during build: " + record["path"])
        bundle = Path(work) / "OpenNumLab.app"
        contents, resources = bundle / "Contents", bundle / "Contents/Resources"
        (contents / "MacOS").mkdir(parents=True)
        source = resources / "source"
        copy_source(frozen, source, before)
        for name, executable in identity["executables"].items():
            shutil.copy2(executable, contents / "MacOS" / name)
            if sha(contents / "MacOS" / name) != identity["executable_sha256"][name]:
                raise RuntimeError("Built executable changed before copy: " + name)
        shutil.copy2(source / "scripts/release-launcher.sh", contents / "MacOS/mathlab-launcher")
        (contents / "MacOS/mathlab-launcher").chmod(0o755)
        for name in ("LICENSE", "README.md", "THIRD_PARTY.md"):
            shutil.copy2(source / name, resources / name)
        for name in BRAND_FILES:
            if (source / name).is_file():
                shutil.copy2(source / name, resources / Path(name).name)
        shutil.copytree(source / "examples", resources / "examples")
        if with_debugger:
            (resources / "native").mkdir()
            for name in NATIVE_FILES:
                shutil.copy2(frozen / "native" / name, resources / "native" / name)
                if sha(resources / "native" / name) != identity["native"]["artifact_sha256"][name]:
                    raise RuntimeError("Native artifact changed before copy: " + name)
        if notice_builder is notices:
            notice_builder(frozen, resources / "licenses", cargo_home=cargo_home)
        else:
            notice_builder(frozen, resources / "licenses")
        version = identity["version"]
        match = re.fullmatch(r"(\d+\.\d+\.\d+)(?:[-+][0-9A-Za-z.+-]+)?", version)
        if not match:
            raise RuntimeError("Version is not a supported Cargo semantic version")
        bundle_version = run(["git", "rev-list", "--count", before["commit"]], repo)
        (contents / "Info.plist").write_bytes(plistlib.dumps({
            "CFBundleName": "OpenNumLab", "CFBundleDisplayName": "OpenNumLab (Development alpha)",
            "CFBundleIdentifier": "org.openmathlab.desktop", "CFBundleExecutable": "mathlab-launcher",
            "CFBundlePackageType": "APPL", "CFBundleShortVersionString": match.group(1),
            "CFBundleVersion": bundle_version, "NSHighResolutionCapable": True}))
        manifest = {"schema_version": 1, "product": "OpenNumLab", "profile": "Development alpha", "version": version,
                    "bundle_version": bundle_version, "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "source": before, "build": {k: v for k, v in identity.items() if k not in {"executables", "version"}},
                    "qualification": {"status": "not_qualified", "installed_tests": "not_run",
                                      "native_regression_suite": "not_run", "public_source_review": "not_run"},
                    "distribution": {"signed": False, "notarized": False, "self_contained": False,
                        "clean_candidate": not before["dirty"],
                        "build_configuration": "Frozen commit export outside checkout ancestry; isolated Cargo home reuses registry/git caches without inheriting user Cargo config",
                        "prerequisites": ["External GNU Octave (octave-cli or octave on PATH, Homebrew path, or MATHLAB_OCTAVE)",
                                          "External gnuplot/working Octave graphics toolkit and fonts for figures",
                                          "control package for control_response.m; other scientific packages remain optional"],
                        "claims": "Implemented Development alpha only; no Core/Full or MATLAB parity qualification",
                        "writable_state": "User project outside app; default ~/Library/Application Support/OpenNumLab/Projects/Getting Started/.mathlab/",
                        "dependency_sources": "Cargo.lock and upstream license inventory included; Cargo registry sources, SDK, Octave and packages remain external build prerequisites"},
                    "payload": inventory(bundle)}
        (resources / "release.json").write_text(json.dumps(manifest, indent=2) + "\n")
        (resources / "checksums.sha256").write_text("".join(r["sha256"] + "  " + r["path"] + "\n" for r in inventory(bundle)))
        validate(bundle)
        backup = publish(bundle, destination)
    return {"bundle": str(destination), "previous_bundle": str(backup) if backup else None,
            "source_commit": before["commit"], "source_dirty": before["dirty"],
            "checksums_sha256": sha(destination / "Contents/Resources/checksums.sha256")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--with-debugger", action="store_true", help="Build/probe all four adapters against matching Octave/SDK and require observed worker value capability")
    parser.add_argument("--allow-dirty", action="store_true", help="Unqualified local build from a hashed frozen export; accepted candidates require clean source")
    parser.add_argument("--output", type=Path, help="App destination (default dist/OpenNumLab.app); previous app retained after replacement")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This application packaging lane requires macOS")
    repo = Path(__file__).resolve().parent.parent
    result = package(repo, args.output or repo / "dist/OpenNumLab.app", args.with_debugger, allow_dirty=args.allow_dirty)
    print(json.dumps(result, indent=2))
    print("Unsigned Development alpha. External Octave and a graphics toolkit/fonts are required.")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.SubprocessError, OSError) as error:
        sys.exit("Packaging failed; existing app retained: " + str(error))
