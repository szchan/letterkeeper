#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Lightweight orchestration checks; never invoke Cargo locally."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("tdd", ROOT / ".github/ci/tdd.py")
tdd = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tdd)
RELEASE_SPEC = importlib.util.spec_from_file_location("release", ROOT / ".github/ci/release.py")
release = importlib.util.module_from_spec(RELEASE_SPEC)
RELEASE_SPEC.loader.exec_module(release)
SHA = "a" * 40


class EvidenceTests(unittest.TestCase):
    def test_valid_exact_selector(self):
        tdd.validate(SHA, "letterkeeper-core", "module::reject_send", "RED")

    def test_reject_untrusted_selectors(self):
        for field, bad in ((0, "main"), (0, "a" * 39), (0, "a" * 40 + "\n"),
                           (1, "unknown"), (1, "--workspace"), (2, ""),
                           (2, "$(touch /tmp/leak)"), (2, "foo;exit 0"),
                           (2, "--ignored"), (2, "name\nother"), (3, "PASS")):
            values = [SHA, "letterkeeper", "module::reject_send", "RED"]
            values[field] = bad
            with self.subTest(field=field, bad=bad), self.assertRaises(ValueError):
                tdd.validate(*values)

    def test_command_is_argument_array_and_exact(self):
        self.assertEqual(tdd.command("letterkeeper", "module::reject_send"),
                         ["cargo", "test", "--locked", "-p", "letterkeeper",
                          "module::reject_send", "--", "--exact", "--nocapture"])

    def run_case(self, listing, test_status=0, actual_sha=SHA):
        calls = []
        def runner(arguments, **kwargs):
            self.assertIsInstance(arguments, list)
            self.assertNotIn("shell", kwargs)
            calls.append(arguments)
            if arguments[0] == "git":
                return subprocess.CompletedProcess(arguments, 0, actual_sha + "\n")
            if "--list" in arguments:
                return subprocess.CompletedProcess(arguments, 0, listing)
            return subprocess.CompletedProcess(arguments, test_status)
        with tempfile.TemporaryDirectory() as directory:
            summary = Path(directory) / "summary.md"
            result = tdd.execute(SHA, "letterkeeper", "module::reject_send", "RED", runner, summary)
            evidence = summary.read_text()
        return result, calls, evidence

    def test_raw_red_failure_stays_failure_and_records_sha(self):
        result, calls, evidence = self.run_case("module::reject_send: test\n", 101)
        self.assertEqual(result, 101)
        self.assertIn(SHA, evidence)
        self.assertIn("101", evidence)
        self.assertIn("RED", evidence)
        self.assertEqual(len(calls), 3)

    def test_passing_red_is_recorded_as_pass_without_claiming_expected_failure(self):
        result, _, evidence = self.run_case("module::reject_send: test\n")
        self.assertEqual(result, 0)
        self.assertIn("raw_exit_code", evidence)
        self.assertIn("0", evidence)

    def test_zero_or_ambiguous_matches_are_refused_before_test_execution(self):
        for listing in ("other: test\n", "module::reject_send: test\n" * 2):
            result, calls, evidence = self.run_case(listing)
            self.assertNotEqual(result, 0)
            self.assertEqual(len(calls), 2)
            self.assertIn("selector_not_unique", evidence)

    def test_checkout_sha_mismatch_refuses_cargo(self):
        result, calls, evidence = self.run_case("module::reject_send: test\n", actual_sha="b" * 40)
        self.assertNotEqual(result, 0)
        self.assertEqual(len(calls), 1)
        self.assertIn("checkout_sha_mismatch", evidence)

    def test_compile_red_preserves_failure_without_running_test(self):
        calls = []
        def runner(arguments, **kwargs):
            calls.append(arguments)
            if arguments[0] == "git":
                return subprocess.CompletedProcess(arguments, 0, SHA + "\n")
            return subprocess.CompletedProcess(arguments, 101, "synthetic compile failure\n")
        with tempfile.TemporaryDirectory() as directory:
            summary = Path(directory) / "summary"
            result = tdd.execute(SHA, "letterkeeper", "reject_send", "RED", runner, summary)
            evidence = summary.read_text()
        self.assertEqual(result, 101)
        self.assertEqual(len(calls), 2)
        self.assertIn("test_inventory_failed", evidence)
        self.assertIn(SHA, evidence)
        self.assertIn("101", evidence)

    def test_long_selector_is_refused(self):
        with self.assertRaises(ValueError):
            tdd.validate(SHA, "letterkeeper", "x" * 257, "GREEN")


class ReleaseSourceTests(unittest.TestCase):
    def test_vendor_archive_preserves_original_notice_and_excludes_private_sibling(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "vendor/synthetic-1.0").mkdir(parents=True)
            notice = b"Synthetic upstream notice\nOriginal attribution\n"
            (root / "vendor/synthetic-1.0/LICENSE").write_bytes(notice)
            (root / ".cargo").mkdir()
            config = b'[source.vendored-sources]\ndirectory = "vendor"\n'
            (root / ".cargo/config.toml").write_bytes(config)
            (root / ".local").mkdir()
            (root / ".local/private").write_text("must not enter artifact")
            archive = root / "vendor.tar.gz"
            release.archive_vendor(root, archive)
            with tarfile.open(archive) as packaged:
                self.assertEqual(packaged.extractfile("vendor/synthetic-1.0/LICENSE").read(), notice)
                self.assertEqual(packaged.extractfile(".cargo/config.toml").read(), config)
                self.assertFalse(any(name.startswith(".local") for name in packaged.getnames()))

    def test_output_hashing_requires_allowlist_and_refuses_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("letterkeeper", "LICENSE", "source.tar.gz", "vendor.tar.gz", "REBUILD.md",
                         "performance-phase-a.json", "performance-phase-c.json"):
                (root / name).write_bytes(b"abc")
            hashes = release.artifact_hashes(root)
            self.assertEqual(hashes["letterkeeper"],
                             "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
            (root / ".local").mkdir()
            with self.assertRaises(ValueError):
                release.artifact_hashes(root)
            (root / ".local").rmdir()
            (root / "letterkeeper").unlink()
            (root / "letterkeeper").symlink_to("LICENSE")
            with self.assertRaises(ValueError):
                release.artifact_hashes(root)


if __name__ == "__main__":
    unittest.main()
