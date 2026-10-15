#!/usr/bin/env python3
"""Synthetic source-archive contracts. Runs Git/Python only; never builds apps.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import gzip
import hashlib
import importlib.util
import io
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
spec = importlib.util.spec_from_file_location("release_source", SCRIPTS / "release-source.py")
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)


class SourceArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="opennumlab-source-tests-")
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.output = self.root / "source-set"
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
        self.git("init", "-q")
        self.git("config", "user.name", "Source Tests")
        self.git("config", "user.email", "source-tests@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        for name in source.policy.ROOT_FILES:
            self.write(name, "original " + name + "\n")
        for name in ("release-source.py", "release-package.py"):
            self.write("scripts/" + name, (SCRIPTS / name).read_text())
        self.write("scripts/setup.sh", "#!/bin/sh\nexit 0\n").chmod(0o755)
        self.write("crates/test/src/conformance.rs", "mod reference;\n")
        self.write("crates/test/src/conformance/reference.rs", "// raw $Format:%H$\n")
        self.write("native/bridge.cc", "// original adapter source\n")
        self.git("add", ".")
        self.git("commit", "-qm", "Original fixture")
        self.commit = self.git("rev-parse", "HEAD").strip()

    def tearDown(self):
        self.temporary.cleanup()

    def git(self, *args, data=None):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], env=self.env,
                                       input=data, text=True).strip()

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def commit_all(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture addition")
        return self.git("rev-parse", "HEAD")

    def create(self, reference=None, output=None):
        return source.create(self.repo, reference or self.commit, output or self.output)

    def archive_contents(self, output=None):
        output = output or self.output
        manifest = json.loads((output / source.MANIFEST).read_text())
        with tarfile.open(output / manifest["archive"]["filename"], "r:gz") as archive:
            return manifest, {member.name: (member, archive.extractfile(member).read() if member.isfile() else b"")
                              for member in archive.getmembers()}

    def refresh_manifest(self, manifest):
        path = self.output / manifest["archive"]["filename"]
        manifest["archive"].update(size=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        data = source.canonical_json(manifest)
        (self.output / source.MANIFEST).write_bytes(data)
        (self.output / source.CHECKSUMS).write_text(manifest["archive"]["sha256"] + "  " + path.name + "\n"
                                                 + hashlib.sha256(data).hexdigest() + "  " + source.MANIFEST + "\n")

    def replace_tar(self, manifest, data):
        archive_path = self.output / manifest["archive"]["filename"]
        with archive_path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0, compresslevel=9) as compressed:
            compressed.write(data)
        self.refresh_manifest(manifest)

    def legacy_fixture(self, manifest, add_legacy_asset=False, add_current_asset=False):
        """Rewrite only this test's owned fixture into retained format 1."""
        old_path = self.output / manifest["archive"]["filename"]
        with tarfile.open(old_path, "r:gz") as archive:
            prefix = manifest["archive"]["root"] + "/"
            contents = {member.name[len(prefix):]: archive.extractfile(member).read()
                        for member in archive if member.isfile()}
        assets = []
        if add_legacy_asset:
            assets.append(("assets/numjinn-logo.png", b"original legacy fixture image"))
        if add_current_asset:
            assets.append(("assets/opennumlab-logo.png", b"current brand fixture image"))
        for name, data in assets:
            contents[name] = data
            manifest["files"].append({"path": name, "sha256": source.digest(data), "size": len(data),
                                      "executable": False, "mode": "0644",
                                      "git_blob": hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()})
            manifest["files"].sort(key=lambda record: record["path"])
        manifest.update(schema_version=1, product="NumJinn")
        root = "numjinn-source-" + manifest["source"]["commit"]
        manifest["archive"].update(root=root, filename=root + ".tar.gz")
        source.write_archive(self.output / manifest["archive"]["filename"], root, manifest["files"], contents)
        old_path.unlink()
        manifest["source"]["inventory_sha256"] = source.inventory_digest(manifest["files"])
        self.refresh_manifest(manifest)
        return manifest

    def test_new_creation_uses_schema_two_and_only_the_current_logo(self):
        self.write("assets/opennumlab-logo.png", "current original image")
        self.write("assets/numjinn-logo.png", "retained legacy alias")
        manifest = self.create(reference=self.commit_all())
        self.assertEqual((manifest["schema_version"], manifest["product"]), (2, "OpenNumLab"))
        self.assertEqual(manifest["archive"]["root"], "opennumlab-source-" + manifest["source"]["commit"])
        names = {record["path"] for record in manifest["files"]}
        self.assertIn("assets/opennumlab-logo.png", names)
        self.assertNotIn("assets/numjinn-logo.png", names)
        self.assertEqual(source.verify(self.output), manifest)

    def test_legacy_schema_one_and_legacy_logo_remain_verifiable(self):
        manifest = self.legacy_fixture(self.create(), add_legacy_asset=True)
        with patch.object(source, "git", side_effect=AssertionError("Legacy verify must not use Git")):
            self.assertEqual(source.verify(self.output), manifest)

    def test_legacy_schema_one_rejects_current_logo_alone_or_with_legacy_logo(self):
        for legacy in (False, True):
            with self.subTest(also_legacy_logo=legacy):
                self.output = self.root / ("legacy-current-logo-" + str(legacy))
                self.legacy_fixture(self.create(), add_legacy_asset=legacy, add_current_asset=True)
                with self.assertRaisesRegex(ValueError, "Forbidden source inventory"):
                    source.verify(self.output)

    def test_mixed_source_schema_product_prefix_and_legacy_logo_are_rejected(self):
        baseline = self.create()
        for fields, message in (({"schema_version": 1, "product": "NumJinn"}, "Archive identity"),
                                ({"product": "NumJinn"}, "schema/product"),
                                ({"schema_version": 1, "product": "OpenNumLab"}, "schema/product"),
                                ({"schema_version": True}, "schema")):
            with self.subTest(fields=fields):
                manifest = copy.deepcopy(baseline)
                manifest.update(fields)
                self.refresh_manifest(manifest)
                with self.assertRaisesRegex(ValueError, message):
                    source.verify(self.output)
        manifest = self.legacy_fixture(copy.deepcopy(baseline), add_legacy_asset=True)
        # Correct prefix/product cannot relabel a v1-only legacy asset as v2.
        old_path = self.output / manifest["archive"]["filename"]
        with tarfile.open(old_path, "r:gz") as archive:
            prefix = manifest["archive"]["root"] + "/"
            contents = {member.name[len(prefix):]: archive.extractfile(member).read()
                        for member in archive if member.isfile()}
        manifest.update(schema_version=2, product="OpenNumLab")
        root = "opennumlab-source-" + manifest["source"]["commit"]
        manifest["archive"].update(root=root, filename=root + ".tar.gz")
        source.write_archive(self.output / manifest["archive"]["filename"], root, manifest["files"], contents)
        old_path.unlink()
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "Forbidden source inventory"):
            source.verify(self.output)

    def test_deterministic_despite_dirty_worktree_mtimes_and_destination(self):
        first = self.create()
        expected = {p.name: p.read_bytes() for p in self.output.iterdir()}
        self.write("README.md", "private worktree marker " + str(self.root / "local-marker") + "\n")
        self.write("crates/test/src/untracked.rs", "untracked private source\n")
        (self.repo / "scripts/setup.sh").chmod(0o600)
        for path in self.repo.rglob("*.rs"):
            os.utime(path, (1234567890, 1234567890))
        other = self.root / "different-parent/renamed-source-set"
        second = self.create(output=other)
        self.assertEqual(first, second)
        self.assertEqual(expected, {p.name: p.read_bytes() for p in other.iterdir()})
        self.assertNotIn(str(self.repo), json.dumps(second))
        self.assertNotIn(str(self.root), json.dumps(second))
        self.assertNotIn("local-marker", json.dumps(second))

    def test_exact_file_inventory_and_canonical_tar_metadata(self):
        self.create()
        manifest, members = self.archive_contents()
        prefix = manifest["archive"]["root"] + "/"
        regular = {name[len(prefix):]: (member, data) for name, (member, data) in members.items() if member.isfile()}
        records = {record["path"]: record for record in manifest["files"]}
        self.assertEqual(set(regular), set(records))
        self.assertEqual(list(members), sorted(members))
        for name, (member, data) in regular.items():
            self.assertEqual(len(data), records[name]["size"])
            self.assertEqual(hashlib.sha256(data).hexdigest(), records[name]["sha256"])
            self.assertEqual(member.mode, int(records[name]["mode"], 8))
        self.assertEqual(regular["scripts/setup.sh"][0].mode, 0o755)
        for member, _ in members.values():
            self.assertEqual((member.uid, member.gid, member.uname, member.gname, member.mtime), (0, 0, "", "", 0))
        self.assertEqual(source.verify(self.output), manifest)

    def test_value_provisioning_source_inputs_preserve_bytes_and_modes_without_widening_selection(self):
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
        self.create(reference=self.commit_all())
        manifest, members = self.archive_contents()
        records = {record["path"]: record for record in manifest["files"]}
        prefix = manifest["archive"]["root"] + "/"
        for name, (text, mode) in selected.items():
            with self.subTest(path=name):
                member, data = members[prefix + name]
                self.assertEqual(data, text.encode())
                self.assertEqual(member.mode, mode)
                self.assertEqual(records[name]["mode"], format(mode, "04o"))
                self.assertEqual(records[name]["executable"], mode == 0o755)
        self.assertFalse(set(records).intersection(denied))
        self.assertFalse(any(b"VALUE_PROVISIONING_PRIVATE_SENTINEL" in data for _, data in members.values()))
        self.assertEqual(source.verify(self.output), manifest)

    def test_forbidden_tracked_and_untracked_content_is_absent(self):
        denied = ("docs/research/private.md", "docs/research/evidence/local.mat", "artifacts/report.json",
                  "crates/vendor/source.rs", "native/vendor.oct", "tests/fixtures/generated.slx",
                  "tests/fixtures/generated.mat", "target/generated.rs", "dist/stale.json")
        for name in denied:
            self.write(name, "PRIVATE_SENTINEL")
        ref = self.commit_all()
        self.write("crates/test/src/untracked.rs", "PRIVATE_UNTRACKED")
        self.create(reference=ref)
        manifest, members = self.archive_contents()
        names = {record["path"] for record in manifest["files"]}
        self.assertFalse(names.intersection(denied))
        self.assertNotIn("crates/test/src/untracked.rs", names)
        self.assertFalse(any(".git" in Path(name).parts for name in members))
        self.assertFalse(any(b"PRIVATE_SENTINEL" in data or b"PRIVATE_UNTRACKED" in data for _, data in members.values()))

    def test_git_attributes_cannot_omit_or_substitute_blob_contents(self):
        self.write(".gitattributes", "README.md export-ignore\ncrates/test/src/conformance/reference.rs export-subst\n")
        ref = self.commit_all()
        self.create(reference=ref)
        manifest, members = self.archive_contents()
        prefix = manifest["archive"]["root"] + "/"
        self.assertEqual(members[prefix + "README.md"][1], b"original README.md\n")
        self.assertEqual(members[prefix + "crates/test/src/conformance/reference.rs"][1], b"// raw $Format:%H$\n")

    def test_selected_symlink_is_rejected_without_output(self):
        secret = self.root / "private"
        secret.write_text("never dereference")
        (self.repo / "crates/test/src/escape.rs").symlink_to(secret)
        ref = self.commit_all()
        with self.assertRaisesRegex(ValueError, "regular Git file"):
            self.create(reference=ref)
        self.assertFalse(self.output.exists())

    def test_selected_gitlink_is_rejected_without_output(self):
        self.git("update-index", "--add", "--cacheinfo", "160000," + self.commit + ",crates/test/src/link.rs")
        self.git("commit", "-qm", "Gitlink fixture")
        with self.assertRaisesRegex(ValueError, "regular Git file"):
            self.create(reference=self.git("rev-parse", "HEAD"))
        self.assertFalse(self.output.exists())

    def test_casefold_directory_collisions_are_rejected_portably(self):
        blob = self.git("hash-object", "-w", "--stdin", data="// fixture\n")
        for name in ("crates/Example/a.rs", "crates/example/b.rs"):
            self.git("-c", "core.ignorecase=false", "update-index", "--add", "--cacheinfo", "100644," + blob + "," + name)
        self.git("commit", "-qm", "Casefold directory collision")
        with self.assertRaisesRegex(ValueError, "extraction collision"):
            self.create(reference=self.git("rev-parse", "HEAD"))

    def test_backslash_source_path_is_rejected(self):
        self.write("crates/test/src/unsafe\\name.rs", "// fixture\n")
        ref = self.commit_all()
        with self.assertRaisesRegex(ValueError, "Unsafe source path"):
            self.create(reference=ref)

    def test_moving_ref_is_resolved_only_once(self):
        resolver = source.resolve_commit
        def move(repo, reference):
            fixed = resolver(repo, reference)
            self.write("README.md", "later branch content\n")
            self.commit_all()
            return fixed
        with patch.object(source, "resolve_commit", side_effect=move):
            manifest = self.create(reference="HEAD")
        self.assertEqual(manifest["source"]["commit"], self.commit)
        _, members = self.archive_contents()
        self.assertEqual(members[manifest["archive"]["root"] + "/README.md"][1], b"original README.md\n")

    def test_git_replace_does_not_relabel_another_source_tree(self):
        expected_tree = self.git("rev-parse", self.commit + "^{tree}")
        self.write("README.md", "replacement tree\n")
        replacement = self.commit_all()
        self.git("replace", self.commit, replacement)
        manifest = self.create()
        self.assertEqual(manifest["source"]["commit"], self.commit)
        self.assertEqual(manifest["source"]["tree"], expected_tree)
        _, members = self.archive_contents()
        self.assertEqual(members[manifest["archive"]["root"] + "/README.md"][1], b"original README.md\n")

    def test_invalid_ref_and_uncommitted_exporter_fail_closed(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.create(reference="missing-ref")
        self.write("scripts/release-source.py", "# unrelated exporter\n")
        ref = self.commit_all()
        with self.assertRaisesRegex(ValueError, "must be committed"):
            self.create(reference=ref)
        self.assertFalse(self.output.exists())

    def test_existing_output_and_failed_write_preserve_prior_files(self):
        self.output.mkdir()
        marker = self.output / "previous"
        marker.write_text("keep")
        with self.assertRaisesRegex(ValueError, "Output already exists"):
            self.create()
        self.assertEqual(marker.read_text(), "keep")
        other = self.root / "new-source-set"
        with patch.object(source, "write_archive", side_effect=OSError("injected disk failure")):
            with self.assertRaisesRegex(OSError, "disk failure"):
                self.create(output=other)
        self.assertFalse(other.exists())
        self.assertEqual(list(self.root.glob(".opennumlab-source-*")), [])
        self.assertEqual(marker.read_text(), "keep")

    def test_extra_archive_member_is_rejected_even_with_updated_checksums(self):
        manifest = self.create()
        archive_path = self.output / manifest["archive"]["filename"]
        with tarfile.open(archive_path, "r:gz") as archive:
            original = [(member, archive.extractfile(member).read() if member.isfile() else None) for member in archive.getmembers()]
        with archive_path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.PAX_FORMAT) as archive:
                for member, data in original:
                    archive.addfile(member, io.BytesIO(data) if data is not None else None)
                extra = tarfile.TarInfo(manifest["archive"]["root"] + "/.git/config")
                extra.size = 7
                archive.addfile(extra, io.BytesIO(b"PRIVATE"))
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "Extra archive member"):
            source.verify(self.output)

    def test_buffered_payload_after_tar_end_is_rejected_with_updated_checksums(self):
        manifest = self.create()
        archive_path = self.output / manifest["archive"]["filename"]
        data = gzip.decompress(archive_path.read_bytes())
        # Keep both normal end blocks and total record length; put the payload
        # in padding that a streaming tar reader has already buffered past EOF.
        self.replace_tar(manifest, data[:-7] + b"PRIVATE")
        with self.assertRaisesRegex(ValueError, "trailing archive data"):
            source.verify(self.output)

    def test_missing_tar_end_padding_is_rejected_with_updated_checksums(self):
        manifest = self.create()
        archive_path = self.output / manifest["archive"]["filename"]
        data = gzip.decompress(archive_path.read_bytes())
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as archive:
            last = archive.getmembers()[-1]
            final_end = last.offset_data + ((last.size + 511) // 512) * 512
        self.replace_tar(manifest, data[:final_end])
        with self.assertRaisesRegex(ValueError, "end padding"):
            source.verify(self.output)

    def test_payload_in_member_padding_is_rejected_with_updated_checksums(self):
        manifest = self.create()
        archive_path = self.output / manifest["archive"]["filename"]
        data = gzip.decompress(archive_path.read_bytes())
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as archive:
            member = archive.getmember(manifest["archive"]["root"] + "/README.md")
            end = member.offset_data + member.size
        self.replace_tar(manifest, data[:end] + b"PRIVATE" + data[end + 7:])
        with self.assertRaisesRegex(ValueError, "member padding"):
            source.verify(self.output)

    def test_false_git_blob_identity_is_rejected_with_updated_checksums(self):
        manifest = self.create()
        manifest["files"][0]["git_blob"] = "0" * len(self.commit)
        manifest["source"]["inventory_sha256"] = source.inventory_digest(manifest["files"])
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "Git blob mismatch"):
            source.verify(self.output)

    def test_object_id_lengths_must_match_the_repository_format(self):
        manifest = self.create()
        original_tree = manifest["source"]["tree"]
        manifest["source"]["tree"] = "0" * 64
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "Invalid source tree"):
            source.verify(self.output)
        manifest["source"]["tree"] = original_tree
        manifest["files"][0]["git_blob"] = "0" * 64
        manifest["source"]["inventory_sha256"] = source.inventory_digest(manifest["files"])
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "Invalid source file record"):
            source.verify(self.output)

    def test_file_directory_prefix_conflicts_are_rejected_before_tar_read(self):
        manifest = self.create()
        record = dict(next(record for record in manifest["files"] if record["path"] == "crates/test/src/conformance.rs"))
        record["path"] += "/child.rs"
        manifest["files"].append(record)
        manifest["files"].sort(key=lambda entry: entry["path"])
        manifest["source"]["inventory_sha256"] = source.inventory_digest(manifest["files"])
        self.refresh_manifest(manifest)
        with self.assertRaisesRegex(ValueError, "File/directory extraction collision"):
            source.verify(self.output)

    def test_regular_archive_name_with_trailing_slash_is_rejected(self):
        manifest = self.create()
        archive_path = self.output / manifest["archive"]["filename"]
        with tarfile.open(archive_path, "r:gz") as archive:
            original = [(member, archive.extractfile(member).read() if member.isfile() else None) for member in archive.getmembers()]
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for member, data in original:
                if member.name == manifest["archive"]["root"] + "/README.md":
                    member.name += "/"
                archive.addfile(member, io.BytesIO(data) if data is not None else None)
        self.replace_tar(manifest, raw.getvalue())
        with self.assertRaisesRegex(ValueError, "Unexpected archive path/type"):
            source.verify(self.output)

    def test_verification_needs_no_git_or_build_tools_and_detects_tampering(self):
        manifest = self.create()
        with patch.object(source, "git", side_effect=AssertionError("Git must not execute")):
            source.verify(self.output)
        env = dict(os.environ, PATH=str(self.root / "no-tools"))
        result = subprocess.run([sys.executable, "-B", str(SCRIPTS / "release-source.py"), "verify", "--directory", str(self.output)],
                                env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        archive_path = self.output / manifest["archive"]["filename"]
        data = archive_path.read_bytes()
        archive_path.write_bytes(data[:-1] + bytes([data[-1] ^ 1]))
        with self.assertRaisesRegex(ValueError, "Archive hash/size mismatch"):
            source.verify(self.output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
