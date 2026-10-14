#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Hosted manual release/performance sample, publishing no release automatically."""
import json
import os
from pathlib import Path
import shutil
import subprocess


def archive_vendor(root, destination):
    raise NotImplementedError


def artifact_hashes(output):
    raise NotImplementedError


def run(arguments, **options):
    return subprocess.run(arguments, check=True, **options)


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("release builds are restricted to GitHub Actions")
    run(["cargo", "test", "--locked", "--release", "--workspace"])
    run(["cargo", "build", "--locked", "--release", "--workspace"])
    build = run(["cargo", "test", "--locked", "--release", "-p", "letterkeeper",
                 "--test", "terminal_workflow", "--no-run", "--message-format=json"],
                stdout=subprocess.PIPE, text=True)
    binaries = []
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact" and message.get("executable")
                and message["target"]["name"] == "terminal_workflow"):
            binaries.append(message["executable"])
    if len(binaries) != 1:
        raise SystemExit("release_terminal_workflow_binary_not_unique")
    output = Path("target/ci-release")
    output.mkdir(parents=True, exist_ok=False)
    for script, executable, name in (
            ("scripts/benchmark.py", "target/release/letterkeeper", "performance-phase-a.json"),
            ("scripts/benchmark_phase_c.py", binaries[0], "performance-phase-c.json")):
        with (output / name).open("w") as stream:
            run(["python3", script, executable], stdout=stream)
    shutil.copy2("target/release/letterkeeper", output / "letterkeeper")
    shutil.copy2("LICENSE", output / "LICENSE")
    commit = run(["git", "rev-parse", "HEAD"], stdout=subprocess.PIPE, text=True).stdout.strip()
    run(["git", "archive", "--format=tar.gz", "--output=" + str(output / "source.tar.gz"), commit])
    manifest = {"commit": commit, "repository": os.environ["GITHUB_REPOSITORY"],
                "source_url": "https://github.com/" + os.environ["GITHUB_REPOSITORY"] + "/tree/" + commit,
                "rust": "1.99.0", "runner": "ubuntu-24.04", "license": "GPL-3.0-only",
                "scope": "synthetic controlled benchmark; not real-provider acceptance",
                "release_published": False}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as stream:
        stream.write("\nManual release/performance sample:\n\n```json\n" +
                     json.dumps(manifest, indent=2) + "\n```\n")


if __name__ == "__main__":
    main()
