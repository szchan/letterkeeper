#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Hosted manual release/performance sample, publishing no release automatically."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

PAYLOAD_FILES = {"letterkeeper", "LICENSE", "source.tar.gz", "vendor.tar.gz", "REBUILD.md",
                 "performance-phase-a.json", "performance-phase-c.json"}


def archive_vendor(root, destination):
    with tarfile.open(destination, "w:gz") as archive:
        archive.add(root / "vendor", arcname="vendor")
        archive.add(root / ".cargo/config.toml", arcname=".cargo/config.toml")


def artifact_hashes(output):
    if {path.name for path in output.iterdir()} != PAYLOAD_FILES:
        raise ValueError("artifact_payload_not_allowlisted")
    result = {}
    for name in sorted(PAYLOAD_FILES):
        path = output / name
        if path.is_symlink() or not path.is_file():
            raise ValueError("artifact_payload_requires_regular_files")
        result[name] = sha256(path)
    return result


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def vendor_dependencies(output):
    manifest_path = str(Path("Cargo.toml").resolve())
    with tempfile.TemporaryDirectory(prefix="letterkeeper-vendor-") as directory:
        root = Path(directory)
        vendored = run(["cargo", "vendor", "--locked", "--versioned-dirs", "--manifest-path",
                       manifest_path, "vendor"], cwd=root, stdout=subprocess.PIPE, text=True)
        config = tomllib.loads(vendored.stdout)
        if config["source"]["vendored-sources"]["directory"] != "vendor":
            raise ValueError("vendor_configuration_requires_relative_directory")
        (root / ".cargo").mkdir()
        (root / ".cargo/config.toml").write_text(vendored.stdout)
        # Resolve from the packaged source replacement offline before publication.
        # All vendored files, including upstream license/notices/checksums, stay intact.
        run(["cargo", "metadata", "--locked", "--offline", "--format-version=1",
             "--manifest-path", manifest_path], cwd=root, stdout=subprocess.DEVNULL)
        archive_vendor(root, output / "vendor.tar.gz")


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
    vendor_dependencies(output)
    (output / "REBUILD.md").write_text(f"""# Rebuilding the hosted sample

Project commit: `{commit}`. Rust toolchain: `1.99.0`.
Runner: Ubuntu 24.04 x86_64 hosted VM. This is a build artifact, not a published release.

Verify `sha256sum --check SHA256SUMS` in the downloaded artifact directory.
Create an empty build directory and extract both `source.tar.gz` and `vendor.tar.gz`
there. The latter includes every Cargo-vendored dependency source with its original
license/copyright/notice files and `.cargo/config.toml` for offline source replacement.
The project's original GPL-3.0-only LICENSE remains unchanged. Third-party source
retains its own license terms; this package is not a legal certification.

Install Rust 1.99.0, build-essential, pkg-config, Python 3 and bubblewrap on Ubuntu 24.04.
From the extracted build directory:

```sh
cargo +1.99.0 build --locked --offline --release --workspace
cargo +1.99.0 test --locked --offline --release --workspace
```

The tests require PTY, loopback and working unprivileged/nested bubblewrap namespaces.
Rust/system tool installation may need network; Cargo dependency sources are included.
The exact source/artifact SHA-256 values are in `manifest.json` and `SHA256SUMS`.
The dependency graph is resolved offline in CI, but bit-for-bit binary reproducibility
across compilers, system libraries, paths and machines is not asserted.
""")
    payload_hashes = artifact_hashes(output)
    manifest = {"commit": commit, "repository": os.environ["GITHUB_REPOSITORY"],
                "source_url": "https://github.com/" + os.environ["GITHUB_REPOSITORY"] + "/tree/" + commit,
                "rust": "1.99.0", "runner": "ubuntu-24.04", "license": "GPL-3.0-only",
                "cargo_lock_sha256": sha256(Path("Cargo.lock")),
                "payload_sha256": payload_hashes,
                "dependency_sources": "cargo vendor --locked --versioned-dirs; original notices preserved",
                "scope": "synthetic controlled benchmark; not real-provider acceptance",
                "release_published": False}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    checksums = {**payload_hashes, "manifest.json": sha256(output / "manifest.json")}
    (output / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n"
                                              for name, digest in sorted(checksums.items())))
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as stream:
        stream.write("\nManual release/performance sample:\n\n```json\n" +
                     json.dumps(manifest, indent=2) + "\n```\n")


if __name__ == "__main__":
    main()
