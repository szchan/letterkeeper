# SPDX-License-Identifier: GPL-3.0-only
"""Source-export boundaries, using only synthetic Git repositories."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location("gitnexus_tool", ROOT / "tools/gitnexus.py")
        self.tool = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.tool)
        self.tmp = tempfile.TemporaryDirectory(prefix="letterkeeper-graph-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "source with spaces"
        self.root.mkdir()
        self.dest = Path(self.tmp.name) / "snapshot"
        self.git("init", "-q")
        self.put("Cargo.toml", '[workspace]\nmembers = ["crates/demo"]\n')
        self.put("crates/demo/Cargo.toml", '[package]\nname = "demo"\nversion = "0.1.0"\n')
        self.put("crates/demo/src/lib.rs", "pub fn synthetic_entry() {}\n")
        self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.PIPE).decode().strip()

    def put(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def commit(self):
        self.git("add", ".")
        self.git("-c", "user.name=Synthetic Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "synthetic")

    def test_exports_committed_source_and_records_exact_revision(self):
        sha = self.git("rev-parse", "HEAD")
        self.put("crates/demo/src/lib.rs", "uncommitted input must not be indexed\n")
        self.tool.export_snapshot(self.root, self.dest)
        self.assertEqual((self.dest / "crates/demo/src/lib.rs").read_text(), "pub fn synthetic_entry() {}\n")
        manifest = json.loads((self.dest / "source-revision.json").read_text())
        self.assertEqual(manifest["commit"], sha)
        self.assertEqual(manifest["files"], ["Cargo.toml", "crates/demo/Cargo.toml", "crates/demo/src/lib.rs"])
        self.assertFalse((self.dest / ".git").exists())

    def test_excludes_private_data_even_if_accidentally_tracked(self):
        for name in [".local/secret.rs", ".env", "crates/demo/.env", "tests/fixtures/mail.eml", "tests/fixtures/secret.rs", "target/generated.rs", "docs/private.md"]:
            self.put(name, "synthetic-private-canary\n")
        self.commit()
        self.put("crates/demo/src/untracked.rs", "untracked-canary\n")
        self.tool.export_snapshot(self.root, self.dest)
        for path in self.dest.rglob("*"):
            if path.is_file():
                self.assertNotIn("canary", path.read_text())

    def test_refuses_selected_symlinks_without_following_them(self):
        outside = Path(self.tmp.name) / "outside.rs"
        outside.write_text("outside-canary\n")
        (self.root / "crates/demo/src/link.rs").symlink_to(outside)
        self.commit()
        with self.assertRaisesRegex(ValueError, "regular"):
            self.tool.export_snapshot(self.root, self.dest)
        self.assertFalse(self.dest.exists())

    def test_refuses_existing_destination_without_modifying_it(self):
        self.dest.mkdir()
        (self.dest / "keep").write_text("keep")
        with self.assertRaises(FileExistsError):
            self.tool.export_snapshot(self.root, self.dest)
        self.assertEqual((self.dest / "keep").read_text(), "keep")

    def test_rejects_source_size_over_budget_before_writing(self):
        self.put("crates/demo/src/large.rs", "x" * 1025)
        self.commit()
        with self.assertRaisesRegex(ValueError, "budget"):
            self.tool.export_snapshot(self.root, self.dest, max_bytes=1024)
        self.assertFalse(self.dest.exists())


if __name__ == "__main__":
    unittest.main()
