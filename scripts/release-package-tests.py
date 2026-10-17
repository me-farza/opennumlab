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
import sys
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

    def builder(self, repo, native, startup=None):
        result = {"executables": {"mathlab": self.executable, "mathlab-desktop": self.executable},
                  "version": "2.7.3-alpha.4", "native": {
                      "status": "not_included", "adapter_profile": dict(release.NATIVE_OMITTED_PROFILE),
                      "value_component": dict(release.VALUE_OMITTED_COMPONENT), "reason": "Native profile not requested"},
                  "executable_sha256": {name: release.sha(self.executable) for name in ("mathlab", "mathlab-desktop")}}
        if native or startup is not None:
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
            if startup is not None:
                runtime = startup["startup"]["executable"]
                result["native"]["ghostscript"] = copy.deepcopy(startup["startup"]["ghostscript"])
                result["native"]["runtime_executable_sha256"] = runtime["sha256"]
                result["native"]["worker_observation"].update(runtime=runtime["path"], runtime_executable_sha256=runtime["sha256"])
                self.qualify_native(result, repo / "native", graphics=startup["profile"] == "qualified-graphics-v2")
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

    def qualify_native(self, identity, native_root, graphics=False):
        """Add synthetic accepted core evidence to the existing native fixture."""
        native = identity["native"]
        provider = native_root / "mathlab_intrinsic_bridge.oct"
        provider.write_text("synthetic qualified provider")
        native["artifact_sha256"][provider.name] = release.sha(provider)
        native["adapter_profile"] = {"schema_version": 1, "name": "qualified-core-v2"}
        native["sdk"]["OCTAVE_API_VERSION"] = "api-v61"
        native["protocols"]["intrinsic"] = {
            "protocol": "mathlab-intrinsics", "version": 1, "octave_version": "11.3.0", "octave_api_version": "api-v61",
            "actual_function_object": True, "native_base_queries": True, "unlocked_registry": True,
            "callback_binding_v2": True, "adapter_carrier_v2": True, "provider_shutdown_proxy": True}
        identity["linked_libraries"][provider.name] = "synthetic provider dependencies"
        if graphics:
            module = native_root / release.GRAPHICS_NAME
            module.write_text("synthetic qualified graphics")
            native["artifact_sha256"][module.name] = release.sha(module)
            native["adapter_profile"] = dict(release.NATIVE_GRAPHICS_PROFILE)
            native["protocols"]["graphics"] = dict(release.GRAPHICS_PROTOCOL)
            native["worker_observation"]["worker_capabilities"].append(release.GRAPHICS_CAPABILITY)
            identity["linked_libraries"][module.name] = "synthetic graphics dependencies"
        observation = native["worker_observation"]
        observation["native_modules"] = {
            name: {"path": str((native_root / name).resolve()), "sha256": native["artifact_sha256"][name]}
            for name in release.qualified_native_modules(native["adapter_profile"])}
        observation["value_adapter"] = observation["native_modules"]["mathlab_value_bridge.oct"]["path"]
        context = {"worker_id": "synthetic-worker", "generation": "1", "invocation": "1", "revision": "0"}
        observation["session_context"] = dict(context)
        observation["qualified_startup_status"] = {"profile": native["adapter_profile"]["name"], "context": context, "intrinsics": {
            "protocol": "mathlab-intrinsics", "version": "1", "phase": "ready",
            "provider_file": observation["native_modules"][provider.name]["path"],
            "worker_file": "/synthetic/session/worker.m", "registry_file": "/synthetic/session/registry.m",
            "entry_alias": "__mlentry_fixture", "shutdown_alias": "__mlshutdown_fixture",
            "octave_version": "11.3.0", "octave_api_version": "api-v61",
            "initial_path_admitted": True, "shutdown_registered": True,
            "adapters": {role: {"path": observation["native_modules"]["mathlab_" + role + "_bridge.oct"]["path"],
                                "protocol": protocol, "version": version}
                         for role, protocol, version in (("debug", "mathlab-debugger", "4"), ("scope", "mathlab-scope", "2"),
                                                         ("input", "mathlab-input", "1"), ("value", "mathlab-value", "1"))}}}

        if graphics:
            observation["qualified_startup_status"]["intrinsics"]["adapters"]["graphics"] = {
                "path": observation["native_modules"][release.GRAPHICS_NAME]["path"],
                "protocol": "mathlab-graphics", "version": "1"}

    def qualified_manifest(self, graphics=False):
        """Use a coherent qualified assembly before independent validator mutations."""
        seed, _ = self.startup_seed(graphics=graphics)
        self.assemble(startup_config=seed)
        return self.manifest()

    def startup_seed(self, graphics=False):
        installation = self.root / "selected-installation"
        runtime = installation / "bin/octave-cli"
        runtime.parent.mkdir(parents=True, exist_ok=True)
        runtime.write_text("#!/bin/sh\n# synthetic runtime; never execute\nexit 0\n")
        runtime.chmod(0o755)
        directory = installation / "share/octave/11.3.0/m"
        directory.mkdir(parents=True, exist_ok=True)
        gnuplot = self.root / "selected-gnuplot"
        gnuplot.write_text("#!/bin/sh\n# synthetic plotting tool; never execute\nexit 0\n")
        gnuplot.chmod(0o755)
        ghostscript = self.root / "selected-ghostscript"
        ghostscript.write_text("#!/bin/sh\n# synthetic export auxiliary; never execute\nexit 0\n")
        ghostscript.chmod(0o755)
        def pin(name):
            return {"path":"/stale-original-build/native/" + name, "sha256":"e" * 64}
        document = {"schema_version":2, "profile":"qualified-core-v2", "startup":{
            "installation_root":str(installation.resolve()), "executable":{"path":str(runtime.resolve()), "sha256":release.sha(runtime)},
            "provider":pin("mathlab_intrinsic_bridge.oct"), "gnuplot":{"path":str(gnuplot.resolve()), "sha256":release.sha(gnuplot)},
            "ghostscript":{"path":str(ghostscript.resolve()), "sha256":release.sha(ghostscript)},
            "runtime_directories":[str(directory.resolve())], "octave_version":"11.3.0", "octave_api_version":"api-v61",
            "adapters":[{"role":role,"file":pin("mathlab_" + role + "_bridge.oct")} for role in ("input","scope","value","debug")]}}
        if graphics:
            document["profile"] = "qualified-graphics-v2"
            document["startup"]["adapters"].append({"role": "graphics", "file": pin(release.GRAPHICS_NAME)})
        path = self.root / "explicit startup Ω.json"
        path.write_text(json.dumps(document))
        return path, document

    def test_graphics6_schema2_selection_and_rebased_package_are_exact(self):
        seed_path, seed = self.startup_seed(graphics=True)
        self.assertEqual(release.read_qualified_startup(seed_path), seed)
        for change in ("core-with-graphics", "missing", "duplicate", "foreign-role", "basename", "schema1", "missing-gs"):
            with self.subTest(change=change):
                altered = copy.deepcopy(seed)
                if change == "core-with-graphics": altered["profile"] = "qualified-core-v2"
                elif change == "missing": altered["startup"]["adapters"].pop()
                elif change == "duplicate": altered["startup"]["adapters"][-1] = copy.deepcopy(altered["startup"]["adapters"][0])
                elif change == "foreign-role": altered["startup"]["adapters"][-1]["role"] = "unknown"
                elif change == "basename": altered["startup"]["adapters"][-1]["file"]["path"] = "/foreign/wrong.oct"
                elif change == "schema1": altered["schema_version"] = 1
                else: del altered["startup"]["ghostscript"]
                seed_path.write_text(json.dumps(altered))
                with patch.object(self, "builder") as builder, self.assertRaises(RuntimeError):
                    self.assemble(startup_config=seed_path)
                builder.assert_not_called()
        seed_path.write_text(json.dumps(seed))
        self.assemble(startup_config=seed_path)
        manifest = self.manifest()
        native = manifest["build"]["native"]
        self.assertEqual(native["adapter_profile"], release.NATIVE_GRAPHICS_PROFILE)
        self.assertEqual(set(native["artifact_sha256"]), set(release.qualified_native_files(release.NATIVE_GRAPHICS_PROFILE)))
        document = json.loads((self.destination / release.STARTUP_CONFIG_PATH).read_text())
        self.assertEqual(document["profile"], "qualified-graphics-v2")
        self.assertEqual(document["startup"]["ghostscript"], seed["startup"]["ghostscript"])
        pins = [document["startup"]["provider"]] + [a["file"] for a in document["startup"]["adapters"]]
        self.assertEqual(len(pins), 6)
        for pin in pins:
            path = self.destination / "Contents/Resources/native" / Path(pin["path"]).name
            self.assertEqual(pin, {"path": str(path.resolve()), "sha256": release.sha(path)})
        release.validate(self.destination)
        def downgrade(repo, native, startup=None):
            identity = self.builder(repo, native, startup)
            identity["native"]["adapter_profile"] = dict(release.NATIVE_QUALIFIED_PROFILE)
            return identity
        with self.assertRaisesRegex(RuntimeError, "requested adapter_profile"):
            self.assemble(startup_config=seed_path, builder=downgrade)
        release.validate(self.destination)

    def test_graphics6_manifest_rejects_resealed_identity_protocol_and_capability_drift(self):
        original = self.qualified_manifest(graphics=True)
        changes = (
            (("protocols", "graphics", "version"), "1"),
            (("protocols", "graphics", "version"), 1.0),
            (("protocols", "graphics", "native_object_lifetime"), 1),
            (("protocols", "graphics", "max_entries"), 4095),
            (("protocols", "graphics", "extra"), True),
            (("worker_observation", "worker_capabilities"), sorted(release.VALUE_REQUIRED_CAPABILITIES)),
            (("worker_observation", "qualified_startup_status", "profile"), "qualified-core-v2"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "graphics", "version"), 1),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "graphics", "path"), "/foreign/mathlab_graphics_bridge.oct"),
            (("worker_observation", "native_modules", release.GRAPHICS_NAME, "sha256"), "b" * 64),
            (("worker_observation", "native_modules", "unexpected.oct"), {"path":"/foreign/unexpected.oct", "sha256":"b"*64}),
            (("artifact_sha256", "ghostscript"), "a" * 64),
            (("adapter_profile", "name"), "qualified-core-v2"),
        )
        for path, value in changes:
            with self.subTest(path=path, value=value):
                changed = copy.deepcopy(original)
                node = changed["build"]["native"]
                for key in path[:-1]: node = node[key]
                node[path[-1]] = value
                self.refresh_metadata(changed)
                with self.assertRaises(RuntimeError): release.validate(self.destination)
        self.refresh_metadata(original)
        module = self.destination / "Contents/Resources/native" / release.GRAPHICS_NAME
        module.unlink()
        self.refresh_metadata(original)
        with self.assertRaises(RuntimeError): release.validate(self.destination)

    def test_schema2_ghostscript_selection_is_strict_and_external(self):
        path, seed = self.startup_seed()
        self.assertEqual(release.read_qualified_startup(path), seed)
        for mutation in ("schema1", "schema1-missing", "missing", "extra", "bad-hash", "relative", "duplicate", "unsafe-path", "unsafe-gnuplot"):
            with self.subTest(mutation=mutation):
                changed = copy.deepcopy(seed)
                if mutation.startswith("schema1"):
                    changed["schema_version"] = 1
                if mutation in ("missing", "schema1-missing"):
                    del changed["startup"]["ghostscript"]
                elif mutation == "extra":
                    changed["startup"]["ghostscript"]["extra"] = True
                elif mutation == "bad-hash":
                    changed["startup"]["ghostscript"]["sha256"] = "A" * 64
                elif mutation == "relative":
                    changed["startup"]["ghostscript"]["path"] = "gs"
                elif mutation == "duplicate":
                    changed["startup"]["ghostscript"] = copy.deepcopy(changed["startup"]["gnuplot"])
                elif mutation.startswith("unsafe-"):
                    key = "gnuplot" if mutation == "unsafe-gnuplot" else "ghostscript"
                    unsafe = self.root / (key + " quoted ' Ω")
                    shutil.copy2(changed["startup"][key]["path"], unsafe)
                    changed["startup"][key]["path"] = str(unsafe)
                path.write_text(json.dumps(changed))
                with self.assertRaises(RuntimeError):
                    release.read_qualified_startup(path)
        path.write_text(json.dumps(seed).replace('"ghostscript":', '"ghostscript": {}, "ghostscript":', 1))
        with self.assertRaises(RuntimeError):
            release.read_qualified_startup(path)
        selected = Path(seed["startup"]["ghostscript"]["path"])
        for mutation in ("missing-file", "nonexecutable", "symlink", "tampered"):
            with self.subTest(mutation=mutation):
                path.write_text(json.dumps(seed))
                original = selected.read_bytes()
                if mutation == "missing-file":
                    selected.unlink()
                elif mutation == "nonexecutable":
                    selected.chmod(0o644)
                elif mutation == "symlink":
                    selected.unlink()
                    selected.symlink_to(seed["startup"]["gnuplot"]["path"])
                else:
                    selected.write_bytes(original + b"# changed\n")
                with self.assertRaises(RuntimeError):
                    release.read_qualified_startup(path)
                if selected.is_symlink():
                    selected.unlink()
                selected.write_bytes(original)
                selected.chmod(0o755)

    def test_explicit_qualified_producer_copies_five_and_resolves_config_for_final_destination(self):
        seed_path, seed = self.startup_seed()
        seed_bytes = seed_path.read_bytes()
        calls = []
        def builder(repo, native, startup=None):
            calls.append({"native":native,"startup":copy.deepcopy(startup)})
            return self.builder(repo, native, startup=startup)
        result = self.assemble(startup_config=seed_path, builder=builder)
        self.assertEqual(calls, [{"native":False,"startup":seed}])
        config = (self.destination / "Contents/Resources/qualified-core-v2.json").resolve()
        self.assertEqual(result["startup_config"], str(config))
        document = json.loads(config.read_text())
        self.assertEqual({key:value for key,value in document["startup"].items() if key not in ("provider","adapters")},
                         {key:value for key,value in seed["startup"].items() if key not in ("provider","adapters")})
        native = self.manifest()["build"]["native"]
        self.assertEqual(native["adapter_profile"], {"schema_version":1,"name":"qualified-core-v2"})
        directory = self.destination / "Contents/Resources/native"
        self.assertEqual({path.name for path in directory.iterdir()}, set(release.QUALIFIED_NATIVE_FILES))
        pins = [document["startup"]["provider"]] + [item["file"] for item in document["startup"]["adapters"]]
        seed_hashes = {Path(pin["path"]).name:pin["sha256"] for pin in
                       [seed["startup"]["provider"]] + [item["file"] for item in seed["startup"]["adapters"]]}
        self.assertEqual({Path(pin["path"]).name for pin in pins}, set(release.QUALIFIED_NATIVE_MODULES))
        for pin in pins:
            path = directory / Path(pin["path"]).name
            self.assertEqual(pin, {"path":str(path.resolve()), "sha256":release.sha(path)})
            self.assertNotEqual(pin["sha256"], seed_hashes[path.name])
            self.assertEqual(pin["sha256"], native["artifact_sha256"][path.name])
            self.assertNotEqual(pin["path"], native["worker_observation"]["native_modules"][path.name]["path"])
        self.assertEqual(seed_path.read_bytes(), seed_bytes)
        launcher = self.destination / "Contents/MacOS/mathlab-launcher"
        source_launcher = self.destination / "Contents/Resources/source/scripts/release-launcher.sh"
        self.assertIn(b"mathlab_packaged_startup=true\n", launcher.read_bytes())
        self.assertIn(release.LAUNCHER_MODE_STAMP, source_launcher.read_bytes())
        self.assertEqual(launcher.read_bytes(), release.installed_launcher_bytes(source_launcher, True))
        payload = {item["path"]: item for item in self.manifest()["payload"]}
        for path in (launcher, config):
            self.assertEqual(payload[path.relative_to(self.destination).as_posix()]["sha256"], release.sha(path))
        release.validate(self.destination)

    def test_qualified_ghostscript_metadata_matches_config_and_rejects_build_drift(self):
        manifest = self.qualified_manifest()
        config = json.loads((self.destination / release.STARTUP_CONFIG_PATH).read_text())
        self.assertEqual(manifest["build"]["native"]["ghostscript"], config["startup"]["ghostscript"])
        self.assertEqual(set(manifest["build"]["native"]["artifact_sha256"]), set(release.QUALIFIED_NATIVE_FILES))
        for changed in (None, {}, {**config["startup"]["ghostscript"], "sha256":"b" * 64},
                        {**config["startup"]["ghostscript"], "extra":True}):
            with self.subTest(metadata=changed):
                invalid = copy.deepcopy(manifest)
                invalid["build"]["native"]["ghostscript"] = changed
                self.refresh_metadata(invalid)
                with self.assertRaisesRegex(RuntimeError, "Ghostscript"):
                    release.validate(self.destination)
        seed_path, seed = self.startup_seed()
        def tampering_builder(repo, native, startup=None):
            identity = self.builder(repo, native, startup)
            Path(startup["startup"]["ghostscript"]["path"]).write_text("changed during build")
            return identity
        with self.assertRaisesRegex(RuntimeError, "Ghostscript"):
            self.assemble(startup_config=seed_path, builder=tampering_builder)

    def test_qualified_resealed_ghostscript_inside_bundle_is_rejected(self):
        manifest = self.qualified_manifest()
        ghostscript = self.destination / "Contents/Resources/export/gs"
        ghostscript.parent.mkdir()
        ghostscript.write_text("Synthetic executable; never run\n")
        ghostscript.chmod(0o755)
        pin = {"path": str(ghostscript.resolve()), "sha256": release.sha(ghostscript)}
        self.assertRegex(pin["path"], r"^[A-Za-z0-9/_+.-]+$")
        config_path = self.destination / release.STARTUP_CONFIG_PATH
        config = json.loads(config_path.read_text())
        config["startup"]["ghostscript"] = dict(pin)
        config_path.write_text(json.dumps(config))
        manifest["build"]["native"]["ghostscript"] = dict(pin)
        self.refresh_metadata(manifest)
        with self.assertRaisesRegex(RuntimeError, "outside the installed bundle"):
            release.validate(self.destination)

    def test_ghostscript_drift_after_validation_preserves_previous_destination(self):
        seed_path, seed = self.startup_seed()
        self.destination.mkdir(parents=True)
        marker = self.destination / "previous-package"
        marker.write_text("retained")
        original_validate = release.validate
        def tampering_validation(bundle):
            original_validate(bundle)
            Path(seed["startup"]["ghostscript"]["path"]).write_text("changed after validation")
        with patch.object(release, "validate", side_effect=tampering_validation):
            with self.assertRaisesRegex(RuntimeError, "Ghostscript"):
                self.assemble(startup_config=seed_path)
        self.assertEqual(marker.read_text(), "retained")

    def test_installed_launcher_stamp_requires_one_unqualified_source_marker(self):
        source = self.root / "launcher-template"
        for data in (b"#!/bin/bash\n", release.LAUNCHER_MODE_STAMP * 2,
                     release.LAUNCHER_MODE_STAMP + b"mathlab_packaged_startup=true\n"):
            with self.subTest(template=data):
                source.write_bytes(data)
                with self.assertRaisesRegex(RuntimeError, "exactly one unqualified"):
                    release.installed_launcher_bytes(source, True)
        source.write_bytes(b"#!/bin/bash\n" + release.LAUNCHER_MODE_STAMP)
        self.assertEqual(release.installed_launcher_bytes(source), source.read_bytes())
        self.assertEqual(release.installed_launcher_bytes(source, True),
                         b"#!/bin/bash\nmathlab_packaged_startup=true\n")

    def test_qualified_launch_policy_and_config_cannot_be_downgraded_with_rehashed_metadata(self):
        original = self.qualified_manifest()
        launcher = self.destination / "Contents/MacOS/mathlab-launcher"
        config = self.destination / release.STARTUP_CONFIG_PATH
        launcher_bytes, config_bytes = launcher.read_bytes(), config.read_bytes()
        cases = ("unstamped", "missing-config", "wrong-native-pin", "wrong-runtime-pin")
        for change in cases:
            with self.subTest(change=change):
                launcher.write_bytes(launcher_bytes)
                config.write_bytes(config_bytes)
                if change == "unstamped":
                    launcher.write_bytes(launcher_bytes.replace(b"mathlab_packaged_startup=true\n", release.LAUNCHER_MODE_STAMP))
                elif change == "missing-config":
                    config.unlink()
                else:
                    document = json.loads(config_bytes)
                    field = "provider" if change == "wrong-native-pin" else "executable"
                    document["startup"][field]["sha256"] = "f" * 64
                    config.write_text(json.dumps(document))
                self.refresh_metadata(copy.deepcopy(original))
                with self.assertRaises(RuntimeError):
                    release.validate(self.destination)

    def test_nonqualified_profiles_keep_unstamped_launcher_and_reject_packaged_policy(self):
        for with_debugger in (False, True):
            with self.subTest(with_debugger=with_debugger):
                self.assemble(with_debugger=with_debugger)
                original = self.manifest()
                launcher = self.destination / "Contents/MacOS/mathlab-launcher"
                source = self.destination / "Contents/Resources/source/scripts/release-launcher.sh"
                self.assertEqual(launcher.read_bytes(), source.read_bytes())
                self.assertFalse((self.destination / release.STARTUP_CONFIG_PATH).exists())
                launcher.write_bytes(release.installed_launcher_bytes(source, True))
                self.refresh_metadata(original)
                with self.assertRaisesRegex(RuntimeError, "launcher policy"):
                    release.validate(self.destination)

    def test_historical_nonqualified_launcher_without_stamp_matches_source_exactly(self):
        for profile in ("legacy", "omitted", "four-adapter"):
            with self.subTest(profile=profile):
                self.assemble(with_debugger=profile == "four-adapter")
                manifest = self.manifest()
                if profile == "legacy":
                    del manifest["build"]["native"]["adapter_profile"]
                    del manifest["build"]["native"]["value_component"]
                source_root = self.destination / "Contents/Resources/source"
                source = source_root / "scripts/release-launcher.sh"
                installed = self.destination / "Contents/MacOS/mathlab-launcher"
                historical = b'#!/bin/bash\n# Historical synthetic entry point, never executed.\nexit 0\n'
                source.write_bytes(historical)
                installed.write_bytes(historical)
                manifest["source"]["files"] = release.inventory(source_root)
                manifest["source"]["export_sha256"] = release.hashlib.sha256(
                    json.dumps(manifest["source"]["files"], sort_keys=True).encode()).hexdigest()
                self.refresh_metadata(manifest)
                release.validate(self.destination)
                installed.write_bytes(historical + b"# mismatched installed entry\n")
                self.refresh_metadata(manifest)
                with self.assertRaisesRegex(RuntimeError, "launcher policy"):
                    release.validate(self.destination)

    def test_explicit_qualified_selector_rejects_bad_seed_or_downgraded_builder_without_fallback(self):
        seed_path, seed = self.startup_seed()
        self.destination.mkdir(parents=True)
        old = self.destination / "old"
        old.write_text("still usable")
        calls = []
        def downgraded(repo, native, startup=None):
            calls.append(startup)
            return self.builder(repo, True)
        with self.assertRaisesRegex(RuntimeError, "adapter_profile"):
            self.assemble(startup_config=seed_path, builder=downgraded)
        self.assertEqual(calls, [seed])
        self.assertEqual(old.read_text(), "still usable")
        for change in ("wrong-runtime-pin", "conflicting-selector"):
            with self.subTest(change=change):
                changed = copy.deepcopy(seed)
                if change == "wrong-runtime-pin":
                    changed["startup"]["executable"]["sha256"] = "b" * 64
                seed_path.write_text(json.dumps(changed))
                with patch.object(self, "builder") as builder:
                    with self.assertRaises(RuntimeError):
                        self.assemble(startup_config=seed_path, with_debugger=change == "conflicting-selector")
                builder.assert_not_called()
                self.assertEqual(old.read_text(), "still usable")
        for selection in ("gnuplot", "ghostscript", "runtime-directory", "executable"):
            with self.subTest(overlapping_selection=selection):
                changed = copy.deepcopy(seed)
                if selection == "runtime-directory":
                    selected = self.destination / "selected-runtime-directory"
                    shutil.copytree(seed["startup"]["runtime_directories"][0], selected)
                    changed["startup"]["installation_root"] = str(self.root.resolve())
                    changed["startup"]["runtime_directories"] = [str(selected.resolve())]
                else:
                    selected = self.destination / ("selected-" + selection)
                    shutil.copy2(seed["startup"][selection]["path"], selected)
                    changed["startup"][selection] = {"path":str(selected.resolve()), "sha256":release.sha(selected)}
                    if selection == "executable":
                        changed["startup"]["installation_root"] = str(self.root.resolve())
                seed_path.write_text(json.dumps(changed))
                self.assertEqual(release.read_qualified_startup(seed_path), changed)
                before = release.inventory(self.destination)
                with patch.object(self, "builder") as builder, patch.object(release, "source_snapshot") as snapshot:
                    with self.assertRaisesRegex(RuntimeError, "overlaps the package destination"):
                        self.assemble(startup_config=seed_path)
                builder.assert_not_called()
                snapshot.assert_not_called()
                self.assertEqual(release.inventory(self.destination), before)
                self.assertEqual(old.read_text(), "still usable")
        self.assertEqual(list(self.destination.parent.glob(".opennumlab-stage-*")), [])

    def test_qualified_core_manifest_requires_identified_accepted_ready(self):
        self.qualified_manifest()
        release.validate(self.destination)

    def test_qualified_core_rejects_missing_foreign_downgraded_and_extra_status(self):
        original = self.qualified_manifest()
        changes = [
            (("worker_observation", "qualified_startup_status"), None),
            (("worker_observation", "native_modules"), {}),
            (("worker_observation", "native_modules", "mathlab_intrinsic_bridge.oct", "sha256"), "b" * 64),
            (("worker_observation", "native_modules", "mathlab_intrinsic_bridge.oct", "extra"), True),
            (("worker_observation", "native_modules", "foreign.oct"), {}),
            (("worker_observation", "qualified_startup_status", "profile"), "four-adapter-value-v1"),
            (("worker_observation", "qualified_startup_status", "extra"), True),
            (("worker_observation", "qualified_startup_status", "intrinsics", "extra"), True),
            (("worker_observation", "qualified_startup_status", "intrinsics", "version"), 1),
            (("worker_observation", "qualified_startup_status", "intrinsics", "phase"), "configured"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "provider_file"), "/foreign/mathlab_intrinsic_bridge.oct"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "octave_api_version"), "api-v60"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "initial_path_admitted"), 1),
            (("worker_observation", "qualified_startup_status", "intrinsics", "shutdown_registered"), False),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "debug", "path"), "/foreign/mathlab_debug_bridge.oct"),
            (("worker_observation", "qualified_startup_status", "intrinsics", "adapters", "graphics"), {}),
            (("worker_observation", "session_context", "invocation"), "2"),
            (("worker_observation", "qualified_startup_status", "context", "generation"), "01"),
            (("protocols", "intrinsic", "version"), "1"),
            (("protocols", "intrinsic", "extra"), True),
            (("sdk", "OCTAVE_API_VERSION"), "api-v60"),
            (("adapter_profile", "name"), "four-adapter-value-v1"),
        ]
        for path, value in changes:
            with self.subTest(path=path):
                manifest = copy.deepcopy(original)
                current = manifest["build"]["native"]
                for key in path[:-1]:
                    current = current[key]
                current[path[-1]] = value
                if path == ("worker_observation", "qualified_startup_status", "context", "generation"):
                    manifest["build"]["native"]["worker_observation"]["session_context"]["generation"] = value
                self.refresh_metadata(manifest)
                with self.assertRaises(RuntimeError):
                    release.validate(self.destination)

    def test_qualified_core_rejects_missing_or_extra_provider_payload(self):
        original = self.qualified_manifest()
        provider = self.destination / "Contents/Resources/native/mathlab_intrinsic_bridge.oct"
        content = provider.read_bytes()
        provider.unlink()
        self.refresh_metadata(copy.deepcopy(original))
        with self.assertRaises(RuntimeError):
            release.validate(self.destination)
        provider.write_bytes(content)
        extra = self.destination / "Contents/Resources/native/foreign.oct"
        extra.write_bytes(content)
        self.refresh_metadata(copy.deepcopy(original))
        with self.assertRaises(RuntimeError):
            release.validate(self.destination)

    def test_nonqualified_manifest_variants_reject_intrinsic_status_declarations(self):
        for profile in ("native-omitted-v1", "four-adapter-value-v1", None):
            self.assemble(with_debugger=profile == "four-adapter-value-v1")
            original = self.manifest()
            if profile is None:
                original["build"]["native"].pop("adapter_profile")
                original["build"]["native"].pop("value_component")
            for key, value in (("qualified_startup_status", {"profile": "qualified-core-v2"}),
                               ("intrinsics", {"phase": "ready"}),
                               ("native_modules", {"mathlab_intrinsic_bridge.oct": {}})):
                with self.subTest(profile=profile, claim=key):
                    manifest = copy.deepcopy(original)
                    manifest["build"]["native"][key] = value
                    self.refresh_metadata(manifest)
                    with self.assertRaises(RuntimeError):
                        release.validate(self.destination)

    def test_legacy_manifest_cannot_declare_an_unclaimed_intrinsic_provider(self):
        self.assemble()
        manifest = self.manifest()
        native = manifest["build"]["native"]
        native.pop("adapter_profile")
        native.pop("value_component")
        provider = self.destination / "Contents/Resources/foreign/mathlab_intrinsic_bridge.oct"
        provider.parent.mkdir()
        provider.write_text("unclaimed provider")
        self.refresh_metadata(manifest)
        with self.assertRaises(RuntimeError):
            release.validate(self.destination)

    def test_duplicate_manifest_fields_are_rejected_before_validation(self):
        self.assemble()
        path = self.destination / "Contents/Resources/release.json"
        text = path.read_text().replace('"schema_version": 1,', '"schema_version": 2, "schema_version": 1,', 1)
        path.write_text(text)
        with self.assertRaisesRegex(RuntimeError, "Duplicate JSON field"):
            release.validate(self.destination)

    def test_qualified_preflight_queries_sdk_api_config_not_header_macro(self):
        _, seed = self.startup_seed()
        runtime = Path(seed["startup"]["executable"]["path"])
        runtime.write_text("#!/bin/sh\nprintf '11.3.0'\n")
        seed["startup"]["executable"]["sha256"] = release.sha(runtime)
        header = runtime.parent.parent / "include/octave/version.h"
        header.parent.mkdir(parents=True)
        header.write_text('#define OCTAVE_VERSION "11.3.0"\n#define OCTAVE_API_VERSION "api-v61"\n')
        mkoctfile = runtime.with_name("mkoctfile")
        mkoctfile.write_text(
            "#!" + sys.executable + "\n"
            "import pathlib, re, sys\n"
            "header = pathlib.Path(__file__).parent.parent / 'include/octave/version.h'\n"
            "macros = dict(re.findall(r'^#define (\\w+) \\\"([^\\\"]*)\\\"$', header.read_text(), re.M))\n"
            "values = {'OCTAVE_VERSION': macros['OCTAVE_VERSION'], 'API_VERSION': macros['OCTAVE_API_VERSION']}\n"
            "if len(sys.argv) != 3 or sys.argv[1] != '-p' or sys.argv[2] not in values:\n"
            "    print('mkoctfile: unknown variable ' + repr(sys.argv[-1]), file=sys.stderr)\n"
            "    sys.exit(1)\n"
            "print(values[sys.argv[2]])\n")
        mkoctfile.chmod(0o755)
        rejected = subprocess.run([mkoctfile, "-p", "OCTAVE_API_VERSION"], capture_output=True, text=True, timeout=5)
        self.assertEqual(rejected.returncode, 1)
        self.assertIn("unknown variable", rejected.stderr)
        self.assertTrue(header.is_file())
        def synthetic_run(argv, cwd, **_kwargs):
            # Keep these tiny fake utilities in the synthetic test's owned group.
            return subprocess.check_output(argv, cwd=cwd, stderr=subprocess.STDOUT, text=True, timeout=5).strip()
        with patch.dict(os.environ, MATHLAB_MKOCTFILE=str(mkoctfile)), \
                patch.object(release, "run", side_effect=synthetic_run):
            tools = release.native_preflight(self.repo, seed)
            self.assertEqual(tools["version"], "11.3.0")
            self.assertEqual(tools["runtime_executable_sha256"], seed["startup"]["executable"]["sha256"])
            for api in ("api-v60", "", "v61"):
                with self.subTest(api=api):
                    header.write_text('#define OCTAVE_VERSION "11.3.0"\n#define OCTAVE_API_VERSION "' + api + '"\n')
                    with self.assertRaisesRegex(RuntimeError, "SDK must match api-v61"):
                        release.native_preflight(self.repo, seed)
            header.write_text('#define OCTAVE_VERSION "8.4.0"\n#define OCTAVE_API_VERSION "api-v61"\n')
            with self.assertRaisesRegex(RuntimeError, "runtime/SDK mismatch"):
                release.native_preflight(self.repo, seed)
            mkoctfile.write_text("#!/bin/sh\nif [ \"$2\" = OCTAVE_VERSION ]; then printf '11.3.0'; else exit 1; fi\n")
            with self.assertRaises(subprocess.CalledProcessError) as unavailable:
                release.native_preflight(self.repo, seed)
            self.assertEqual(unavailable.exception.cmd[-1], "API_VERSION")

    def test_native_protocol_probe_uses_value_specific_arity_and_validates_limits(self):
        octave = self.root / "selected-octave"
        octave.write_text("synthetic executable")
        tools = {"octave": str(octave), "mkoctfile": "/synthetic/mkoctfile", "version": "11.3.0",
                 "runtime_executable_sha256": release.sha(octave)}
        sdk_api = "api-v61"
        def synthetic_run(argv, *_args, **_kwargs):
            argv = [str(item) for item in argv]
            if argv[:2] == ["bash", "scripts/build-debugger.sh"]:
                qualified = "--profile" in argv
                self.assertEqual(argv, ["bash", "scripts/build-debugger.sh"] + (["--profile", "qualified-core-v2"] if qualified else []))
                for name in release.QUALIFIED_NATIVE_FILES if qualified else release.NATIVE_FILES:
                    (self.repo / "native" / name).write_text("11.3.0\n" if name == "runtime-version.txt" else "fresh " + name)
                return ""
            if argv[0] == str(octave):
                expression = argv[-1]
                if "mathlab_graphics_bridge" in expression:
                    self.assertIn("('protocol',0)", expression)
                    return json.dumps(release.GRAPHICS_PROTOCOL)
                if "mathlab_intrinsic_bridge" in expression:
                    self.assertIn("('intrinsic_protocol')", expression)
                    self.assertNotIn("mathlab_intrinsic_bridge('protocol')", expression)
                    return json.dumps(release.INTRINSIC_PROTOCOL)
                for short, protocol, version in (("debug", "mathlab-debugger", 4), ("scope", "mathlab-scope", 2), ("input", "mathlab-input", 1)):
                    if "mathlab_" + short + "_bridge" in expression:
                        self.assertIn("('protocol',0)", expression)
                        return json.dumps({"protocol": protocol, "version": version})
                self.assertIn("mathlab_value_bridge('protocol')", expression)
                self.assertNotIn("mathlab_value_bridge('protocol',0)", expression)
                return json.dumps(release.VALUE_PROTOCOL)
            if argv[0] == tools["mkoctfile"]:
                if "-o" in argv:
                    self.assertEqual(argv, [tools["mkoctfile"], "-DMATHLAB_QUALIFIED_CORE_V2=1", "-Wall", "-Wextra", "-Werror", "-std=c++17",
                        "-I" + str(self.repo / "native"), str(self.repo / "native/mathlab_graphics_bridge.cc"),
                        "-o", str(self.repo / "native" / release.GRAPHICS_NAME)])
                    (self.repo / "native" / release.GRAPHICS_NAME).write_text("fresh graphics")
                    return ""
                if argv[-1] == "OCTAVE_API_VERSION":
                    raise subprocess.CalledProcessError(1, argv, output="unknown variable")
                return {"OCTAVE_VERSION": "11.3.0", "API_VERSION":sdk_api, "CXX": "synthetic-cxx"}.get(argv[-1], "synthetic config")
            self.assertEqual(argv, ["synthetic-cxx", "--version"])
            return "synthetic compiler"
        with patch.object(release, "run", side_effect=synthetic_run) as runner:
            identity = release.native_build(self.repo, tools)
        self.assertEqual(identity["protocols"]["value"], release.VALUE_PROTOCOL)
        self.assertEqual(sum("--eval" in call.args[0] for call in runner.call_args_list), 4)
        with self.assertRaisesRegex(RuntimeError, "preexisting adapters"):
            release.native_build(self.repo, tools)
        for name in release.NATIVE_FILES:
            (self.repo / "native" / name).unlink()
        with patch.object(release, "run", side_effect=synthetic_run) as runner:
            identity = release.native_build(self.repo, tools, qualified=True)
        self.assertEqual(identity["protocols"]["intrinsic"], release.INTRINSIC_PROTOCOL)
        self.assertEqual(identity["sdk"]["OCTAVE_API_VERSION"], "api-v61")
        self.assertEqual(sum("--eval" in call.args[0] for call in runner.call_args_list), 5)
        self.assertIn([tools["mkoctfile"], "-p", "API_VERSION"], [call.args[0] for call in runner.call_args_list])
        for name in release.QUALIFIED_NATIVE_FILES:
            (self.repo / "native" / name).unlink()
        leftover = self.repo / "native" / release.GRAPHICS_NAME
        leftover.write_text("preexisting graphics only")
        with patch.object(release, "run") as forbidden:
            with self.assertRaisesRegex(RuntimeError, "preexisting adapters"):
                release.native_build(self.repo, tools, qualified=True, graphics=True)
        forbidden.assert_not_called()
        leftover.unlink()
        with patch.object(release, "run", side_effect=synthetic_run) as runner:
            identity = release.native_build(self.repo, tools, qualified=True, graphics=True)
        self.assertEqual(identity["adapter_profile"], release.NATIVE_GRAPHICS_PROFILE)
        self.assertEqual(identity["protocols"]["graphics"], release.GRAPHICS_PROTOCOL)
        self.assertEqual(sum("--eval" in call.args[0] for call in runner.call_args_list), 6)
        with self.assertRaisesRegex(RuntimeError, "preexisting adapters"):
            release.native_build(self.repo, tools, qualified=True, graphics=True)
        for name in release.qualified_native_files(release.NATIVE_GRAPHICS_PROFILE):
            (self.repo / "native" / name).unlink()
        sdk_api = "api-v60"
        with patch.object(release, "run", side_effect=synthetic_run):
            with self.assertRaisesRegex(RuntimeError, "SDK changed from api-v61"):
                release.native_build(self.repo, tools, qualified=True)

    def test_native_prerequisite_failure_precedes_cargo(self):
        with patch.object(release, "native_preflight", side_effect=RuntimeError("runtime/SDK mismatch")), \
                patch.object(release, "run") as runner:
            with self.assertRaisesRegex(RuntimeError, "runtime/SDK mismatch"):
                release.build(self.repo, True)
        runner.assert_not_called()

    def test_qualified_build_routes_owned_fresh_config_to_observer_and_removes_it(self):
        self.qualified_build_route()

    def test_graphics6_build_routes_six_native_pins_and_graphics_selection(self):
        self.qualified_build_route(graphics=True)

    def qualified_build_route(self, graphics=False):
        _, seed = self.startup_seed(graphics=graphics)
        profile = release.NATIVE_GRAPHICS_PROFILE if graphics else release.NATIVE_QUALIFIED_PROFILE
        original = copy.deepcopy(seed)
        seed_hashes = {Path(pin["path"]).name:pin["sha256"] for pin in
                       [seed["startup"]["provider"]] + [item["file"] for item in seed["startup"]["adapters"]]}
        runtime = seed["startup"]["executable"]
        tools = {"octave":runtime["path"], "mkoctfile":"/synthetic/mkoctfile", "version":"11.3.0",
                 "runtime_executable_sha256":runtime["sha256"]}
        expected_observation = {}
        selected_config = []
        linked_paths = []
        version = "2.7.3-alpha.4"
        cargo_build = ["cargo", "build", "--release", "--locked", "--message-format=json-render-diagnostics",
                       "-p", "mathlab-desktop", "-p", "mathlab-cli"]
        cargo_metadata = ["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"]
        native_paths = {(self.repo / "native" / name).resolve() for name in release.qualified_native_modules(profile)}

        def synthetic_native(repo, selected, qualified=False, **kwargs):
            self.assertEqual(kwargs, {"graphics": True} if graphics else {})
            self.assertEqual((repo, selected, qualified), (self.repo, tools, True))
            fixture = self.builder(repo, False, startup=seed)["native"]
            expected_observation.update(copy.deepcopy(fixture["worker_observation"]))
            # Real native_build returns protocol/SDK evidence; build must supply
            # fresh byte hashes, worker observation and value declaration itself.
            for key in ("artifact_sha256", "worker_observation", "value_component"):
                fixture.pop(key)
            return fixture

        def synthetic_run(argv, cwd, timeout=120, env=None):
            command = [str(item) for item in argv]
            self.assertEqual(cwd, self.repo)
            if command == cargo_build:
                return "\n".join(json.dumps({"reason":"compiler-artifact", "target":{"name":name},
                                             "executable":str(self.executable)}) for name in ("mathlab-desktop", "mathlab"))
            if command == cargo_metadata:
                return json.dumps({"packages":[{"name":name,"version":version} for name in ("mathlab-desktop","mathlab-cli")]})
            if command == [str(self.executable), "--version"]:
                return "mathlab " + version
            if len(command) == 3 and command[:2] == ["otool", "-L"]:
                path = Path(command[2]).resolve()
                self.assertIn(path, native_paths | {self.executable.resolve()})
                linked_paths.append(path)
                return "synthetic linked dependencies for " + path.name
            metadata = {("rustc", "-vV"):"synthetic rustc", ("cargo", "--version"):"synthetic cargo",
                        ("xcrun", "--show-sdk-version"):"synthetic SDK", ("sw_vers", "-productVersion"):"synthetic OS",
                        ("uname", "-m"):"synthetic architecture"}
            if tuple(command) in metadata:
                return metadata[tuple(command)]
            raise AssertionError("Unexpected producer command: " + repr(command))

        def observe(repo, cli, selected, native, startup_config=None):
            self.assertEqual((repo, cli, selected), (self.repo, self.executable, tools))
            self.assertIsNotNone(startup_config)
            path = Path(startup_config)
            self.assertTrue(path.is_file())
            self.assertFalse(path.resolve().is_relative_to(self.repo.resolve()))
            document = json.loads(path.read_text())
            self.assertEqual({key:value for key,value in document["startup"].items() if key not in ("provider","adapters")},
                             {key:value for key,value in seed["startup"].items() if key not in ("provider","adapters")})
            self.assertEqual(set(native["artifact_sha256"]), set(release.qualified_native_files(profile)))
            pins = [document["startup"]["provider"]] + [item["file"] for item in document["startup"]["adapters"]]
            self.assertEqual({Path(pin["path"]).name for pin in pins}, set(release.qualified_native_modules(profile)))
            for pin in pins:
                built = (repo / "native" / Path(pin["path"]).name).resolve()
                self.assertEqual(pin, {"path":str(built),"sha256":release.sha(built)})
                self.assertNotEqual(pin["sha256"], seed_hashes[built.name])
                self.assertEqual(pin["sha256"], native["artifact_sha256"][built.name])
            selected_config.append(path)
            return copy.deepcopy(expected_observation)

        with patch.object(release, "native_preflight", return_value=tools) as preflight, \
                patch.object(release, "native_build", side_effect=synthetic_native) as native_builder, \
                patch.object(release, "run", side_effect=synthetic_run), \
                patch.object(release, "observe_value_capability", side_effect=observe) as observer:
            identity = release.build(self.repo, False, startup=seed)
        preflight.assert_called_once_with(self.repo, seed)
        native_builder.assert_called_once_with(self.repo, tools, qualified=True, **({"graphics": True} if graphics else {}))
        observer.assert_called_once()
        self.assertEqual(len(selected_config), 1)
        self.assertFalse(selected_config[0].exists())
        self.assertEqual(identity["native"]["worker_observation"], expected_observation)
        self.assertEqual(identity["native"]["artifact_sha256"]["mathlab_intrinsic_bridge.oct"],
                         release.sha(self.repo / "native/mathlab_intrinsic_bridge.oct"))
        self.assertEqual(set(linked_paths), native_paths | {self.executable.resolve()})
        self.assertEqual(len(linked_paths), 8 if graphics else 7)
        self.assertIn("mathlab_intrinsic_bridge.oct", identity["linked_libraries"])
        self.assertEqual(seed, original)

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

    def test_qualified_doctor_routes_config_and_requires_all_fresh_module_pins(self):
        self.qualified_doctor_contract()

    def test_graphics6_doctor_requires_selected_six_and_live_capability(self):
        self.qualified_doctor_contract(graphics=True)

    def qualified_doctor_contract(self, graphics=False):
        native = self.qualified_manifest(graphics=graphics)["build"]["native"]
        provider = self.repo / "native/mathlab_intrinsic_bridge.oct"
        for name in release.qualified_native_modules(native["adapter_profile"]):
            shutil.copyfile(self.destination / "Contents/Resources/native" / name, self.repo / "native" / name)
        native["runtime_executable_sha256"] = release.sha(self.executable)
        doctor = copy.deepcopy(native["worker_observation"])
        doctor.update(runtime=str(self.executable), ready=True,
                      runtime_executable_sha256=native["runtime_executable_sha256"])
        modules = doctor["native_modules"]
        for name in modules:
            modules[name]["path"] = str((self.repo / "native" / name).resolve())
        doctor["value_adapter"] = modules["mathlab_value_bridge.oct"]["path"]
        ready = doctor["qualified_startup_status"]["intrinsics"]
        ready["provider_file"] = modules[provider.name]["path"]
        for role, adapter in ready["adapters"].items():
            adapter["path"] = modules["mathlab_" + role + "_bridge.oct"]["path"]
        config = self.root / "explicit-startup.json"
        _, seed = self.startup_seed(graphics=graphics)
        seed["startup"]["installation_root"] = str(self.root.resolve())
        seed["startup"]["executable"] = {"path":str(self.executable.resolve()), "sha256":release.sha(self.executable)}
        config.write_text(json.dumps(release.qualified_startup_document(seed, self.repo / "native", native["artifact_sha256"])))
        tools = {"octave": str(self.executable)}
        stale = {name: "/foreign/stale" for name in ("MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE",
                 "MATHLAB_DEBUG_BRIDGE", "MATHLAB_SCOPE_BRIDGE", "MATHLAB_INPUT_BRIDGE", "MATHLAB_VALUE_BRIDGE")}
        with patch.dict(os.environ, stale), patch.object(release, "run", return_value=json.dumps(doctor)) as runner:
            observed = release.observe_value_capability(self.repo, self.executable, tools, native, config)
        command = runner.call_args.args[0]
        self.assertEqual(command[2:4], ["--startup-config", str(config.resolve())])
        self.assertNotIn("--octave", command)
        self.assertTrue(set(stale).isdisjoint(runner.call_args.kwargs["env"]))
        self.assertEqual(observed["qualified_startup_status"], doctor["qualified_startup_status"])
        variants = []
        for field in ("native_modules", "qualified_startup_status", "session_context"):
            changed = copy.deepcopy(doctor)
            changed.pop(field)
            variants.append(changed)
        foreign = copy.deepcopy(doctor)
        foreign["native_modules"][provider.name]["path"] = "/foreign/" + provider.name
        foreign["qualified_startup_status"]["intrinsics"]["provider_file"] = "/foreign/" + provider.name
        variants.append(foreign)
        bad_hash = copy.deepcopy(doctor)
        bad_hash["native_modules"][provider.name]["sha256"] = "0" * 64
        variants.append(bad_hash)
        if graphics:
            for field in ("sha256", "path"):
                changed = copy.deepcopy(doctor)
                changed["native_modules"][release.GRAPHICS_NAME][field] = "b" * 64 if field == "sha256" else "/foreign/" + release.GRAPHICS_NAME
                variants.append(changed)
            missing_capability = copy.deepcopy(doctor)
            missing_capability["worker_capabilities"].remove(release.GRAPHICS_CAPABILITY)
            variants.append(missing_capability)
        for index, changed in enumerate(variants):
            with self.subTest(index=index), patch.object(release, "run", return_value=json.dumps(changed)):
                with self.assertRaises(RuntimeError):
                    release.observe_value_capability(self.repo, self.executable, tools, native, config)
        ghostscript = Path(native["ghostscript"]["path"])
        original_ghostscript = ghostscript.read_bytes()
        def tamper_ghostscript(*_args, **_kwargs):
            ghostscript.write_bytes(original_ghostscript + b"# changed during doctor\n")
            return json.dumps(doctor)
        with patch.object(release, "run", side_effect=tamper_ghostscript):
            with self.assertRaisesRegex(RuntimeError, "Ghostscript"):
                release.observe_value_capability(self.repo, self.executable, tools, native, config)
        ghostscript.write_bytes(original_ghostscript)
        with patch.object(release, "run", return_value=json.dumps(doctor)):
            provider.write_text("changed provider after observation")
            with self.assertRaisesRegex(RuntimeError, "freshly built native module"):
                release.observe_value_capability(self.repo, self.executable, tools, native, config)

    def test_doctor_rejects_implicit_qualified_or_configured_legacy_selection(self):
        with patch.object(release, "run") as runner:
            for profile, config in ((release.NATIVE_QUALIFIED_PROFILE, None),
                                    (release.NATIVE_REQUIRED_PROFILE, self.root / "explicit.json")):
                with self.subTest(profile=profile), self.assertRaisesRegex(RuntimeError, "explicit startup configuration"):
                    release.observe_value_capability(self.repo, self.executable, {}, {"adapter_profile": profile}, config)
        runner.assert_not_called()

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
        config = self.root / "explicit startup Ω.json"
        config.write_text("{}\n")
        for index, args in enumerate((("--startup-config", str(config)), ("--startup-config=" + str(config),))):
            startup_project = self.root / ("startup-only project " + str(index))
            values = self.launch(args=args, project=startup_project)
            self.assertEqual(values, [str(startup_project), "--cwd", str(startup_project), "--open",
                                      str(startup_project / "getting_started.m"), *args])

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
