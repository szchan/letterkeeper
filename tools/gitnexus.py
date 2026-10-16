# SPDX-License-Identifier: GPL-3.0-only
"""Optional source-only GitNexus launcher. No Cargo or email operations."""
import argparse
import fcntl
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
VERSION = "1.6.12"
DATABASE_VERSION = "0.18.3"
BASE = ROOT / ".local/gitnexus"
READ_TOOLS = ["list_repos", "query", "context", "impact", "trace"]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def admitted(name):
    path = PurePosixPath(name)
    parts = path.parts
    if name in {"Cargo.toml", "Cargo.lock"}:
        return True
    if not parts or path.is_absolute() or ".." in parts:
        return False
    tail = parts[1:] if parts[0] == ".github" else parts
    if any(p.startswith(".") or p in {"target", "fixtures", "vendor", "node_modules"} for p in tail):
        return False
    return (
        (parts[0] == "crates" and (path.suffix == ".rs" or path.name == "Cargo.toml"))
        or (parts[0] in {"tests", "tools"} and path.suffix in {".py", ".sh"})
        or (parts[:2] == (".github", "ci") and path.suffix in {".py", ".sh"})
    )


def export_snapshot(root, destination, max_bytes=32 * 1024 * 1024):
    """Read bounded committed Git blobs; never follow checkout paths or links."""
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(destination)
    commit = git(root, "rev-parse", "HEAD").decode().strip()
    selected = []
    total = 0
    for entry in git(root, "ls-tree", "-rlz", commit).split(b"\0"):
        if not entry:
            continue
        header, raw_name = entry.split(b"\t", 1)
        name = raw_name.decode("utf-8")
        if not admitted(name):
            continue
        mode, kind, oid, size = header.split()
        if mode not in {b"100644", b"100755"} or kind != b"blob":
            raise ValueError("selected source must be a regular Git blob")
        total += int(size)
        if int(size) > 512 * 1024 or total > max_bytes or len(selected) >= 4096:
            raise ValueError("source snapshot exceeds file/count/byte budget")
        selected.append((name, oid.decode()))
    if not selected:
        raise ValueError("no source files selected")
    destination.mkdir(parents=True, mode=0o700)
    try:
        for name, oid in selected:
            target = destination / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(git(root, "cat-file", "blob", oid))
        manifest = {"commit": commit, "files": [name for name, _ in selected]}
        (destination / "source-revision.json").write_text(json.dumps(manifest, indent=2) + "\n")
    except BaseException:
        shutil.rmtree(destination)
        raise
    return manifest


def environment(registry=None, network=False):
    # Drop model keys/endpoints, NODE_OPTIONS and inherited GitNexus settings.
    names = ["PATH", "HOME", "LANG", "LC_ALL", "TERM", "TMPDIR"]
    if network:
        names += ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY", "http_proxy", "https_proxy", "all_proxy", "no_proxy"]
    env = {key: os.environ[key] for key in names if key in os.environ}
    env.update({"SCARF_ANALYTICS": "false", "DO_NOT_TRACK": "1", "GITNEXUS_MCP_READ_ONLY": "1"})
    env["GITNEXUS_LBUG_EXTENSION_INSTALL"] = "load-only"
    if registry:
        env["GITNEXUS_HOME"] = str(registry)
    return env


def install():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("this no-build installer supports Linux x86_64 only")
    tool = BASE / "tool"
    subprocess.run([
        "npm", "install", "--prefix", str(tool), "--ignore-scripts", "--omit=optional",
        "--no-audit", "--no-fund", "--cache", str(BASE / "npm-cache"),
        f"gitnexus@{VERSION}", f"@ladybugdb/core-linux-x64@{DATABASE_VERSION}",
    ], check=True, env=environment(network=True))
    # The upstream installer also copies this file, but can fall back to compiling.
    # Copy the explicit platform package ourselves; absence is an error.
    modules = tool / "node_modules"
    shutil.copyfile(modules / "@ladybugdb/core-linux-x64/lbugjs.node", modules / "@ladybugdb/core/lbugjs.node")
    binary()
    # This upstream downloader only installs a prebuilt extension. LadybugDB
    # uses ~/.lbdb for its shared extension cache; no editor settings are changed.
    subprocess.run([
        "node", str(modules / "gitnexus/scripts/install-duckdb-extension.mjs"),
        "fts", "8589934592",
    ], check=True, timeout=60, env=environment(network=True))
    print(f"Installed GitNexus {VERSION}; lifecycle scripts disabled; no local compilation.")


