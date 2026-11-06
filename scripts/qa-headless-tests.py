#!/usr/bin/env python3
"""Synthetic headless QA contracts; no Rust build or interpreter is invoked.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("headless_qa", SCRIPTS / "qa-headless.py")
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class HeadlessQAContracts(unittest.TestCase):
    def pair(self):
        common = {"schema_version": 4, "profile": "class-v1", "backend": "rust", "source_graph_complete": True,
                  "key": {"session_id": 1, "epoch": 1, "request_id": 1}, "source_graph_sha256": "a" * 64}
        return [dict(copy.deepcopy(common), event="ready"), dict(copy.deepcopy(common), event="outcome")]

    def test_exact_schema_and_key_pair_reject_stale_profile_and_extra_row(self):
        original = self.pair()
        self.assertIsNot(original[0]["key"], original[1]["key"])
        self.assertEqual(qa.rows("\n".join(json.dumps(row) for row in original)), original)
        for mutation in ("key", "profile", "extra"):
            value = copy.deepcopy(original)
            if mutation == "key":
                value[1]["key"]["request_id"] = 2
                self.assertEqual(value[0]["key"]["request_id"], 1, "Mutating outcome must not alter ready fixture")
                self.assertNotEqual(value[0]["key"], value[1]["key"])
            if mutation == "profile": value[1]["profile"] = "scientific-v2"
            if mutation == "extra": value.append(value[1])
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                qa.rows("\n".join(json.dumps(row) for row in value))

    def test_numeric_checks_lossless_components_shape_and_completeness(self):
        word = lambda value: struct.pack(">d", value).hex()
        value = {"storage": "dense", "shape": [1, 1], "window_offset": 0, "payload_complete": True,
                 "binary64_components": [[word(2.0), word(-3.0)]]}
        qa.numeric(value, [1, 1], [(2, -3)])
        for change in ({"payload_complete": False}, {"shape": [1, 2]}, {"binary64_components": [[word(2.0)]]}):
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                qa.numeric(dict(value, **change), [1, 1], [(2, -3)])

    def test_health_refuses_poison_fuel_and_incomplete_events(self):
        original = {"events_complete": True, "stats": {"ledger_poisoned": False, "primitive_steps": 1,
            "transitions": 1, "peak_live_payload_bytes": 8, "peak_metadata_bytes": 8}}
        qa.healthy(original)
        for key, value in (("ledger_poisoned", True), ("primitive_steps", 1_000_000_001), ("peak_metadata_bytes", 17 << 20)):
            bad = copy.deepcopy(original)
            bad["stats"][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError): qa.healthy(bad)

    def test_real_ready_observer_is_bounded_one_shot_and_preserves_entry_edit(self):
        with tempfile.TemporaryDirectory() as temp:
            entry = Path(temp).resolve() / "entry.m"
            entry.write_text("original")
            observer = qa.ReadyObservation(entry, replacement="new unexecuted text", stop=True)
            process = type("OwnedFixture", (), {"pid": 12345, "poll": lambda self: None})()
            encoded = (json.dumps(self.pair()[0]) + "\n").encode()
            with patch.object(qa.os, "kill") as stop:
                observer(process, encoded[:5])
                stop.assert_not_called()
                observer(process, encoded[5:])
                observer(process, b"later outcome is not another signal")
                stop.assert_called_once_with(12345, qa.signal.SIGINT)
            self.assertEqual(entry.read_text(), "new unexecuted text")
            with self.assertRaises(RuntimeError):
                qa.ReadyObservation(entry)(process, b"x" * 32769)

    def test_runner_observer_uses_owned_python_signal_fixture_and_cleans_group(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            entry = root / "entry.m"
            entry.write_text("fixture")
            runner = qa.installed.Runner(root, root, {"PATH": os.defpath}, 5, output_limit=4096)
            code = ("import json,signal,sys;signal.signal(signal.SIGINT,lambda *_:sys.exit(130));"
                    "print(json.dumps({'event':'ready','schema_version':4}),flush=True);signal.pause()")
            receipt, stdout, stderr = runner.run("synthetic-ready-signal", [sys.executable, "-u", "-c", code],
                                                 on_stdout=qa.ReadyObservation(entry, stop=True))
            self.assertEqual(receipt["exit_code"], 130)
            self.assertFalse(receipt["timed_out"])
            self.assertEqual(receipt["cleanup_status"], "passed")
            self.assertEqual(stderr, "")
            self.assertEqual(json.loads(stdout)["event"], "ready")

    def test_fresh_output_and_failed_inspection_keep_evidence_without_runtime(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            output = root / "evidence"
            with patch.object(qa.release, "validate_headless", side_effect=RuntimeError("synthetic malformed package")):
                report = qa.accept(root / "bundle", output, 5)
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["commands"], [])
            self.assertEqual(json.loads((output / "acceptance.json").read_text())["error"]["message"], "synthetic malformed package")
            with self.assertRaisesRegex(RuntimeError, "fresh owned"):
                qa.accept(root / "bundle", output, 5)

    def test_qa_output_inside_package_is_refused_without_deleting_user_files(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp).resolve() / "bundle"
            bundle.mkdir()
            marker = bundle / "user-file"
            marker.write_text("KEEP")
            with self.assertRaisesRegex(RuntimeError, "overlap"):
                qa.accept(bundle, bundle / "evidence", 5)
            self.assertEqual(marker.read_text(), "KEEP")


    def test_indirect_output_ancestor_alias_inside_bundle_is_refused_before_any_write(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            bundle = root / "bundle"
            nested = bundle / "existing-subdir"
            nested.mkdir(parents=True)
            (bundle / "user-file").write_text("KEEP ENTIRE PACKAGE")
            (nested / "retained").write_bytes(b"original")
            alias = root / "alias"
            alias.symlink_to(bundle, target_is_directory=True)
            before = qa.release.inventory(bundle)
            with patch.object(qa.release, "validate_headless", side_effect=AssertionError("Refuse before inspection or writes")):
                with self.assertRaisesRegex(RuntimeError, "canonical"):
                    qa.accept(bundle, alias / "existing-subdir/new-evidence", 5)
            self.assertEqual(qa.release.inventory(bundle), before)
            self.assertFalse((nested / "new-evidence").exists())


    def test_expected_negative_exit_never_hides_runner_reason_or_incomplete_cleanup(self):
        original = {"exit_code": 130, "timed_out": False, "output_limit_exceeded": False,
                    "cleanup_errors": [], "cleanup_status": "passed", "status": "failed"}
        command = (dict(original), "owned CLI output", "")
        self.assertEqual(qa.checked_command(command, 130), ("owned CLI output", ""))
        self.assertEqual(command[0]["expectation_status"], "passed")
        for change in ({"reason": "synthetic stdout read error"}, {"cleanup_status": "not_run"},
                       {"cleanup_status": "failed"}, {"status": "passed"}, {"exit_code": 124}):
            receipt = dict(original, **change)
            with self.subTest(change=change), self.assertRaises((RuntimeError, qa.installed.QAFailure)):
                qa.checked_command((receipt, "owned CLI output", ""), 130)
            self.assertEqual(receipt["expectation_status"], "failed")
        invalid = dict(original, exit_code=2)
        self.assertEqual(qa.checked_command((invalid, "", "argument diagnostic"), 2, stderr_required=True), ("", "argument diagnostic"))


    def test_buffered_ready_from_already_exited_child_never_signals_stale_pid(self):
        with tempfile.TemporaryDirectory() as temp:
            entry = Path(temp).resolve() / "entry.m"
            entry.write_text("owned fixture")
            exited = type("ExitedOwnedFixture", (), {"pid": 12345, "poll": lambda self: 0})()
            observer = qa.ReadyObservation(entry, stop=True)
            with patch.object(qa.os, "kill") as stop:
                with self.assertRaisesRegex(RuntimeError, "already exited"):
                    observer(exited, (json.dumps(self.pair()[0]) + "\n").encode())
                stop.assert_not_called()


    def readonly_relocation_fixture(self, root):
        original = root / "original package"
        (original / "bin").mkdir(parents=True)
        (original / "share").mkdir()
        (original / "bin/mathlab").write_bytes(b"synthetic executable bytes; never run")
        (original / "share/notes").write_bytes("original payload Ω".encode())
        for path in original.rglob("*"):
            path.chmod(0o555 if path.is_dir() or path.name == "mathlab" else 0o444)
        original.chmod(0o555)
        output = root / "QA owned evidence"
        output.mkdir()
        relocated = output / "installed with spaces"
        import shutil
        shutil.copytree(original, relocated)
        return original, output, relocated, output / "second relocated install"

    def restore_fixture_directory_modes(self, *roots):
        # Only fresh fixture directories are made removable after assertions.
        for root in roots:
            if root.exists():
                root.chmod(0o755)
                for path in root.rglob("*"):
                    if path.is_dir(): path.chmod(0o755)

    def test_readonly_second_relocation_restores_modes_inventory_and_original(self):
        with tempfile.TemporaryDirectory() as temp:
            original, output, relocated, second = self.readonly_relocation_fixture(Path(temp).resolve())
            before = qa.release.inventory(relocated)
            before_modes = {path.relative_to(relocated).as_posix(): path.stat().st_mode & 0o777
                            for path in relocated.rglob("*")}
            original_before = qa.release.inventory(original)
            real_rename = Path.rename
            def checked_rename(source, target):
                self.assertEqual(source, relocated)
                self.assertEqual(target, second)
                self.assertEqual(source.stat().st_mode & 0o777, 0o755)
                self.assertEqual(qa.release.inventory(source), before)
                self.assertEqual(original.stat().st_mode & 0o777, 0o555)
                return real_rename(source, target)
            try:
                with patch.object(Path, "rename", autospec=True, side_effect=checked_rename) as rename, \
                        patch.object(qa.installed.Runner, "run") as execute:
                    qa.relocate_readonly_copy(relocated, second, output)
                    rename.assert_called_once_with(relocated, second)
                    execute.assert_not_called()
                self.assertFalse(relocated.exists())
                self.assertEqual(second.stat().st_mode & 0o777, 0o555)
                self.assertEqual(qa.release.inventory(second), before)
                self.assertEqual({path.relative_to(second).as_posix(): path.stat().st_mode & 0o777
                                  for path in second.rglob("*")}, before_modes)
                self.assertEqual(qa.release.inventory(original), original_before)
                self.assertEqual(original.stat().st_mode & 0o777, 0o555)
            finally:
                self.restore_fixture_directory_modes(original, relocated, second)

    def test_failed_second_relocation_restores_readonly_without_execution_or_payload_change(self):
        with tempfile.TemporaryDirectory() as temp:
            original, output, relocated, second = self.readonly_relocation_fixture(Path(temp).resolve())
            before = qa.release.inventory(relocated)
            before_modes = {path.relative_to(relocated).as_posix(): path.stat().st_mode & 0o777
                            for path in relocated.rglob("*")}
            original_before = qa.release.inventory(original)
            def refuse_rename(source, target):
                self.assertEqual(source.stat().st_mode & 0o777, 0o755)
                raise PermissionError("synthetic rename refusal")
            try:
                with patch.object(Path, "rename", autospec=True, side_effect=refuse_rename), \
                        patch.object(qa.installed.Runner, "run") as execute:
                    with self.assertRaisesRegex(PermissionError, "synthetic rename refusal"):
                        qa.relocate_readonly_copy(relocated, second, output)
                    execute.assert_not_called()
                self.assertFalse(second.exists())
                self.assertEqual(relocated.stat().st_mode & 0o777, 0o555)
                self.assertEqual(qa.release.inventory(relocated), before)
                self.assertEqual({path.relative_to(relocated).as_posix(): path.stat().st_mode & 0o777
                                  for path in relocated.rglob("*")}, before_modes)
                self.assertEqual(qa.release.inventory(original), original_before)
                self.assertEqual(original.stat().st_mode & 0o777, 0o555)
            finally:
                self.restore_fixture_directory_modes(original, relocated, second)


if __name__ == "__main__":
    unittest.main(verbosity=2)
