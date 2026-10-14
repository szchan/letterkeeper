#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run one exact Rust test remotely and preserve its actual exit status."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

CRATES = {"letterkeeper", "letterkeeper-content", "letterkeeper-core",
          "letterkeeper-adapters", "letterkeeper-protocol", "letterkeeper-accounts"}


def validate(commit, crate, test, phase):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("commit_requires_full_lowercase_sha")
    if crate not in CRATES:
        raise ValueError("unknown_workspace_crate")
    if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)*", test):
        raise ValueError("test_requires_exact_rust_name")
    if len(test) > 256:
        raise ValueError("test_name_too_long")
    if phase not in {"RED", "GREEN"}:
        raise ValueError("invalid_phase")


def command(crate, test, list_only=False):
    base = ["cargo", "test", "--locked", "-p", crate]
    return base + (["--", "--list"] if list_only else [test, "--", "--exact", "--nocapture"])


def execute(commit, crate, test, phase, runner, summary):
    validate(commit, crate, test, phase)
    evidence = {"commit": commit, "crate": crate, "test": test, "phase": phase,
                "matched_tests": 0, "raw_exit_code": None, "state": "checkout_sha_mismatch"}
    checked = runner(["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
    result = 2
    if checked.returncode == 0 and checked.stdout.strip() == commit:
        listing = runner(command(crate, test, True), stdout=subprocess.PIPE, text=True, check=False)
        print(listing.stdout, end="", flush=True)
        evidence["raw_exit_code"] = listing.returncode
        evidence["state"] = "test_inventory_failed"
        result = listing.returncode
        if listing.returncode == 0:
            count = listing.stdout.splitlines().count(test + ": test")
            evidence["matched_tests"] = count
            evidence["state"] = "selector_not_unique"
            result = 2
            if count == 1:
                outcome = runner(command(crate, test), check=False)
                result = outcome.returncode
                evidence["raw_exit_code"] = result
                evidence["state"] = "test_finished"
    serialized = json.dumps(evidence, sort_keys=True)
    print("TDD_EVIDENCE " + serialized, flush=True)
    with summary.open("a") as stream:
        stream.write("\nTDD evidence (phase is a label; failures remain failures):\n\n```json\n" + serialized + "\n```\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    values = [os.environ.get(name, "") for name in
              ("CI_TEST_COMMIT", "CI_TEST_CRATE", "CI_TEST_NAME", "CI_TEST_PHASE")]
    try:
        validate(*values)
    except ValueError as error:
        parser.error(str(error))
    if args.validate_only:
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as stream:
            stream.write("commit=" + values[0] + "\n")
        return 0
    if os.environ.get("GITHUB_ACTIONS") != "true":
        parser.error("Cargo evidence runs are restricted to GitHub Actions")
    return execute(*values, subprocess.run, Path(os.environ["GITHUB_STEP_SUMMARY"]))


if __name__ == "__main__":
    raise SystemExit(main())
