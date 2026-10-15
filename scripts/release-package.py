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
SCRIPT_FILES = {"build-debugger.sh", "fetch-upstream-licenses.py", "generate-notices.py",
                "license_support.py", "package-macos.sh", "setup.sh", "setup-runtime.sh", "verify.sh",
                "qa-installed.py", "qa-installed-tests.py"}
DOC_FILES = {"docs/CONSOLE_PROTOCOL.md", "docs/KERNEL_READINESS.md", "docs/qa/INSTALLED_ACCEPTANCE.md",
             "docs/WORKSPACE_PROTOCOL.md", "docs/VALUE_PAGE_PROTOCOL.md", "ROADMAP.md", "roadmap.json",
             "DEVELOPMENT_PLAN.md", "FUNCTIONALITY_CATALOG.md", "INTEGRATION_STRATEGY.md"}
BRAND_FILES = {"assets/opennumlab-logo.png"}
# Legacy format-1 verification only; new source selection never admits these.
LEGACY_BRAND_FILES = {"assets/numjinn-logo.png"}
FORBIDDEN_PARTS = {"artifacts", "target", "dist", "vendor", ".git", ".mathlab", "__pycache__",
                   "research", "evidence", "private", "local", "node_modules"}
BINARY_SUFFIXES = {".mat", ".slx", ".mdl", ".mex", ".mexmaci64", ".mexmaca64", ".oct",
                   ".o", ".a", ".so", ".dylib", ".dll", ".exe", ".p", ".pyc"}


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


def native_build(repo):
    octave = os.environ.get("MATHLAB_OCTAVE") or shutil.which("octave-cli") or shutil.which("octave")
    mkoctfile = os.environ.get("MATHLAB_MKOCTFILE") or (
        str(Path(octave).parent / "mkoctfile") if octave and (Path(octave).parent / "mkoctfile").is_file() else shutil.which("mkoctfile"))
    if not octave or not mkoctfile:
        raise RuntimeError("Native alpha requires matching Octave and mkoctfile")
    octave = os.path.abspath(shutil.which(octave) or octave)
    mkoctfile = os.path.abspath(shutil.which(mkoctfile) or mkoctfile)
    native_env = dict(os.environ, MATHLAB_OCTAVE=octave, MATHLAB_MKOCTFILE=mkoctfile)
    run(["bash", "scripts/build-debugger.sh"], repo, timeout=600, env=native_env)
    selected = run([octave, "--no-init-file", "--no-site-file", "--quiet", "--eval", "printf('%s',version());"], repo)
    built = (repo / "native/runtime-version.txt").read_text().strip()
    if selected != built:
        raise RuntimeError("Octave runtime/SDK version mismatch: " + selected + " / " + built)
    protocols = {}
    for short, version in (("debug", 4), ("scope", 2), ("input", 1)):
        module = "mathlab_" + short + "_bridge"
        module_path = str(repo / "native" / (module + ".oct")).replace("'", "''")
        expression = "autoload('{0}','{1}'); p={0}('protocol',0); assert(p.version=={2}); disp(jsonencode(p));".format(module, module_path, version)
        protocols[short] = json.loads(run([octave, "--no-init-file", "--no-site-file", "--quiet", "--eval", expression], repo, timeout=45))
    config = {key: run([mkoctfile, "-p", key], repo) for key in ("OCTAVE_VERSION", "CANONICAL_HOST_TYPE", "CXX", "CXXFLAGS", "ALL_LDFLAGS")}
    config["compiler_version"] = run(shlex.split(config["CXX"]) + ["--version"], repo)
    return {"status": "enabled_protocol_load_passed", "runtime_version": selected, "sdk": config,
            "runtime_executable_sha256": sha(Path(octave).resolve()), "protocols": protocols,
            "runtime_condition": "External Octave must match this OS, architecture, version and native ABI; protocol load is not the full native regression suite."}


def build(repo, with_debugger, target_dir=None, cargo_home=None):
    env = dict(os.environ)
    if cargo_home:
        env["CARGO_HOME"] = str(cargo_home)
    if target_dir:
        env["CARGO_TARGET_DIR"] = str(target_dir)
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
    native = native_build(repo) if with_debugger else {"status": "not_included", "reason": "--with-debugger was not selected"}
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"], repo, env=env))
    versions = {p["version"] for p in metadata["packages"] if p["name"] in {"mathlab-desktop", "mathlab-cli"}}
    if len(versions) != 1:
        raise RuntimeError("Desktop and CLI versions disagree")
    version = versions.pop()
    if run([executables["mathlab"], "--version"], repo) != "mathlab " + version:
        raise RuntimeError("Built CLI version disagrees with Cargo")
    inputs = {name: sha(path) for name, path in executables.items()}
    native_paths = [repo / "native" / name for name in (
        "mathlab_debug_bridge.oct", "mathlab_scope_bridge.oct", "mathlab_input_bridge.oct", "runtime-version.txt")] if with_debugger else []
    native["artifact_sha256"] = {path.name: sha(path) for path in native_paths}
    linked = {name: run(["otool", "-L", path], repo) for name, path in executables.items()}
    linked.update({p.name: run(["otool", "-L", p], repo) for p in native_paths if p.suffix == ".oct"})
    return {"executables": executables, "version": version, "native": native,
            "executable_sha256": inputs, "linked_libraries": linked,
            "rustc": run(["rustc", "-vV"], repo), "cargo": run(["cargo", "--version"], repo),
            "sdk": run(["xcrun", "--show-sdk-version"], repo), "platform": run(["sw_vers", "-productVersion"], repo),
            "architecture": run(["uname", "-m"], repo),
            "cargo_command": "cargo build --release --locked --message-format=json-render-diagnostics -p mathlab-desktop -p mathlab-cli",
            "configuration": {k: env[k] for k in ("CARGO_BUILD_TARGET", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "MACOSX_DEPLOYMENT_TARGET", "SDKROOT", "CC", "CXX") if k in env}}


def notices(repo, destination, cargo_home=None):
    env = dict(os.environ)
    if cargo_home:
        env["CARGO_HOME"] = str(cargo_home)
    run([sys.executable, "scripts/generate-notices.py", destination, "--require-complete"], repo, timeout=300, env=env)


def validate(bundle):
    resources = bundle / "Contents/Resources"
    manifest = json.loads((resources / "release.json").read_text())
    plist = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
    if (plist["CFBundleShortVersionString"] != manifest["version"].split("-")[0].split("+")[0]
            or plist["CFBundleVersion"] != manifest["bundle_version"]):
        raise RuntimeError("Version metadata disagrees")
    if inventory(resources / "source") != manifest["source"]["files"]:
        raise RuntimeError("Corresponding source hashes disagree")
    if any(not allowed_source(f["path"]) for f in manifest["source"]["files"]):
        raise RuntimeError("Unsafe corresponding source path")
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
            for name in ("mathlab_debug_bridge.oct", "mathlab_scope_bridge.oct", "mathlab_input_bridge.oct", "runtime-version.txt"):
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
    parser.add_argument("--with-debugger", action="store_true", help="Build and load-probe all three adapters against matching external Octave/SDK")
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