def binary():
    package = BASE / "tool/node_modules/gitnexus"
    if not (package / "package.json").is_file():
        raise ValueError("run: python3 tools/gitnexus.py install")
    if json.loads((package / "package.json").read_text())["version"] != VERSION:
        raise ValueError("GitNexus version mismatch; reinstall the pinned version")
    return package / "dist/cli/index.js"


def invoke(args, snapshot, registry, replace=False):
    command = ["node", str(binary()), *args]
    env = environment(registry)
    if replace:
        os.chdir(snapshot)
        os.execvpe(command[0], command, env)
    subprocess.run(command, cwd=snapshot, env=env, check=True)


def index():
    # Serialize publication; failed analysis never replaces the current pointer.
    with (BASE / "index.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        sha = git(ROOT, "rev-parse", "HEAD").decode().strip()
        generation = sha + "-" + uuid.uuid4().hex[:12]
        snapshot = BASE / "snapshots" / generation
        registry = BASE / "registries" / generation
        manifest = export_snapshot(ROOT, snapshot)
        if manifest["commit"] != sha:
            raise ValueError("HEAD changed during export; rerun index")
        invoke(["analyze", str(snapshot), "--skip-git", "--index-only", "--workers", "2", "--name", "letterkeeper"], snapshot, registry)
        meta = json.loads((snapshot / ".gitnexus/meta.json").read_text())
        verify_index(meta)
        pending = BASE / "current.pending.json"
        pending.write_text(json.dumps({"commit": sha, "snapshot": generation}) + "\n")
        pending.replace(BASE / "current.json")
        print(f"Source revision: {sha}", flush=True)


def verify_index(meta):
    if not meta.get("stats", {}).get("nodes"):
        raise ValueError("index has no nodes; current pointer not published")
    if meta.get("capabilities", {}).get("fts", {}).get("status") != "available":
        raise ValueError("FTS unavailable; run install again; current pointer not published")


def current():
    marker = BASE / "current.json"
    if not marker.is_file():
        raise ValueError("run: python3 tools/gitnexus.py index")
    marker_data = json.loads(marker.read_text())
    sha = marker_data["commit"]
    generation = marker_data.get("snapshot", "")
    if len(sha) != 40 or any(c not in "0123456789abcdef" for c in sha):
        raise ValueError("invalid index revision")
    if not re.fullmatch(re.escape(sha) + r"-[0-9a-f]{12}", generation):
        raise ValueError("invalid snapshot generation")
    if sha != git(ROOT, "rev-parse", "HEAD").decode().strip():
        print("GitNexus snapshot is stale; run python3 tools/gitnexus.py index", file=sys.stderr)
    return sha, BASE / "snapshots" / generation, BASE / "registries" / generation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["install", "index", "status", "serve", "mcp", "query", "context", "impact", "trace"])
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command in {"install", "index", "status", "serve", "mcp"} and args.arguments:
        parser.error("this command does not accept extra arguments")
    os.umask(0o077)
    BASE.mkdir(parents=True, exist_ok=True, mode=0o700)
    if args.command == "install":
        install()
    elif args.command == "index":
        index()
    else:
        sha, snapshot, registry = current()
        if args.command == "status":
            print(f"Indexed source revision: {sha}", flush=True)
            invoke(["status", "--repo", "letterkeeper"], snapshot, registry)
        elif args.command == "serve":
            print(f"Graph: http://127.0.0.1:4747 (source {sha})", flush=True)
            invoke(["serve", "--host", "127.0.0.1"], snapshot, registry, replace=True)
        else:
            # No automatic setup, wiki, embeddings, rename, raw Cypher or HTTP MCP.
            invoke([args.command, *args.arguments], snapshot, registry, replace=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"GitNexus: {error}", file=sys.stderr)
        sys.exit(1)
