#!/usr/bin/env python3
"""Create or verify a deterministic, allowlisted corresponding-project-source set.

No build, runtime invocation, Git-history distribution or publication occurs.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import tarfile
import tempfile
import unicodedata
import zlib

SCRIPTS = Path(__file__).resolve().parent
POLICY_PATH = "scripts/release-package.py"
EXPORTER_PATH = "scripts/release-source.py"
MANIFEST = "source-manifest.json"
CHECKSUMS = "checksums.sha256"
HEX = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?\Z")
CURRENT_SCHEMA = 2
CURRENT_PRODUCT = "OpenNumLab"
FORMAT_IDENTITIES = {1: ("NumJinn", "numjinn-source-"),
                     2: (CURRENT_PRODUCT, "opennumlab-source-")}
spec = importlib.util.spec_from_file_location("source_selection_policy", SCRIPTS / "release-package.py")
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def format_identity(schema_version, product):
    """Return the exact archive prefix; schema 1 remains verification only."""
    require(type(schema_version) is int and schema_version in FORMAT_IDENTITIES,
            "Unsupported source format schema")
    expected_product, prefix = FORMAT_IDENTITIES[schema_version]
    require(product == expected_product, "Source schema/product identity disagrees")
    return prefix


def allowed_source_for_schema(name, schema_version):
    # Each exact brand asset belongs only to its own format generation.
    legacy = getattr(policy, "LEGACY_BRAND_FILES", {"assets/numjinn-logo.png"})
    if name in legacy:
        return schema_version == 1
    if name in policy.BRAND_FILES:
        return schema_version == 2
    return policy.allowed_source(name)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical_json(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode()


def inventory_digest(files):
    return digest(json.dumps(files, sort_keys=True, separators=(",", ":")).encode())


def git(repo, *arguments):
    # A supplied repo must not be redirected by inherited Git environment or
    # refs/replace. No user configuration, hooks or remote operations are used.
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update(GIT_NO_REPLACE_OBJECTS="1", GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    return subprocess.check_output(["git", "--no-replace-objects", "-C", str(repo), *arguments],
                                   env=env, timeout=120)


def resolve_commit(repo, reference):
    commit = git(repo, "rev-parse", "--verify", "--end-of-options", reference + "^{commit}").decode().strip()
    require(HEX.fullmatch(commit), "Ref did not resolve to a full commit identity")
    return commit


def portable_names(names):
    prefixes = {}
    files = set(names)
    for name in names:
        path = PurePosixPath(name)
        require(path.parts and not path.is_absolute() and path.as_posix() == name
                and ".." not in path.parts and "\\" not in name and ":" not in name
                and not any(ord(c) < 32 or ord(c) == 127 for c in name), "Unsafe source path: " + name)
        for length in range(1, len(path.parts) + 1):
            prefix = "/".join(path.parts[:length])
            require(length == len(path.parts) or prefix not in files,
                    "File/directory extraction collision: " + prefix)
            key = unicodedata.normalize("NFC", prefix).casefold()
            require(key not in prefixes or prefixes[key] == prefix, "Portable extraction collision: " + prefix)
            prefixes[key] = prefix


def committed_files(repo, commit):
    files, contents = [], {}
    entries = git(repo, "ls-tree", "-rz", "--full-tree", commit).split(b"\0")
    selected = []
    for entry in entries:
        if not entry:
            continue
        metadata, raw_name = entry.split(b"\t", 1)
        name = raw_name.decode("utf-8")
        if not allowed_source_for_schema(name, CURRENT_SCHEMA):
            continue
        mode, kind, object_id = metadata.decode("ascii").split()
        require(kind == "blob" and mode in {"100644", "100755"}, "Selected source is not a regular Git file: " + name)
        selected.append((name, mode, object_id))
    selected.sort()
    portable_names([entry[0] for entry in selected])
    require(policy.ROOT_FILES.issubset({entry[0] for entry in selected}), "Committed ref lacks required source root files")
    for name, mode, object_id in selected:
        data = git(repo, "cat-file", "blob", object_id)
        contents[name] = data
        files.append({"path": name, "sha256": digest(data), "size": len(data),
                      "executable": mode == "100755", "mode": "0755" if mode == "100755" else "0644",
                      "git_blob": object_id})
    # A reproducible set includes the exact selector/exporter used, rather than
    # silently applying an uncommitted or unrelated policy to an older ref.
    for name, local in ((POLICY_PATH, SCRIPTS / "release-package.py"), (EXPORTER_PATH, Path(__file__))):
        require(contents.get(name) == local.read_bytes(),
                "Exporter and selection policy must be committed at --ref; run their versions from that checkout: " + name)
    return files, contents


def member_specs(root, files):
    directories = {""}
    for record in files:
        for parent in PurePosixPath(record["path"]).parents:
            if str(parent) != ".":
                directories.add(parent.as_posix())
    entries = [(name, True, None) for name in directories]
    entries += [(record["path"], False, record) for record in files]
    return [(root + ("/" + name if name else ""), is_directory, record)
            for name, is_directory, record in sorted(entries, key=lambda entry: entry[0])]


def write_archive(path, root, files, contents):
    with path.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as compressed:
        with tarfile.open(mode="w|", fileobj=compressed, format=tarfile.PAX_FORMAT) as archive:
            for name, is_directory, record in member_specs(root, files):
                member = tarfile.TarInfo(name)
                member.uid = member.gid = member.mtime = 0
                member.uname = member.gname = ""
                member.mode = 0o755 if is_directory else int(record["mode"], 8)
                member.type = tarfile.DIRTYPE if is_directory else tarfile.REGTYPE
                data = b"" if is_directory else contents[record["path"]]
                member.size = len(data)
                archive.addfile(member, None if is_directory else io.BytesIO(data))


def verify(directory):
    directory = Path(directory)
    require(directory.is_dir() and not directory.is_symlink(), "Source set must be a real directory")
    manifest_path = directory / MANIFEST
    require(manifest_path.is_file() and not manifest_path.is_symlink(), "Source manifest missing or symlinked")
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    require(isinstance(manifest, dict) and manifest.get("kind") == "corresponding-project-source", "Unsupported source manifest")
    prefix = format_identity(manifest.get("schema_version"), manifest.get("product"))
    require(canonical_json(manifest) == manifest_bytes, "Manifest is not canonically serialized")
    commit = manifest["source"]["commit"]
    require(isinstance(commit, str) and HEX.fullmatch(commit), "Invalid source commit")
    tree = manifest["source"]["tree"]
    require(isinstance(tree, str) and HEX.fullmatch(tree) and len(tree) == len(commit), "Invalid source tree")
    root = prefix + commit
    filename = root + ".tar.gz"
    require(manifest["archive"]["filename"] == filename and manifest["archive"]["root"] == root, "Archive identity disagrees")
    require(manifest["archive"]["format"] == "POSIX pax tar + gzip" and manifest["archive"]["mtime"] == 0
            and manifest["archive"]["gzip_level"] == 9, "Archive format parameters disagree")
    require({path.name for path in directory.iterdir()} == {filename, MANIFEST, CHECKSUMS}, "Unexpected source-set files")
    require(all(path.is_file() and not path.is_symlink() for path in directory.iterdir()), "Nonregular source-set artifact")
    archive_bytes = (directory / filename).read_bytes()
    require(len(archive_bytes) == manifest["archive"]["size"] and digest(archive_bytes) == manifest["archive"]["sha256"], "Archive hash/size mismatch")
    require(archive_bytes[:10] == b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff", "Gzip metadata is not canonical")
    expected_sums = digest(archive_bytes) + "  " + filename + "\n" + digest(manifest_bytes) + "  " + MANIFEST + "\n"
    require((directory / CHECKSUMS).read_text() == expected_sums, "Source-set checksum mismatch")
    files = manifest["files"]
    require(isinstance(files, list) and files, "Empty source inventory")
    names = [record["path"] for record in files]
    require(names == sorted(set(names)), "Source inventory is unordered or duplicated")
    portable_names(names)
    require(all(allowed_source_for_schema(name, manifest["schema_version"]) for name in names), "Forbidden source inventory path")
    require(policy.ROOT_FILES.issubset(names), "Source inventory lacks root inputs")
    require(inventory_digest(files) == manifest["source"]["inventory_sha256"], "Source inventory digest mismatch")
    for record in files:
        require(record["mode"] in {"0644", "0755"} and type(record["executable"]) is bool
                and record["executable"] == (record["mode"] == "0755")
                and type(record["size"]) is int and record["size"] >= 0
                and re.fullmatch(r"[0-9a-f]{64}", record["sha256"])
                and HEX.fullmatch(record["git_blob"]) and len(record["git_blob"]) == len(commit), "Invalid source file record")
    expected = member_specs(root, files)
    # Validate the complete decompressed stream. A streaming tar reader buffers
    # ahead and can otherwise hide bytes after its end marker from gzip.read().
    tar_bytes = gzip.decompress(archive_bytes)
    with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
        observed, final_end = 0, 0
        for member in archive:
            require(observed < len(expected), "Extra archive member")
            name, is_directory, record = expected[observed]
            observed += 1
            require(member.name == name and member.type == (tarfile.DIRTYPE if is_directory else tarfile.REGTYPE), "Unexpected archive path/type")
            require(member.uid == member.gid == member.mtime == 0 and member.uname == member.gname == ""
                    and member.mode == (0o755 if is_directory else int(record["mode"], 8))
                    and not (set(member.pax_headers) - {"path"}), "Noncanonical archive metadata")
            final_end = member.offset_data + ((member.size + 511) // 512) * 512
            require(not tar_bytes[member.offset_data + member.size:final_end].strip(b"\0"),
                    "Nonzero archive member padding")
            if is_directory:
                require(member.size == 0, "Directory carries data")
            else:
                require(member.size == record["size"], "Source member size mismatch")
                data = archive.extractfile(member).read()
                require(digest(data) == record["sha256"], "Source member hash mismatch")
                blob = b"blob " + str(len(data)).encode("ascii") + b"\0" + data
                require(hashlib.new("sha1" if len(commit) == 40 else "sha256", blob).hexdigest() == record["git_blob"],
                        "Source Git blob mismatch")
        require(observed == len(expected), "Missing archive member")
    tail = tar_bytes[final_end:]
    require(len(tail) >= 1024 and len(tar_bytes) % tarfile.RECORDSIZE == 0 and not tail.strip(b"\0"),
            "Unexpected trailing archive data or missing canonical tar end padding")
    by_name = {record["path"]: record for record in files}
    for label, name in (("selection_policy", POLICY_PATH), ("exporter", EXPORTER_PATH)):
        require(manifest[label]["path"] == name and name in by_name
                and manifest[label]["sha256"] == by_name[name]["sha256"], "Producer identity missing from corresponding source")
    return manifest


def create(repo, reference, destination):
    repo, destination = Path(repo).resolve(), Path(destination).absolute()
    require(not destination.exists() and not destination.is_symlink(), "Output already exists; existing outputs are preserved")
    commit = resolve_commit(repo, reference)
    tree = git(repo, "rev-parse", "--verify", commit + "^{tree}").decode().strip()
    files, contents = committed_files(repo, commit)
    root = format_identity(CURRENT_SCHEMA, CURRENT_PRODUCT) + commit
    filename = root + ".tar.gz"
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Atomically claim an absent output name. Only this empty reservation may be
    # removed on failure; no existing output or unrelated user file is deleted.
    destination.mkdir(mode=0o755)
    try:
        with tempfile.TemporaryDirectory(prefix=".opennumlab-source-", dir=destination.parent) as temporary:
            work = Path(temporary)
            source = work / "source"
            for record in files:
                path = source / record["path"]
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(contents[record["path"]])
                path.chmod(int(record["mode"], 8))
            expected_inventory = [{key: record[key] for key in ("path", "sha256", "size", "executable")} for record in files]
            require(policy.inventory(source) == expected_inventory, "Staged source disagrees with committed blobs")
            bundle = work / "bundle"
            bundle.mkdir()
            write_archive(bundle / filename, root, files, contents)
            archive_bytes = (bundle / filename).read_bytes()
            by_name = {record["path"]: record for record in files}
            manifest = {"schema_version": CURRENT_SCHEMA, "kind": "corresponding-project-source", "product": CURRENT_PRODUCT,
                        "source": {"commit": commit, "tree": tree, "inventory_sha256": inventory_digest(files)},
                        "selection_policy": {"path": POLICY_PATH, "sha256": by_name[POLICY_PATH]["sha256"]},
                        "exporter": {"path": EXPORTER_PATH, "sha256": by_name[EXPORTER_PATH]["sha256"],
                                     "python_version": platform.python_version(), "zlib_version": zlib.ZLIB_RUNTIME_VERSION},
                        "archive": {"filename": filename, "root": root, "sha256": digest(archive_bytes), "size": len(archive_bytes),
                                    "format": "POSIX pax tar + gzip", "mtime": 0, "gzip_level": 9,
                                    "owner": "uid=0 gid=0; empty uname/gname", "modes": "files 0644/0755 from Git; directories 0755"},
                        "files": files,
                        "scope": {"contents": "Allowlisted project source and retained notices; no Git history or raw local evidence",
                                  "external_dependencies": "Cargo registry/git dependencies, compiler/linker/SDK, Octave/toolkit/fonts and optional packages remain external",
                                  "builds": "not_run", "offline_dependency_closure": "pending", "installed_qualification": "not_run",
                                  "packaging": "Committed Git checkout required; archive-only app packaging unsupported",
                                  "publication": "not_performed"}}
            manifest_bytes = canonical_json(manifest)
            (bundle / MANIFEST).write_bytes(manifest_bytes)
            (bundle / CHECKSUMS).write_text(digest(archive_bytes) + "  " + filename + "\n" + digest(manifest_bytes) + "  " + MANIFEST + "\n")
            verify(bundle)
            os.replace(bundle, destination)
        return manifest
    except BaseException:
        try:
            destination.rmdir()  # Never recursive: preserves any completed set.
        except OSError:
            pass
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    export = commands.add_parser("create", help="Read only committed Git blobs and emit a new source set")
    export.add_argument("--ref", required=True, help="Explicit commit or ref, resolved exactly once")
    export.add_argument("--repo", type=Path, default=SCRIPTS.parent)
    export.add_argument("--output", type=Path, required=True, help="New directory; existing outputs are never replaced")
    check = commands.add_parser("verify", help="Verify source-set checksums, exact members and metadata without Git")
    check.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    manifest = create(args.repo, args.ref, args.output) if args.command == "create" else verify(args.directory)
    print(json.dumps({"source_commit": manifest["source"]["commit"], "source_files": len(manifest["files"]),
                      "archive": manifest["archive"]["filename"], "archive_sha256": manifest["archive"]["sha256"],
                      "status": "source_set_verified", "builds": "not_run", "publication": "not_performed"}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError, KeyError, TypeError) as error:
        sys.exit("Source export failed: " + str(error))
