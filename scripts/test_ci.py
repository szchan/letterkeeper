#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Lightweight orchestration checks; never invoke Cargo locally."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("tdd", ROOT / "scripts/ci/tdd.py")
tdd = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tdd)
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


if __name__ == "__main__":
    unittest.main()
