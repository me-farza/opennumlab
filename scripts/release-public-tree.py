#!/usr/bin/env python3
"""Export or verify a flat public project tree without Git history or builds.

Creation consumes a verified source set and two configuration blobs from its
fixed commit. Verification needs only this tool and its shared source policy.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("public_tree_source", SCRIPTS / "release-source.py")
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)
policy = source.policy
HELPER_BYTES = {source.EXPORTER_PATH: (SCRIPTS / "release-source.py").read_bytes(),
                source.POLICY_PATH: (SCRIPTS / "release-package.py").read_bytes()}

PRODUCER = "scripts/release-public-tree.py"
CONFIGS = (".github/workflows/ci.yml", ".gitignore")
TREE = "tree"
FILE_TREE = "FILE_TREE.txt"
MANIFEST = "public-tree-manifest.json"
CHECKSUMS = "checksums.sha256"
RECORD_FIELDS = {"path", "sha256", "size", "mode", "executable", "git_blob", "origin"}
SOURCE_FIELDS = RECORD_FIELDS - {"origin"}
SCOPE = {"contents": "Allowlisted source archive and exactly two committed configuration files",
         "git_history": "not_included", "builds": "not_run", "binary_correspondence": "not_claimed",
         "installed_qualification": "not_run", "publication": "not_performed",
         "authenticity": "Unsigned consistency records; no authentication claim"}


def require(condition, message):
    source.require(condition, message)


def hex256(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def blob_id(data, commit):
    header = b"blob " + str(len(data)).encode("ascii") + b"\0"
    return hashlib.new("sha1" if len(commit) == 40 else "sha256", header + data).hexdigest()


def directories(files):
    result = {""}
    for record in files:
        result.update(parent.as_posix() for parent in PurePosixPath(record["path"]).parents
                      if parent.as_posix() != ".")
    return result


def file_tree(files):
    names = [TREE + "/" + (name + "/" if name else "") for name in directories(files)]
    names += [TREE + "/" + record["path"] for record in files]
    return ("\n".join(sorted(names)) + "\n").encode("utf-8")


def checksums(directory, files):
    names = [FILE_TREE, MANIFEST] + [TREE + "/" + record["path"] for record in files]
    return "".join(source.digest((directory / name).read_bytes()) + "  " + name + "\n"
                   for name in sorted(names)).encode("utf-8")


def git_tree(repo, commit):
    result = {}
    for entry in source.git(repo, "ls-tree", "-rz", "--full-tree", commit).split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        name = name.decode("utf-8")
        require(name not in result, "Duplicate committed tree entry")
        result[name] = (mode, kind, object_id)
    return result


def inputs(source_set, repo):
    source_set = Path(source_set)
    manifest = source.verify(source_set)
    require(manifest["schema_version"] == source.CURRENT_SCHEMA,
            "New public-tree creation requires source format 2; legacy format 1 is verification only")
    manifest_bytes = (source_set / source.MANIFEST).read_bytes()
    require(manifest_bytes == source.canonical_json(manifest), "Source manifest changed after verification")
    archive_bytes = (source_set / manifest["archive"]["filename"]).read_bytes()
    require(source.digest(archive_bytes) == manifest["archive"]["sha256"]
            and len(archive_bytes) == manifest["archive"]["size"], "Source archive changed after verification")
    commit, tree = manifest["source"]["commit"], manifest["source"]["tree"]
    require(source.resolve_commit(repo, commit) == commit, "Source commit differs from repository")
    require(source.git(repo, "rev-parse", "--verify", commit + "^{tree}").decode().strip() == tree,
            "Source tree differs from repository commit")
    committed = git_tree(repo, commit)
    selected = {name for name in committed if source.allowed_source_for_schema(name, source.CURRENT_SCHEMA)}
    require(selected == {record["path"] for record in manifest["files"]},
            "Source archive selection differs from the fixed commit")
    contents = {}
    prefix = manifest["archive"]["root"] + "/"
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as archive:
        for member in archive:
            if member.isdir():
                continue
            require(member.isfile() and member.name.startswith(prefix), "Unexpected source archive member")
            name = member.name[len(prefix):]
            require(name in selected and name not in contents, "Unexpected or duplicate source member")
            contents[name] = archive.extractfile(member).read()
    files = []
    for record in manifest["files"]:
        mode, kind, object_id = committed[record["path"]]
        require(kind == "blob" and mode in {"100644", "100755"}
                and record["git_blob"] == object_id
                and record["mode"] == ("0755" if mode == "100755" else "0644"),
                "Source file differs from the fixed Git tree: " + record["path"])
        require(source.digest(contents[record["path"]]) == record["sha256"], "Source member changed")
        files.append({**record, "origin": "source_archive"})
    require(contents.get(PRODUCER) == Path(__file__).read_bytes(),
            "Public-tree producer must be committed in the source archive and equal the executing script")
    for name, data in HELPER_BYTES.items():
        require(contents.get(name) == data,
                "Executing source exporter and selection policy must equal the included helper: " + name)
    for name in CONFIGS:
        require(name in committed, "Missing committed configuration: " + name)
        mode, kind, object_id = committed[name]
        require(kind == "blob" and mode in {"100644", "100755"},
                "Configuration is not a regular Git file: " + name)
        data = source.git(repo, "cat-file", "blob", object_id)
        require(blob_id(data, commit) == object_id, "Configuration Git blob mismatch")
        contents[name] = data
        files.append({"path": name, "sha256": source.digest(data), "size": len(data),
                      "mode": "0755" if mode == "100755" else "0644", "executable": mode == "100755",
                      "git_blob": object_id, "origin": "committed_configuration"})
    files.sort(key=lambda record: record["path"])
    source.portable_names([record["path"] for record in files])
    return manifest, manifest_bytes, files, contents


def verify(directory):
    directory = Path(directory)
    require(directory.is_dir() and not directory.is_symlink(), "Public tree must be a real directory")
    require({path.name for path in directory.iterdir()} == {TREE, FILE_TREE, MANIFEST, CHECKSUMS},
            "Unexpected public-tree output names")
    for name in (FILE_TREE, MANIFEST, CHECKSUMS):
        path = directory / name
        require(path.is_file() and not path.is_symlink() and stat.S_IMODE(path.stat().st_mode) == 0o644,
                "Nonregular or noncanonical public-tree metadata: " + name)
    manifest_bytes = (directory / MANIFEST).read_bytes()
    manifest = json.loads(manifest_bytes)
    require(isinstance(manifest, dict) and set(manifest) == {"schema_version", "kind", "product", "source",
            "producer", "files", "inventory_sha256", "scope"}, "Unexpected public-tree manifest fields")
    require(source.canonical_json(manifest) == manifest_bytes, "Public-tree manifest is not canonical")
    prefix = source.format_identity(manifest["schema_version"], manifest["product"])
    require(manifest["kind"] == "public-project-tree" and manifest["scope"] == SCOPE,
            "Unsupported public-tree manifest or claims")
    identity = manifest["source"]
    require(isinstance(identity, dict) and set(identity) == {"commit", "tree", "inventory_sha256", "archive",
            "manifest", "selection_policy", "exporter"}, "Unexpected source identity fields")
    commit = identity["commit"]
    require(isinstance(commit, str) and source.HEX.fullmatch(commit), "Invalid source commit")
    require(isinstance(identity["tree"], str) and source.HEX.fullmatch(identity["tree"])
            and len(identity["tree"]) == len(commit), "Invalid source tree")
    archive = identity["archive"]
    require(isinstance(archive, dict) and set(archive) == {"filename", "root", "sha256", "size"}
            and archive["root"] == prefix + commit
            and archive["filename"] == archive["root"] + ".tar.gz"
            and hex256(archive["sha256"]) and type(archive["size"]) is int and archive["size"] > 0,
            "Invalid source archive identity")
    require(isinstance(identity["manifest"], dict) and set(identity["manifest"]) == {"filename", "sha256"}
            and identity["manifest"]["filename"] == source.MANIFEST
            and hex256(identity["manifest"]["sha256"]), "Invalid source manifest identity")
    files = manifest["files"]
    require(isinstance(files, list) and files, "Missing public-tree file inventory")
    require(all(isinstance(record, dict) and set(record) == RECORD_FIELDS for record in files),
            "Unexpected public-tree file fields")
    names = [record["path"] for record in files]
    require(all(isinstance(name, str) for name in names) and names == sorted(set(names)),
            "Public-tree file inventory is unordered or duplicated")
    source.portable_names(names)
    source_files = []
    observed_configs = set()
    for record in files:
        name = record["path"]
        require(record["mode"] in {"0644", "0755"} and type(record["executable"]) is bool
                and record["executable"] == (record["mode"] == "0755")
                and type(record["size"]) is int and record["size"] >= 0 and hex256(record["sha256"])
                and isinstance(record["git_blob"], str) and source.HEX.fullmatch(record["git_blob"])
                and len(record["git_blob"]) == len(commit), "Invalid public-tree file identity")
        if name in CONFIGS:
            require(record["origin"] == "committed_configuration", "Configuration origin mismatch")
            observed_configs.add(name)
        else:
            require(record["origin"] == "source_archive"
                    and source.allowed_source_for_schema(name, manifest["schema_version"]),
                    "Forbidden public-tree path or origin: " + name)
            source_files.append({key: record[key] for key in SOURCE_FIELDS})
    require(observed_configs == set(CONFIGS), "Public tree lacks its exact configuration files")
    require(policy.ROOT_FILES.issubset(names), "Public tree lacks required source inputs")
    require(source.inventory_digest(source_files) == identity["inventory_sha256"], "Source inventory identity mismatch")
    require(source.inventory_digest(files) == manifest["inventory_sha256"], "Public-tree inventory identity mismatch")
    by_name = {record["path"]: record for record in files}
    for label, name in (("selection_policy", source.POLICY_PATH), ("exporter", source.EXPORTER_PATH)):
        value = identity[label]
        require(isinstance(value, dict) and set(value) == {"path", "sha256"} and value["path"] == name
                and name in by_name and value["sha256"] == by_name[name]["sha256"], "Source producer identity mismatch")
    require(manifest["producer"] == {"path": PRODUCER, "sha256": by_name.get(PRODUCER, {}).get("sha256")}
            and PRODUCER in by_name, "Public-tree producer identity mismatch")
    tree = directory / TREE
    require(tree.is_dir() and not tree.is_symlink(), "Missing or symlinked public root")
    actual_files, actual_directories = set(), {""}
    for path in [tree, *tree.rglob("*")]:
        mode = path.lstat().st_mode
        require(not stat.S_ISLNK(mode), "Symlink in public tree")
        name = path.relative_to(tree).as_posix()
        if stat.S_ISDIR(mode):
            require(stat.S_IMODE(mode) == 0o755, "Noncanonical public directory mode")
            if name != ".":
                actual_directories.add(name)
        else:
            require(stat.S_ISREG(mode) and name in by_name, "Unexpected public-tree file/type: " + name)
            record = by_name[name]
            data = path.read_bytes()
            require(len(data) == record["size"] and source.digest(data) == record["sha256"]
                    and stat.S_IMODE(mode) == int(record["mode"], 8), "Public-tree file bytes/mode mismatch: " + name)
            require(blob_id(data, commit) == record["git_blob"], "Public-tree Git blob mismatch: " + name)
            actual_files.add(name)
    require(actual_files == set(names) and actual_directories == directories(files), "Public-tree inventory mismatch")
    policy.require_project_license_files(actual_files, (tree / "LICENSE").read_bytes())
    require((directory / FILE_TREE).read_bytes() == file_tree(files), "FILE_TREE is not canonical or complete")
    require((directory / CHECKSUMS).read_bytes() == checksums(directory, files), "Public-tree checksums disagree")
    return manifest


def create(source_set, repo, destination):
    destination = Path(destination).absolute()
    require(not destination.exists() and not destination.is_symlink(), "Output already exists; existing outputs are preserved")
    manifest, manifest_bytes, files, contents = inputs(source_set, repo)
    destination.parent.mkdir(parents=True, exist_ok=True)
    reservation = None
    try:
        with tempfile.TemporaryDirectory(prefix=".opennumlab-public-tree-", dir=destination.parent) as temporary:
            bundle = Path(temporary) / "bundle"
            tree = bundle / TREE
            for name in sorted(directories(files)):
                path = tree / name
                path.mkdir(parents=True, exist_ok=True)
                path.chmod(0o755)
            for record in files:
                path = tree / record["path"]
                path.write_bytes(contents[record["path"]])
                path.chmod(int(record["mode"], 8))
            by_name = {record["path"]: record for record in files}
            result = {"schema_version": source.CURRENT_SCHEMA, "kind": "public-project-tree", "product": source.CURRENT_PRODUCT,
                      "source": {"commit": manifest["source"]["commit"], "tree": manifest["source"]["tree"],
                                 "inventory_sha256": manifest["source"]["inventory_sha256"],
                                 "archive": {key: manifest["archive"][key] for key in ("filename", "root", "sha256", "size")},
                                 "manifest": {"filename": source.MANIFEST, "sha256": source.digest(manifest_bytes)},
                                 "selection_policy": manifest["selection_policy"],
                                 "exporter": {key: manifest["exporter"][key] for key in ("path", "sha256")}},
                      "producer": {"path": PRODUCER, "sha256": by_name[PRODUCER]["sha256"]},
                      "files": files, "inventory_sha256": source.inventory_digest(files), "scope": SCOPE}
            (bundle / FILE_TREE).write_bytes(file_tree(files))
            (bundle / MANIFEST).write_bytes(source.canonical_json(result))
            (bundle / CHECKSUMS).write_bytes(checksums(bundle, files))
            for name in (FILE_TREE, MANIFEST, CHECKSUMS):
                (bundle / name).chmod(0o644)
            bundle.chmod(0o755)
            verify(bundle)
            destination.mkdir(mode=0o755)
            reservation = destination.stat()
            os.replace(bundle, destination)
        return result
    except BaseException:
        if reservation is not None:
            try:
                current = destination.lstat()
                if stat.S_ISDIR(current.st_mode) and (current.st_dev, current.st_ino) == (reservation.st_dev, reservation.st_ino):
                    destination.rmdir()  # Empty reservation only; never delete user contents.
            except OSError:
                pass
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    export = commands.add_parser("create", help="Export verified source and exactly two fixed-commit configurations")
    export.add_argument("--source-set", required=True, type=Path)
    export.add_argument("--repo", required=True, type=Path)
    export.add_argument("--output", required=True, type=Path)
    check = commands.add_parser("verify", help="Verify standalone public-tree bytes and metadata without Git")
    check.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    manifest = create(args.source_set, args.repo, args.output) if args.command == "create" else verify(args.directory)
    print(json.dumps({"source_commit": manifest["source"]["commit"], "files": len(manifest["files"]),
                      "status": "public_tree_verified", "builds": "not_run", "binary_correspondence": "not_claimed",
                      "publication": "not_performed"}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError, KeyError, TypeError) as error:
        sys.exit("Public-tree export failed: " + str(error))
