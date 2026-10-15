#!/usr/bin/env python3
"""Synthetic assembly and installed-launcher contracts; no Cargo/Octave builds.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("release_package", SCRIPTS / "release-package.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class PackageTests(unittest.TestCase):
    OPTIONAL_DOC_FILES = ("ROADMAP.md", "roadmap.json", "docs/WORKSPACE_PROTOCOL.md",
                          "docs/VALUE_PAGE_PROTOCOL.md", "docs/VALUE_EDIT_PROTOCOL.md")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mathlab-release-test-")
        self.root = Path(self.temp.name)
        self.repo = self.root / "repository"
        self.repo.mkdir()
        self.destination = self.root / "distribution/OpenNumLab.app"
        self.env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
        self.git("init", "-q")
        self.git("config", "user.name", "Release Tests")
        self.git("config", "user.email", "tests@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        for name in release.ROOT_FILES:
            self.write(name, "fixture " + name)
        self.write("examples/getting_started.m", "assert(1+1 == 2);\n")
        self.write("examples/other.m", "x = 1;\n")
        self.write("crates/test/src/main.rs", "fn main() {}\n")
        self.write("native/bridge.cc", "// original source\n")
        self.write("compatibility/conformance/fixtures/original.m", "assert(true);\n")
        self.write("docs/research/private.m", "never ship\n")
        self.write("docs/research/evidence/local.mat", "private golden\n")
        self.write("artifacts/local.json", "private report\n")
        self.write("crates/vendor/file.rs", "vendor source denied\n")
        self.write("native/vendor.dylib", "vendor binary denied\n")
        self.write("tests/fixtures/generated.mat", "MATLAB generated denied\n")
        self.write("scripts/release-launcher.sh", (SCRIPTS / "release-launcher.sh").read_text())
        self.write(".gitignore", "/target/\n/native/*.oct\n/native/runtime-version.txt\n")
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture source")
        self.executable = self.root / "fake-desktop"
        self.executable.write_text("#!/bin/bash\nset -eu\nprintf '%s\\n' \"$PWD\" \"$@\" > \"$TEST_CAPTURE\"\nmkdir -p .mathlab\nprintf 'recovery\\n' > .mathlab/session.json\n")
        self.executable.chmod(0o755)

    def tearDown(self):
        # Restore read-only test directories for cleanup on all host platforms.
        for p in self.root.rglob("*"):
            if p.is_dir():
                p.chmod(0o755)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, env=self.env, text=True).strip()

    def write(self, name, data):
        p = self.repo / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(data)
        return p

    def builder(self, repo, native):
        result = {"executables": {"mathlab": self.executable, "mathlab-desktop": self.executable},
                  "version": "2.7.3-alpha.4", "native": {
                      "status": "not_included", "adapter_profile": dict(release.NATIVE_OMITTED_PROFILE),
                      "value_component": dict(release.VALUE_OMITTED_COMPONENT), "reason": "Native profile not requested"},
                  "executable_sha256": {name: release.sha(self.executable) for name in ("mathlab", "mathlab-desktop")}}
        if native:
            result["native"] = {"status": "enabled_protocol_load_passed", "artifact_sha256": {},
                                "adapter_profile": dict(release.NATIVE_REQUIRED_PROFILE),
                                "runtime_version": "11.3.0", "sdk": {"OCTAVE_VERSION": "11.3.0"},
                                "runtime_executable_sha256": "a" * 64,
                                "protocols": {"debug": {"protocol": "mathlab-debugger", "version": 4},
                                              "scope": {"protocol": "mathlab-scope", "version": 2},
                                              "input": {"protocol": "mathlab-input", "version": 1},
                                              "value": dict(release.VALUE_PROTOCOL)}}
            for name in release.NATIVE_FILES:
                p = repo / "native" / name
                p.write_text("11.3.0\n" if name == "runtime-version.txt" else "synthetic ABI " + name)
                result["native"]["artifact_sha256"][name] = release.sha(p)
            module_hash = result["native"]["artifact_sha256"]["mathlab_value_bridge.oct"]
            result["native"]["value_component"] = {"status": "required", "path": release.VALUE_PATH,
                                                     "capability": release.VALUE_CAPABILITY, "sha256": module_hash}
            result["native"]["worker_observation"] = {
                "method": "built-cli-doctor", "ready": True, "cli_executable_sha256": release.sha(self.executable),
                "value_adapter": str((repo / "native/mathlab_value_bridge.oct").resolve()),
                "runtime": "/synthetic/external/octave-cli", "worker_capabilities": sorted(release.VALUE_REQUIRED_CAPABILITIES),
                "module_sha256": module_hash, "runtime_executable_sha256": "a" * 64}
            result["linked_libraries"] = {name: "synthetic linked dependencies for " + name for name in release.NATIVE_MODULES}
        return result

    def notices(self, repo, destination):
        destination.mkdir()
        (destination / "index.json").write_text('{"synthetic":true}\n')

    def assemble(self, **kwargs):
        return release.package(self.repo, self.destination, builder=kwargs.pop("builder", self.builder),
                               notice_builder=kwargs.pop("notice_builder", self.notices), **kwargs)

    def manifest(self):
        return json.loads((self.destination / "Contents/Resources/release.json").read_text())

    def refresh_metadata(self, manifest):
        """Make envelope hashes valid so mutations exercise semantic validation."""
        resources = self.destination / "Contents/Resources"
        manifest["payload"] = [record for record in release.inventory(self.destination)
                               if record["path"] not in {"Contents/Resources/release.json", "Contents/Resources/checksums.sha256"}]
        (resources / "release.json").write_text(json.dumps(manifest, indent=2) + "\n")
        records = [record for record in release.inventory(self.destination) if record["path"] != "Contents/Resources/checksums.sha256"]
        (resources / "checksums.sha256").write_text("".join(record["sha256"] + "  " + record["path"] + "\n" for record in records))

    def test_stale_files_removed_and_previous_app_preserved(self):
        self.destination.mkdir(parents=True)
        (self.destination / "stale-private.mat").write_text("old app")
        self.write("examples/untracked.m", "untracked example")
        with self.assertRaisesRegex(RuntimeError, "Commit intended release source"):
            self.assemble()
        (self.repo / "examples/untracked.m").unlink()
        self.write("examples/stale.m", "once included")
        self.git("add", "examples/stale.m")
        self.git("commit", "-qm", "Temporary example")
        (self.repo / "examples/stale.m").unlink()
        result = self.assemble(allow_dirty=True)
        self.assertFalse((self.destination / "stale-private.mat").exists())
        self.assertFalse((self.destination / "Contents/Resources/examples/stale.m").exists())
        self.assertEqual((Path(result["previous_bundle"]) / "stale-private.mat").read_text(), "old app")
        self.assertTrue(self.manifest()["source"]["dirty"])

    def test_source_allowlist_versions_and_final_hashes(self):
        self.assemble()
        manifest = self.manifest()
        paths = {r["path"] for r in manifest["source"]["files"]}
        self.assertIn("crates/test/src/main.rs", paths)
        self.assertIn("compatibility/conformance/fixtures/original.m", paths)
        for name in ("docs/research/private.m", "artifacts/local.json", "crates/vendor/file.rs", "native/vendor.dylib", "tests/fixtures/generated.mat"):
            self.assertNotIn(name, paths)
        self.assertFalse(manifest["source"]["dirty"])
        self.assertEqual(manifest["source"]["commit"], self.git("rev-parse", "HEAD"))
        plist = plistlib.loads((self.destination / "Contents/Info.plist").read_bytes())
        self.assertEqual(plist["CFBundleShortVersionString"], "2.7.3")
        self.assertEqual(plist["CFBundleVersion"], manifest["bundle_version"])
        self.assertEqual(plist["CFBundleName"], "OpenNumLab")
        self.assertEqual(plist["CFBundleDisplayName"], "OpenNumLab (Development alpha)")
        self.assertEqual(plist["CFBundleIdentifier"], "org.openmathlab.desktop")
        self.assertEqual(plist["CFBundleExecutable"], "mathlab-launcher")
        self.assertEqual(manifest["product"], "OpenNumLab")
        self.assertIn("Application Support/OpenNumLab/Projects/Getting Started/.mathlab/",
                      manifest["distribution"]["writable_state"])
        release.validate(self.destination)
        (self.destination / "Contents/Resources/examples/other.m").write_text("tampered")
        with self.assertRaisesRegex(RuntimeError, "Payload hash mismatch"):
            release.validate(self.destination)

    def test_default_cli_destination_is_opennumlab(self):
        with patch.object(release.sys, "argv", ["release-package.py"]), \
                patch.object(release.sys, "platform", "darwin"), \
                patch.object(release, "package", return_value={}) as package, \
                patch("builtins.print"):
            release.main()
        repo = Path(release.__file__).resolve().parent.parent
        package.assert_called_once_with(repo, repo / "dist/OpenNumLab.app", False, allow_dirty=False)

    def test_opennumlab_brand_asset_is_exact_and_legacy_asset_is_excluded(self):
        self.assertEqual(release.BRAND_FILES, {"assets/opennumlab-logo.png"})
        self.assertEqual(release.LEGACY_BRAND_FILES, {"assets/numjinn-logo.png"})
        assets = {"assets/opennumlab-logo.png": "new canonical logo bytes\n",
                  "assets/numjinn-logo.png": "legacy logo bytes\n",
                  "assets/opennumlab-logo-copy.png": "unapproved similar logo\n"}
        for name, data in assets.items():
            self.write(name, data)
        self.git("add", *assets)
        self.git("commit", "-qm", "Prospective and legacy brand fixtures")
        self.assemble()
        resources = self.destination / "Contents/Resources"
        records = {record["path"]: record for record in self.manifest()["source"]["files"]}
        canonical = "assets/opennumlab-logo.png"
        self.assertTrue(release.allowed_source(canonical))
        self.assertEqual((resources / "source" / canonical).read_bytes(), (self.repo / canonical).read_bytes())
        self.assertEqual((resources / "opennumlab-logo.png").read_bytes(), (self.repo / canonical).read_bytes())
        self.assertEqual(records[canonical]["sha256"], release.sha(self.repo / canonical))
        for name in assets.keys() - {canonical}:
            with self.subTest(path=name):
                self.assertFalse(release.allowed_source(name))
                self.assertNotIn(name, records)
                self.assertFalse((resources / "source" / name).exists())
                self.assertFalse((resources / Path(name).name).exists())

    def test_new_product_preserves_legacy_bundle_and_user_project(self):
        legacy_bundle = self.destination.parent / "NumJinn.app"
        legacy_payload = legacy_bundle / "Contents/Resources/release.json"
        legacy_payload.parent.mkdir(parents=True)
        legacy_payload.write_text('{"product":"NumJinn","historical":true}\n')
        legacy_project = self.root / "home/Library/Application Support/NumJinn/Projects/Getting Started"
        (legacy_project / ".mathlab").mkdir(parents=True)
        (legacy_project / "getting_started.m").write_text("legacy user edit\n")
        (legacy_project / ".mathlab/session.json").write_text("legacy recovery\n")
        bundle_before = release.inventory(legacy_bundle)
        project_before = release.inventory(legacy_project)
        result = self.assemble()
        self.assertIsNone(result["previous_bundle"])
        project = self.root / "home/Library/Application Support/OpenNumLab/Projects/Getting Started"
        self.assertEqual(self.launch()[:3], [str(project), "--cwd", str(project)])
        self.assertEqual((project / "getting_started.m").read_text(), "assert(1+1 == 2);\n")
        self.assertEqual(release.inventory(legacy_bundle), bundle_before)
        self.assertEqual(release.inventory(legacy_project), project_before)
        self.assertEqual(list(self.destination.parent.glob("NumJinn.app.previous-*")), [])

    def test_optional_docs_do_not_expand_required_fixture_roots(self):
        self.assertEqual(release.ROOT_FILES,
                         {"Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "THIRD_PARTY.md"})
        self.assertTrue(all(not (self.repo / name).exists() for name in self.OPTIONAL_DOC_FILES))
        self.assemble()
        paths = {record["path"] for record in self.manifest()["source"]["files"]}
        self.assertTrue(release.ROOT_FILES.issubset(paths))
        self.assertTrue(paths.isdisjoint(self.OPTIONAL_DOC_FILES))

    def test_committed_optional_docs_are_included_with_exact_bytes(self):
        contents = {name: "Reviewed optional source: " + name + "\n"
                    for name in self.OPTIONAL_DOC_FILES}
        contents["roadmap.json"] = '{"schema_version":1,"scope":"original roadmap"}\n'
        for name, text in contents.items():
            self.write(name, text)
        with self.assertRaisesRegex(RuntimeError, "Commit intended release source"):
            release.source_snapshot(self.repo)
        self.git("add", *self.OPTIONAL_DOC_FILES)
        self.git("commit", "-qm", "Reviewed optional documentation")
        self.assemble()
        records = {record["path"]: record for record in self.manifest()["source"]["files"]}
        for name, text in contents.items():
            with self.subTest(path=name):
                self.assertIn(name, records)
                self.assertEqual((self.destination / "Contents/Resources/source" / name).read_text(), text)
                self.assertEqual(records[name]["sha256"], release.sha(self.repo / name))

    def test_nearby_unapproved_docs_and_roadmaps_remain_excluded(self):
        denied = ("roadmap.md", "ROADMAP.json", "ROADMAP.private.md", "roadmap-local.json",
                  "docs/ROADMAP.md", "docs/roadmap/ROADMAP.md", "docs/OTHER_PROTOCOL.md",
                  "docs/WORKSPACE_PROTOCOL_NOTES.md", "docs/VALUE_PAGE_PROTOCOL_DRAFT.md",
                  "docs/research/WORKSPACE_PROTOCOL.md")
        for name in denied:
            self.write(name, "Unapproved nearby document\n")
        self.git("add", *denied)
        self.git("commit", "-qm", "Nearby documents are not release source")
        self.assemble()
        paths = {record["path"] for record in self.manifest()["source"]["files"]}
        for name in denied:
            with self.subTest(path=name):
                self.assertFalse(release.allowed_source(name))
                self.assertNotIn(name, paths)
                self.assertFalse((self.destination / "Contents/Resources/source" / name).exists())

    def test_dirty_source_bytes_are_exact(self):
        self.write("crates/test/src/main.rs", "// local changes\n")
        with self.assertRaisesRegex(RuntimeError, "require a clean checkout"):
            self.assemble()
        self.assemble(allow_dirty=True)
        self.assertTrue(self.manifest()["source"]["dirty"])
        self.assertFalse(self.manifest()["distribution"]["clean_candidate"])
        self.assertEqual((self.destination / "Contents/Resources/source/crates/test/src/main.rs").read_text(), "// local changes\n")

    def test_module_file_and_same_named_directory_export_consistently(self):
        self.write("crates/test/src/conformance.rs", "mod reference;\n")
        self.write("crates/test/src/conformance/reference.rs", "// original import helper\n")
        self.git("add", "crates/test/src/conformance.rs", "crates/test/src/conformance/reference.rs")
        self.git("commit", "-qm", "Module file and directory share a basename")
        self.assemble()
        paths = [record["path"] for record in self.manifest()["source"]["files"]]
        self.assertEqual(paths, sorted(paths))
        self.assertIn("crates/test/src/conformance.rs", paths)
        self.assertIn("crates/test/src/conformance/reference.rs", paths)
        release.validate(self.destination)

    def test_source_and_directory_symlinks_are_rejected(self):
        source = self.repo / "crates/test/src/main.rs"
        source.unlink()
        source.symlink_to(self.executable)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.assemble()
        source.unlink()
        self.write("crates/test/src/main.rs", "fn main() {}\n")
        elsewhere = self.root / "elsewhere"
        (self.repo / "crates/test/src").rename(elsewhere)
        (self.repo / "crates/test/src").symlink_to(elsewhere, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.assemble()

    def test_build_or_notice_failure_preserves_old_app(self):
        self.destination.mkdir(parents=True)
        old = self.destination / "old"
        old.write_text("still usable")
        def fail(*_):
            raise RuntimeError("injected failure")
        for kwargs in ({"builder": fail}, {"notice_builder": fail}):
            with self.assertRaisesRegex(RuntimeError, "injected"):
                self.assemble(**kwargs)
            self.assertEqual(old.read_text(), "still usable")
        self.assertEqual(list(self.destination.parent.glob(".opennumlab-stage-*")), [])

    def test_frozen_source_mutation_or_binary_change_aborts(self):
        def edit(repo, native):
            result = self.builder(repo, native)
            (repo / "crates/test/src/main.rs").write_text("// unexpected frozen edit\n")
            return result
        with self.assertRaisesRegex(RuntimeError, "Frozen source changed during build"):
            self.assemble(builder=edit)
        def change_binary(repo, native):
            result = self.builder(repo, native)
            self.executable.write_text("new executable")
            return result
        with self.assertRaisesRegex(RuntimeError, "executable changed"):
            self.assemble(builder=change_binary)
        self.assertFalse(self.destination.exists())

    def test_publication_exchange_failure_retains_previous_app(self):
        self.destination.mkdir(parents=True)
        (self.destination / "old").write_text("rollback")
        candidate = self.root / "candidate.app"
        candidate.mkdir()
        with patch.object(release, "atomic_exchange", side_effect=OSError("injected exchange failure")):
            with self.assertRaisesRegex(OSError, "injected exchange"):
                release.publish(candidate, self.destination)
        self.assertEqual((self.destination / "old").read_text(), "rollback")
        self.assertTrue(candidate.is_dir())

    def test_interruption_after_atomic_exchange_retains_both_bundles(self):
        self.destination.mkdir(parents=True)
        (self.destination / "old").write_text("previous app")
        candidate = self.root / "candidate.app"
        candidate.mkdir()
        (candidate / "new").write_text("validated app")
        exchange = release.atomic_exchange
        def interrupt_after(first, second):
            exchange(first, second)
            raise KeyboardInterrupt("interrupted after kernel exchange")
        with patch.object(release, "atomic_exchange", side_effect=interrupt_after):
            with self.assertRaises(KeyboardInterrupt):
                release.publish(candidate, self.destination)
        self.assertEqual((self.destination / "new").read_text(), "validated app")
        previous = list(self.destination.parent.glob("OpenNumLab.app.previous-*"))
        self.assertEqual(len(previous), 1)
        self.assertEqual((previous[0] / "old").read_text(), "previous app")

    def test_checkout_can_change_after_frozen_export_without_changing_candidate(self):
        commit = self.git("rev-parse", "HEAD")
        def edit_checkout(repo, native):
            self.assertNotEqual(repo, self.repo)
            self.write("crates/test/src/main.rs", "// later checkout edit\n")
            self.git("add", "crates/test/src/main.rs")
            self.git("commit", "-qm", "Later source")
            self.assertEqual(release.run(["git", "rev-parse", "HEAD"], repo), commit)
            return self.builder(repo, native)
        self.assemble(builder=edit_checkout)
        self.assertEqual(self.manifest()["source"]["commit"], commit)
        self.assertTrue(self.manifest()["source"]["frozen_export"])
        self.assertFalse(self.manifest()["source"]["dirty"])
        self.assertEqual((self.destination / "Contents/Resources/source/crates/test/src/main.rs").read_text(), "fn main() {}\n")

    def test_native_presence_and_omission_are_explicit(self):
        self.assemble(with_debugger=True)
        self.assertEqual(self.manifest()["build"]["native"]["status"], "enabled_protocol_load_passed")
        self.assertEqual(self.manifest()["build"]["native"]["adapter_profile"], release.NATIVE_REQUIRED_PROFILE)
        self.assertEqual(len(list((self.destination / "Contents/Resources/native").glob("*.oct"))), 4)
        self.assemble()
        self.assertEqual(self.manifest()["build"]["native"]["status"], "not_included")
        self.assertEqual(self.manifest()["build"]["native"]["adapter_profile"], release.NATIVE_OMITTED_PROFILE)
        self.assertEqual(self.manifest()["build"]["native"]["value_component"], release.VALUE_OMITTED_COMPONENT)
        self.assertFalse((self.destination / "Contents/Resources/native").exists())

    def test_unknown_or_malformed_profile_never_becomes_legacy(self):
        self.assemble()
        original = self.manifest()
        malformed = (None, {}, True, 1, "native-omitted-v1",
                     {"schema_version": True, "name": "native-omitted-v1"},
                     {"schema_version": 1.0, "name": "native-omitted-v1"},
                     {"schema_version": "1", "name": "native-omitted-v1"},
                     {"schema_version": 2, "name": "native-omitted-v1"},
                     {"schema_version": 1, "name": "unknown"},
                     {"schema_version": 1, "name": "native-omitted-v1", "extra": False})
        for profile in malformed:
            with self.subTest(profile=profile):
                manifest = copy.deepcopy(original)
                manifest["build"]["native"]["adapter_profile"] = profile
                self.refresh_metadata(manifest)
                with self.assertRaisesRegex(RuntimeError, "malformed native adapter_profile"):
                    release.validate(self.destination)

    def test_legacy_three_adapter_manifest_remains_valid_without_value(self):
        self.assemble(with_debugger=True)
        manifest = self.manifest()
        native = manifest["build"]["native"]
        for field in ("adapter_profile", "value_component", "worker_observation"):
            del native[field]
        del native["artifact_sha256"]["mathlab_value_bridge.oct"]
        del native["protocols"]["value"]
        del manifest["build"]["linked_libraries"]["mathlab_value_bridge.oct"]
        (self.destination / release.VALUE_PATH).unlink()
        self.refresh_metadata(manifest)
        release.validate(self.destination)

    def test_legacy_and_omitted_profiles_reject_rehashed_value_payload_anywhere(self):
        self.assemble()
        original = self.manifest()
        for legacy in (False, True):
            manifest = copy.deepcopy(original)
            if legacy:
                for field in ("adapter_profile", "value_component"):
                    del manifest["build"]["native"][field]
            for name in (release.VALUE_PATH, "Contents/Resources/alternate/mathlab_value_bridge.oct",
                         "Contents/Resources/alternate/MathLab_Value_Bridge.OCT"):
                with self.subTest(legacy=legacy, path=name):
                    module = self.destination / name
                    module.parent.mkdir(parents=True, exist_ok=True)
                    module.write_text("synthetic undeclared module")
                    self.refresh_metadata(manifest)
                    with self.assertRaisesRegex(RuntimeError, "must not bundle"):
                        release.validate(self.destination)
                    module.unlink()
            for field, claim in (("artifact_sha256", {"mathlab_value_bridge.oct": "b" * 64}),
                                 ("protocols", {"value": dict(release.VALUE_PROTOCOL)}),
                                 ("worker_observation", {})):
                with self.subTest(legacy=legacy, claim=field):
                    changed = copy.deepcopy(manifest)
                    changed["build"]["native"][field] = claim
                    self.refresh_metadata(changed)
                    with self.assertRaisesRegex(RuntimeError, "must not bundle"):
                        release.validate(self.destination)

    def test_required_value_profile_rejects_rehashed_identity_and_protocol_changes(self):
        self.assemble(with_debugger=True)
        original = self.manifest()
        changes = [
            ("runtime hash missing", ("runtime_executable_sha256",), None),
            ("runtime hash malformed", ("runtime_executable_sha256",), "A" * 64),
            ("module hash missing", ("artifact_sha256", "mathlab_value_bridge.oct"), None),
            ("wrong component path", ("value_component", "path"), "Contents/Resources/foreign.oct"),
            ("wrong component hash", ("value_component", "sha256"), "b" * 64),
            ("wrong capability", ("value_component", "capability"), "unrelated.v1"),
            ("missing observation", ("worker_observation",), None),
            ("not ready", ("worker_observation", "ready"), False),
            ("wrong CLI", ("worker_observation", "cli_executable_sha256"), "b" * 64),
            ("wrong runtime", ("worker_observation", "runtime_executable_sha256"), "b" * 64),
            ("wrong module", ("worker_observation", "module_sha256"), "b" * 64),
            ("missing capability", ("worker_observation", "worker_capabilities"), []),
            ("capability scalar", ("worker_observation", "worker_capabilities"), release.VALUE_CAPABILITY),
            ("relative observation", ("worker_observation", "value_adapter"), "mathlab_value_bridge.oct"),
            ("SDK mismatch", ("sdk", "OCTAVE_VERSION"), "8.4.0"),
        ]
        changes.extend(("protocol " + field, ("protocols", "value", field), value)
                       for field, value in (("protocol", "foreign"), ("version", True), ("version", 1.0),
                                            ("dense_complex_components", 1), ("max_cells", 1999),
                                            ("max_rank", 31), ("max_replace_leaf_bytes", 134217729), ("extra", 1)))
        for label, path, value in changes:
            with self.subTest(change=label):
                manifest = copy.deepcopy(original)
                current = manifest["build"]["native"]
                for key in path[:-1]:
                    current = current[key]
                current[path[-1]] = value
                self.refresh_metadata(manifest)
                with self.assertRaises(RuntimeError):
                    release.validate(self.destination)
        manifest = copy.deepcopy(original)
        del manifest["build"]["linked_libraries"]["mathlab_value_bridge.oct"]
        self.refresh_metadata(manifest)
        with self.assertRaisesRegex(RuntimeError, "linked-library"):
            release.validate(self.destination)

    def test_omitted_profile_rejects_relocated_or_unrecognized_oct_modules(self):
        self.assemble()
        original = self.manifest()
        for basename in ("mathlab_debug_bridge.oct", "MathLab_Scope_Bridge.OCT", "mathlab_input_bridge.oct", "foreign.oct"):
            with self.subTest(module=basename):
                path = self.destination / "Contents/Resources/alternate" / basename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("synthetic undeclared module")
                self.refresh_metadata(copy.deepcopy(original))
                with self.assertRaisesRegex(RuntimeError, "must not bundle"):
                    release.validate(self.destination)
                path.unlink()

    def test_required_value_profile_rejects_missing_artifacts_and_extra_value_copy(self):
        self.assemble(with_debugger=True)
        original = self.manifest()
        for name in release.NATIVE_FILES:
            with self.subTest(missing=name):
                path = self.destination / "Contents/Resources/native" / name
                data = path.read_bytes()
                path.unlink()
                self.refresh_metadata(copy.deepcopy(original))
                with self.assertRaises(RuntimeError):
                    release.validate(self.destination)
                path.write_bytes(data)
        duplicate = self.destination / "Contents/Resources/alternate/mathlab_value_bridge.oct"
        duplicate.parent.mkdir()
        shutil.copyfile(self.destination / release.VALUE_PATH, duplicate)
        self.refresh_metadata(copy.deepcopy(original))
        with self.assertRaisesRegex(RuntimeError, "exactly the canonical"):
            release.validate(self.destination)

    def test_duplicate_manifest_fields_are_rejected_before_validation(self):
        self.assemble()
        path = self.destination / "Contents/Resources/release.json"
        text = path.read_text().replace('"schema_version": 1,', '"schema_version": 2, "schema_version": 1,', 1)
        path.write_text(text)
        with self.assertRaisesRegex(RuntimeError, "Duplicate JSON field"):
            release.validate(self.destination)

    def test_native_protocol_probe_uses_value_specific_arity_and_validates_limits(self):
        octave = self.root / "selected-octave"
        octave.write_text("synthetic executable")
        tools = {"octave": str(octave), "mkoctfile": "/synthetic/mkoctfile", "version": "11.3.0",
                 "runtime_executable_sha256": release.sha(octave)}
        def synthetic_run(argv, *_args, **_kwargs):
            argv = [str(item) for item in argv]
            if argv[:2] == ["bash", "scripts/build-debugger.sh"]:
                for name in release.NATIVE_FILES:
                    (self.repo / "native" / name).write_text("11.3.0\n" if name == "runtime-version.txt" else "fresh " + name)
                return ""
            if argv[0] == str(octave):
                expression = argv[-1]
                for short, protocol, version in (("debug", "mathlab-debugger", 4), ("scope", "mathlab-scope", 2), ("input", "mathlab-input", 1)):
                    if "mathlab_" + short + "_bridge" in expression:
                        self.assertIn("('protocol',0)", expression)
                        return json.dumps({"protocol": protocol, "version": version})
                self.assertIn("mathlab_value_bridge('protocol')", expression)
                self.assertNotIn("mathlab_value_bridge('protocol',0)", expression)
                return json.dumps(release.VALUE_PROTOCOL)
            if argv[0] == tools["mkoctfile"]:
                return {"OCTAVE_VERSION": "11.3.0", "CXX": "synthetic-cxx"}.get(argv[-1], "synthetic config")
            self.assertEqual(argv, ["synthetic-cxx", "--version"])
            return "synthetic compiler"
        with patch.object(release, "run", side_effect=synthetic_run) as runner:
            identity = release.native_build(self.repo, tools)
        self.assertEqual(identity["protocols"]["value"], release.VALUE_PROTOCOL)
        self.assertEqual(sum("--eval" in call.args[0] for call in runner.call_args_list), 4)
        with self.assertRaisesRegex(RuntimeError, "preexisting adapters"):
            release.native_build(self.repo, tools)

    def test_native_prerequisite_failure_precedes_cargo(self):
        with patch.object(release, "native_preflight", side_effect=RuntimeError("runtime/SDK mismatch")), \
                patch.object(release, "run") as runner:
            with self.assertRaisesRegex(RuntimeError, "runtime/SDK mismatch"):
                release.build(self.repo, True)
        runner.assert_not_called()

    def test_doctor_requires_actual_capabilities_and_matching_fresh_inputs(self):
        identity = self.builder(self.repo, True)
        tools = {"octave": str(self.executable)}
        identity["native"]["runtime_executable_sha256"] = release.sha(self.executable)
        doctor = {"ready": True, "runtime": str(self.executable),
                  "value_adapter": str((self.repo / "native/mathlab_value_bridge.oct").resolve()),
                  "worker_capabilities": sorted(release.VALUE_REQUIRED_CAPABILITIES)}
        with patch.object(release, "run", return_value=json.dumps(doctor)):
            observed = release.observe_value_capability(self.repo, self.executable, tools, identity["native"])
        self.assertEqual(observed["cli_executable_sha256"], release.sha(self.executable))
        mutations = (("ready", False), ("runtime", "relative-octave"), ("runtime", str(self.root / "other-runtime")),
                     ("value_adapter", str(self.root / "foreign.oct")), ("value_adapter", None),
                     ("worker_capabilities", None), ("worker_capabilities", []),
                     ("worker_capabilities", release.VALUE_CAPABILITY),
                     ("worker_capabilities", [release.VALUE_CAPABILITY] * 2))
        for field, value in mutations:
            with self.subTest(field=field, value=value):
                changed = dict(doctor, **{field: value})
                with patch.object(release, "run", return_value=json.dumps(changed)):
                    with self.assertRaises(RuntimeError):
                        release.observe_value_capability(self.repo, self.executable, tools, identity["native"])

    def launch(self, bundle=None, args=(), project=None):
        capture = self.root / "captured-arguments"
        env = dict(os.environ, HOME=str(self.root / "home"), TEST_CAPTURE=str(capture))
        env.pop("MATHLAB_PROJECT_DIR", None)
        if project:
            env["MATHLAB_PROJECT_DIR"] = str(project)
        subprocess.run([str((bundle or self.destination) / "Contents/MacOS/mathlab-launcher"), *args], env=env, check=True, timeout=15)
        return capture.read_text().splitlines()

    def test_read_only_relocated_bundle_uses_writable_persistent_project(self):
        self.assemble()
        relocated = self.root / "Relocated app with spaces.app"
        self.destination.rename(relocated)
        for p in relocated.rglob("*"):
            p.chmod(0o555 if p.is_dir() or p.stat().st_mode & 0o111 else 0o444)
        before = release.inventory(relocated)
        arguments = self.launch(relocated)
        project = self.root / "home/Library/Application Support/OpenNumLab/Projects/Getting Started"
        self.assertEqual(arguments[:3], [str(project), "--cwd", str(project)])
        self.assertEqual(arguments[3:], ["--open", str(project / "getting_started.m")])
        self.assertEqual((project / ".mathlab/session.json").read_text(), "recovery\n")
        (project / "getting_started.m").write_text("user edit\n")
        second = self.launch(relocated)
        self.assertEqual(second, [str(project), "--cwd", str(project)])
        self.assertEqual((project / "getting_started.m").read_text(), "user edit\n")
        self.assertEqual(release.inventory(relocated), before)

    def test_launcher_directory_source_and_state_overrides(self):
        self.assemble()
        project = self.root / "chosen project"
        project.mkdir()
        values = self.launch(args=("--cwd", str(project), "--open", "chosen.m", "--state", "chosen.json"))
        self.assertEqual(values, [str(project), "--cwd", str(project), "--open", "chosen.m", "--state", "chosen.json"])
        values = self.launch(args=("--cwd=" + str(project), "chosen.m"))
        self.assertEqual(values, [str(project), "--cwd", str(project), "chosen.m"])
        custom = self.root / "custom default"
        values = self.launch(args=("--open=chosen.m",), project=custom)
        self.assertEqual(values, [str(custom), "--cwd", str(custom), "--open=chosen.m"])
        relative = os.path.relpath(project, Path.cwd())
        values = self.launch(args=("--cwd", relative, "chosen.m"))
        self.assertEqual(values, [str(project), "--cwd", str(project), "chosen.m"])

    def test_removed_executable_permission_is_rejected(self):
        self.assemble()
        (self.destination / "Contents/MacOS/mathlab-launcher").chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, "Payload hash mismatch"):
            release.validate(self.destination)

    def test_real_builder_receives_private_target_despite_shared_target_environment(self):
        shared = self.repo / "target/release/mathlab"
        shared.parent.mkdir(parents=True)
        shared.write_text("unrelated build")
        def isolated_builder(repo, native, target_dir, cargo_home):
            self.assertNotIn(self.repo, target_dir.parents)
            self.assertNotEqual(target_dir, shared.parent.parent)
            self.assertTrue(cargo_home.is_dir())
            own = target_dir / "release/mathlab"
            own.parent.mkdir(parents=True)
            shutil.copy2(self.executable, own)
            result = self.builder(repo, native)
            result["executables"] = {name: own for name in result["executables"]}
            shared.write_text("replaced by concurrent checkout build")
            return result
        # Exercise the production branch without compiling: replace its injected
        # builder identity, not its internal private-target decision.
        with patch.object(release, "build", isolated_builder), patch.dict(os.environ, CARGO_TARGET_DIR=str(shared.parent.parent)):
            release.package(self.repo, self.destination, builder=isolated_builder, notice_builder=self.notices)
        self.assertEqual((self.destination / "Contents/MacOS/mathlab").read_bytes(), self.executable.read_bytes())


if __name__ == "__main__":
    unittest.main(verbosity=2)
