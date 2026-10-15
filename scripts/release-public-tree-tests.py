#!/usr/bin/env python3
"""Focused public-tree regressions using synthetic files and local Git only.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("release_public_tree", SCRIPTS / "release-public-tree.py")
public = importlib.util.module_from_spec(spec)
spec.loader.exec_module(public)
source = public.source


class PublicTreeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="opennumlab-public-tree-tests-")
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.source_set = self.root / "source-set"
        self.output = self.root / "public-set"
        self.environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
        self.git("init", "-q")
        self.git("config", "user.name", "Public Tree Tests")
        self.git("config", "user.email", "public-tree-tests@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        for name in source.policy.ROOT_FILES:
            self.write(name, "Original " + name + "\n")
        for name in ("release-package.py", "release-source.py", "release-public-tree.py"):
            self.write("scripts/" + name, (SCRIPTS / name).read_text())
        self.write("scripts/setup.sh", "#!/bin/sh\nexit 0\n").chmod(0o755)
        self.write("crates/example/src/lib.rs", "// original source\n")
        self.write(".gitignore", "/target/\n/dist/\n")
        self.write(".github/workflows/ci.yml", "name: Original CI\non: [push]\njobs: {}\n")
        self.write(".github/workflows/private.yml", "PRIVATE_TRACKED_CONFIGURATION\n")
        self.write(".env", "PRIVATE_TRACKED_ENVIRONMENT\n")
        self.write("docs/research/private.md", "PRIVATE_RESEARCH\n")
        self.commit = self.commit_all()
        self.source_manifest = source.create(self.repo, self.commit, self.source_set)

    def tearDown(self):
        self.temporary.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args],
                                       env=self.environment, text=True).strip()

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def commit_all(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Synthetic committed input")
        return self.git("rev-parse", "HEAD")

    def create(self, source_set=None, output=None):
        return public.create(source_set or self.source_set, self.repo, output or self.output)

    def output_bytes(self, directory=None):
        directory = directory or self.output
        return {path.relative_to(directory).as_posix(): path.read_bytes()
                for path in directory.rglob("*") if path.is_file()}

    def refresh(self, manifest=None, include_checksums=True):
        manifest = manifest or json.loads((self.output / public.MANIFEST).read_text())
        manifest["inventory_sha256"] = source.inventory_digest(manifest["files"])
        source_files = [{key: record[key] for key in public.SOURCE_FIELDS}
                        for record in manifest["files"] if record["origin"] == "source_archive"]
        manifest["source"]["inventory_sha256"] = source.inventory_digest(source_files)
        (self.output / public.MANIFEST).write_bytes(source.canonical_json(manifest))
        if include_checksums:
            (self.output / public.CHECKSUMS).write_bytes(public.checksums(self.output, manifest["files"]))

    def changed_source_set(self, commit=None, omit=(), schema_version=2):
        """Author internally consistent unsigned input for negative boundary tests."""
        commit = commit or self.commit
        manifest = copy.deepcopy(self.source_manifest)
        committed = public.git_tree(self.repo, commit)
        files, contents = [], {}
        for name in sorted(committed):
            if not source.allowed_source_for_schema(name, schema_version) or name in omit:
                continue
            mode, kind, object_id = committed[name]
            self.assertEqual(kind, "blob")
            data = source.git(self.repo, "cat-file", "blob", object_id)
            contents[name] = data
            files.append({"path": name, "sha256": source.digest(data), "size": len(data),
                          "mode": "0755" if mode == "100755" else "0644", "executable": mode == "100755",
                          "git_blob": object_id})
        output = self.root / ("changed-source-" + str(len(list(self.root.glob("changed-source-*")))))
        output.mkdir()
        product, prefix = source.FORMAT_IDENTITIES[schema_version]
        manifest.update(schema_version=schema_version, product=product)
        root = prefix + commit
        filename = root + ".tar.gz"
        source.write_archive(output / filename, root, files, contents)
        archive = (output / filename).read_bytes()
        manifest["source"] = {"commit": commit, "tree": self.git("rev-parse", commit + "^{tree}"),
                              "inventory_sha256": source.inventory_digest(files)}
        manifest["files"] = files
        manifest["archive"].update(filename=filename, root=root, size=len(archive), sha256=source.digest(archive))
        by_name = {record["path"]: record for record in files}
        manifest["selection_policy"]["sha256"] = by_name[source.POLICY_PATH]["sha256"]
        manifest["exporter"]["sha256"] = by_name[source.EXPORTER_PATH]["sha256"]
        data = source.canonical_json(manifest)
        (output / source.MANIFEST).write_bytes(data)
        (output / source.CHECKSUMS).write_text(source.digest(archive) + "  " + filename + "\n"
                                              + source.digest(data) + "  " + source.MANIFEST + "\n")
        source.verify(output)
        return output

    def test_new_public_creation_uses_schema_two_and_requires_new_source_format(self):
        manifest = self.create()
        self.assertEqual((manifest["schema_version"], manifest["product"]), (2, "OpenNumLab"))
        self.assertEqual(manifest["source"]["archive"]["root"], "opennumlab-source-" + self.commit)
        legacy_source = self.changed_source_set(schema_version=1)
        rejected_output = self.root / "legacy-creation-is-disabled"
        with self.assertRaisesRegex(ValueError, "legacy format 1 is verification only"):
            self.create(source_set=legacy_source, output=rejected_output)
        self.assertFalse(rejected_output.exists())

    def test_legacy_public_schema_one_and_legacy_logo_remain_verifiable(self):
        manifest = self.create()
        manifest.update(schema_version=1, product="NumJinn")
        archive = manifest["source"]["archive"]
        archive["root"] = "numjinn-source-" + self.commit
        archive["filename"] = archive["root"] + ".tar.gz"
        name, data = "assets/numjinn-logo.png", b"retained legacy image fixture"
        path = self.output / public.TREE / name
        path.parent.mkdir(mode=0o755)
        path.write_bytes(data)
        path.chmod(0o644)
        manifest["files"].append({"path": name, "sha256": source.digest(data), "size": len(data),
                                  "mode": "0644", "executable": False,
                                  "git_blob": public.blob_id(data, self.commit), "origin": "source_archive"})
        manifest["files"].sort(key=lambda record: record["path"])
        (self.output / public.FILE_TREE).write_bytes(public.file_tree(manifest["files"]))
        self.refresh(manifest)
        with patch.object(source, "git", side_effect=AssertionError("Legacy verify must not use Git")):
            self.assertEqual(public.verify(self.output), manifest)
        manifest.update(schema_version=2, product="OpenNumLab")
        archive["root"] = "opennumlab-source-" + self.commit
        archive["filename"] = archive["root"] + ".tar.gz"
        self.refresh(manifest)
        with self.assertRaisesRegex(ValueError, "Forbidden public-tree path"):
            public.verify(self.output)

    def test_mixed_public_schema_product_and_source_prefix_are_rejected(self):
        baseline = self.create()
        for fields, message in (({"schema_version": 1}, "schema/product"),
                                ({"product": "NumJinn"}, "schema/product"),
                                ({"schema_version": 1, "product": "NumJinn"}, "source archive identity"),
                                ({"schema_version": True}, "schema")):
            with self.subTest(fields=fields):
                manifest = copy.deepcopy(baseline)
                manifest.update(fields)
                self.refresh(manifest)
                with self.assertRaisesRegex(ValueError, message):
                    public.verify(self.output)

    def test_legacy_public_rejects_current_logo_alone_or_with_legacy_logo(self):
        for legacy in (False, True):
            with self.subTest(also_legacy_logo=legacy):
                self.output = self.root / ("legacy-current-logo-" + str(legacy))
                manifest = self.create()
                manifest.update(schema_version=1, product="NumJinn")
                archive = manifest["source"]["archive"]
                archive["root"] = "numjinn-source-" + self.commit
                archive["filename"] = archive["root"] + ".tar.gz"
                assets = ["assets/opennumlab-logo.png"]
                if legacy:
                    assets.append("assets/numjinn-logo.png")
                for name in assets:
                    data = ("brand fixture " + name).encode()
                    path = self.output / public.TREE / name
                    path.parent.mkdir(mode=0o755, exist_ok=True)
                    path.write_bytes(data)
                    path.chmod(0o644)
                    manifest["files"].append({"path": name, "sha256": source.digest(data), "size": len(data),
                                              "mode": "0644", "executable": False,
                                              "git_blob": public.blob_id(data, self.commit), "origin": "source_archive"})
                manifest["files"].sort(key=lambda record: record["path"])
                (self.output / public.FILE_TREE).write_bytes(public.file_tree(manifest["files"]))
                self.refresh(manifest)
                with self.assertRaisesRegex(ValueError, "Forbidden public-tree path"):
                    public.verify(self.output)

    def test_exact_flat_inventory_configs_modes_and_no_git_initialization(self):
        original = source.git
        calls = []
        def observed(repo, *args):
            calls.append(args)
            self.assertNotIn(args[0], {"init", "checkout", "clone", "fetch"})
            return original(repo, *args)
        with patch.object(source, "git", side_effect=observed):
            manifest = self.create()
        self.assertEqual({path.name for path in self.output.iterdir()},
                         {public.TREE, public.FILE_TREE, public.MANIFEST, public.CHECKSUMS})
        self.assertEqual([args[0] for args in calls].count("cat-file"), 2)
        names = {record["path"] for record in manifest["files"]}
        self.assertEqual({name for name in names if any(part.startswith(".") for part in Path(name).parts)},
                         set(public.CONFIGS))
        self.assertFalse(any(".git" in path.parts for path in self.output.rglob("*")))
        self.assertEqual((self.output / "tree/scripts/setup.sh").stat().st_mode & 0o777, 0o755)
        self.assertEqual((self.output / "tree/README.md").stat().st_mode & 0o777, 0o644)
        self.assertEqual(public.verify(self.output), manifest)
        serialized = (self.output / public.MANIFEST).read_text()
        self.assertNotIn(str(self.repo), serialized)
        self.assertNotIn(str(self.root), serialized)
        self.assertEqual(manifest["scope"]["binary_correspondence"], "not_claimed")

    def test_value_provisioning_inputs_are_inherited_from_source_with_exact_modes_and_exclusions(self):
        selected = {
            "scripts/build-value-bridge.sh": ("#!/bin/sh\n# owned value builder fixture\n", 0o755),
            "scripts/verify-value-provisioning-test.sh": ("#!/bin/sh\n# owned provisioning contract fixture\n", 0o755),
            "scripts/release-launcher-value-test.sh": ("#!/bin/sh\n# owned launcher value contract fixture\n", 0o755),
            "docs/VALUE_EDIT_PROTOCOL.md": ("# Owned value edit protocol fixture\n", 0o644),
        }
        denied = ("scripts/build-value-bridge-local.sh", "scripts/verify-value-provisioning-test-private.sh",
                  "scripts/private/build-value-bridge.sh", "docs/VALUE_EDIT_PROTOCOL_PRIVATE.md",
                  "docs/private/VALUE_EDIT_PROTOCOL.md", "native/mathlab_value_bridge.oct")
        for name, (text, mode) in selected.items():
            self.write(name, text).chmod(mode)
        for name in denied:
            self.write(name, "VALUE_PROVISIONING_PRIVATE_SENTINEL\n").chmod(0o755)
        selected_source_set = self.root / "value-provisioning-source"
        source_manifest = source.create(self.repo, self.commit_all(), selected_source_set)
        manifest = self.create(source_set=selected_source_set)
        source_records = {record["path"]: record for record in source_manifest["files"]}
        records = {record["path"]: record for record in manifest["files"]}
        for name, (text, mode) in selected.items():
            with self.subTest(path=name):
                path = self.output / public.TREE / name
                self.assertEqual(path.read_bytes(), text.encode())
                self.assertEqual(path.stat().st_mode & 0o777, mode)
                self.assertEqual(records[name]["origin"], "source_archive")
                self.assertEqual({key: records[name][key] for key in public.SOURCE_FIELDS}, source_records[name])
                self.assertEqual(records[name]["mode"], format(mode, "04o"))
                self.assertEqual(records[name]["executable"], mode == 0o755)
        self.assertFalse(set(records).intersection(denied))
        self.assertEqual(set(records), set(source_records) | set(public.CONFIGS))
        self.assertFalse(any(b"VALUE_PROVISIONING_PRIVATE_SENTINEL" in data for data in self.output_bytes().values()))
        self.assertEqual(public.verify(self.output), manifest)

    def test_fixed_configs_despite_dirty_extra_replacement_and_moving_head(self):
        self.write(".gitignore", "later committed ignore\n")
        self.write(".github/workflows/ci.yml", "name: Later committed CI\n")
        replacement = self.commit_all()
        self.git("replace", self.commit, replacement)
        self.write(".gitignore", "DIRTY_WORKTREE\n")
        self.write(".github/workflows/untracked.yml", "UNTRACKED_PRIVATE\n")
        self.write("README.md", "DIRTY_README\n")
        with patch.dict(os.environ, {"GIT_DIR": str(self.root / "wrong-git"),
                                     "GIT_WORK_TREE": str(self.root / "wrong-worktree")}):
            manifest = self.create()
        self.assertEqual(manifest["source"]["commit"], self.commit)
        self.assertEqual((self.output / "tree/.gitignore").read_text(), "/target/\n/dist/\n")
        self.assertEqual((self.output / "tree/.github/workflows/ci.yml").read_text(),
                         "name: Original CI\non: [push]\njobs: {}\n")
        self.assertNotIn(b"DIRTY_", b"".join(self.output_bytes().values()))
        self.assertFalse((self.output / "tree/.github/workflows/untracked.yml").exists())

    def test_deterministic_across_destinations_and_mtimes(self):
        first = self.create()
        expected = self.output_bytes()
        for path in self.repo.rglob("*"):
            if path.is_file():
                os.utime(path, (1234567890, 1234567890))
        other = self.root / "different-parent/other-public-set"
        self.assertEqual(first, self.create(output=other))
        self.assertEqual(expected, self.output_bytes(other))

    def test_standalone_verification_needs_no_source_set_repo_or_git(self):
        expected = self.create()
        shutil.rmtree(self.repo)
        shutil.rmtree(self.source_set)
        with patch.object(source, "git", side_effect=AssertionError("Git must not run")), \
                patch.object(subprocess, "Popen", side_effect=AssertionError("No tool may run")):
            self.assertEqual(public.verify(self.output), expected)
        environment = dict(os.environ, PATH=str(self.root / "no-tools"), PYTHONDONTWRITEBYTECODE="1")
        result = subprocess.run([sys.executable, "-B", str(SCRIPTS / "release-public-tree.py"),
                                 "verify", "--directory", str(self.output)], env=environment,
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "public_tree_verified")

    def test_config_symlinks_and_gitlinks_are_rejected(self):
        config = self.repo / ".gitignore"
        config.unlink()
        config.symlink_to("README.md")
        symlink_commit = self.commit_all()
        symlink_source = self.root / "symlink-source"
        source.create(self.repo, symlink_commit, symlink_source)
        with self.assertRaisesRegex(ValueError, "regular Git file"):
            self.create(source_set=symlink_source)
        config.unlink()
        self.git("update-index", "--add", "--cacheinfo", "160000," + self.commit + ",.gitignore")
        self.git("commit", "-qm", "Synthetic configuration gitlink")
        gitlink_source = self.root / "gitlink-source"
        source.create(self.repo, self.git("rev-parse", "HEAD"), gitlink_source)
        with self.assertRaisesRegex(ValueError, "regular Git file"):
            self.create(source_set=gitlink_source)
        self.assertFalse(self.output.exists())

    def test_wrong_source_tree_and_omitted_allowed_file_are_rejected(self):
        manifest = json.loads((self.source_set / source.MANIFEST).read_text())
        manifest["source"]["tree"] = "0" * 40
        data = source.canonical_json(manifest)
        (self.source_set / source.MANIFEST).write_bytes(data)
        (self.source_set / source.CHECKSUMS).write_text(manifest["archive"]["sha256"] + "  "
            + manifest["archive"]["filename"] + "\n" + source.digest(data) + "  " + source.MANIFEST + "\n")
        with self.assertRaisesRegex(ValueError, "tree differs"):
            self.create()
        omitted = self.changed_source_set(omit={"crates/example/src/lib.rs"})
        with self.assertRaisesRegex(ValueError, "selection differs"):
            self.create(source_set=omitted)
        self.assertFalse(self.output.exists())

    def test_source_and_policy_helper_versions_must_match_executing_helpers(self):
        for name in (source.EXPORTER_PATH, source.POLICY_PATH):
            with self.subTest(helper=name):
                for helper, data in public.HELPER_BYTES.items():
                    (self.repo / helper).write_bytes(data)
                path = self.repo / name
                path.write_bytes(path.read_bytes() + b"\n# Other committed helper version\n")
                changed = self.changed_source_set(commit=self.commit_all())
                with self.assertRaisesRegex(ValueError, "included helper: " + name):
                    self.create(source_set=changed)
        self.assertFalse(self.output.exists())

    def test_public_producer_must_match_committed_archive(self):
        path = self.repo / public.PRODUCER
        path.write_bytes(path.read_bytes() + b"\n# Other committed producer\n")
        changed_commit = self.commit_all()
        changed = self.root / "other-producer-source"
        source.create(self.repo, changed_commit, changed)
        with self.assertRaisesRegex(ValueError, "producer must be committed"):
            self.create(source_set=changed)
        self.assertFalse(self.output.exists())

    def test_bytes_modes_git_blobs_and_origin_tampering_are_rejected(self):
        self.create()
        readme = self.output / "tree/README.md"
        original = readme.read_bytes()
        readme.write_bytes(b"tampered\n")
        with self.assertRaisesRegex(ValueError, "bytes/mode mismatch"):
            public.verify(self.output)
        readme.write_bytes(original)
        readme.chmod(0o755)
        with self.assertRaisesRegex(ValueError, "bytes/mode mismatch"):
            public.verify(self.output)
        readme.chmod(0o644)
        manifest = json.loads((self.output / public.MANIFEST).read_text())
        next(record for record in manifest["files"] if record["path"] == "README.md")["git_blob"] = "0" * 40
        self.refresh(manifest)
        with self.assertRaisesRegex(ValueError, "Git blob mismatch"):
            public.verify(self.output)
        manifest = self.create(output=self.root / "origin-source")
        config = next(record for record in manifest["files"] if record["path"] == ".gitignore")
        config["origin"] = "source_archive"
        # Restore the initial complete output before isolating origin rejection.
        shutil.rmtree(self.output)
        shutil.copytree(self.root / "origin-source", self.output)
        self.refresh(manifest)
        with self.assertRaisesRegex(ValueError, "origin mismatch"):
            public.verify(self.output)

    def test_hidden_extra_empty_directory_symlink_and_special_file_rejection(self):
        self.create()
        extra = self.output / "tree/.private"
        extra.write_text("private")
        with self.assertRaisesRegex(ValueError, "Unexpected public-tree file"):
            public.verify(self.output)
        extra.unlink()
        extra.mkdir()
        with self.assertRaisesRegex(ValueError, "inventory mismatch"):
            public.verify(self.output)
        extra.rmdir()
        extra.symlink_to(self.output / "tree/README.md")
        with self.assertRaisesRegex(ValueError, "Symlink"):
            public.verify(self.output)
        extra.unlink()
        os.mkfifo(extra)
        with self.assertRaisesRegex(ValueError, "Unexpected public-tree file/type"):
            public.verify(self.output)
        extra.unlink()
        (self.output / ".hidden-extra").write_text("private")
        with self.assertRaisesRegex(ValueError, "output names"):
            public.verify(self.output)

    def test_canonical_metadata_and_producer_identity_are_required(self):
        self.create()
        listing = self.output / public.FILE_TREE
        listing.write_bytes(listing.read_bytes() + b"tree/nonexistent\n")
        self.refresh()
        with self.assertRaisesRegex(ValueError, "FILE_TREE"):
            public.verify(self.output)
        manifest = json.loads((self.output / public.MANIFEST).read_text())
        listing.write_bytes(public.file_tree(manifest["files"]))
        manifest["producer"]["sha256"] = "0" * 64
        self.refresh(manifest)
        with self.assertRaisesRegex(ValueError, "producer identity"):
            public.verify(self.output)
        manifest["producer"]["sha256"] = next(r["sha256"] for r in manifest["files"] if r["path"] == public.PRODUCER)
        self.refresh(manifest)
        (self.output / public.MANIFEST).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "not canonical"):
            public.verify(self.output)

    def test_file_names_must_be_unique_sorted_portable_and_narrowly_allowed(self):
        baseline = self.create()
        for mutation in ("duplicate", "unordered", "casefold", "unapproved_dotfile"):
            with self.subTest(mutation=mutation):
                manifest = copy.deepcopy(baseline)
                if mutation == "duplicate":
                    manifest["files"].append(copy.deepcopy(manifest["files"][0]))
                elif mutation == "unordered":
                    manifest["files"].reverse()
                else:
                    names = ("crates/Example/a.rs", "crates/example/b.rs") if mutation == "casefold" else (".gitattributes",)
                    for name in names:
                        record = copy.deepcopy(next(r for r in manifest["files"] if r["path"] == "README.md"))
                        record["path"] = name
                        manifest["files"].append(record)
                    manifest["files"].sort(key=lambda record: record["path"])
                self.refresh(manifest, include_checksums=False)
                with self.assertRaisesRegex(ValueError, "unordered or duplicated|extraction collision|Forbidden public-tree path"):
                    public.verify(self.output)

    def test_existing_output_and_failed_writes_preserve_user_content(self):
        self.output.mkdir()
        marker = self.output / "keep"
        marker.write_text("existing output")
        with self.assertRaisesRegex(ValueError, "Output already exists"):
            self.create()
        other = self.root / "failed-output"
        original = Path.write_bytes
        def fail_metadata(path, data):
            if path.name == public.MANIFEST:
                raise OSError("injected write failure")
            return original(path, data)
        with patch.object(Path, "write_bytes", fail_metadata):
            with self.assertRaisesRegex(OSError, "write failure"):
                self.create(output=other)
        self.assertFalse(other.exists())
        self.assertEqual(marker.read_text(), "existing output")
        self.assertEqual(list(self.root.glob(".opennumlab-public-tree-*")), [])
        with patch.object(public.os, "replace", side_effect=OSError("empty reservation failure")):
            with self.assertRaisesRegex(OSError, "reservation failure"):
                self.create(output=other)
        self.assertFalse(other.exists())
        def contested_reservation(staged, destination):
            (destination / "user-file").write_text("preserve racing user content")
            raise OSError("injected publication failure")
        with patch.object(public.os, "replace", side_effect=contested_reservation):
            with self.assertRaisesRegex(OSError, "publication failure"):
                self.create(output=other)
        self.assertEqual((other / "user-file").read_text(), "preserve racing user content")
        self.assertEqual(marker.read_text(), "existing output")


if __name__ == "__main__":
    unittest.main(verbosity=2)
