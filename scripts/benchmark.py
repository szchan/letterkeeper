#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Small Linux baseline, using only bundled synthetic mail and /proc child stats."""
import hashlib
import json
import os
from pathlib import Path
import resource
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target/debug/letterkeeper").resolve()
FIXTURE = ROOT / "tests/fixtures/welcome.eml"


def memory_tree(pid):
    pending = [pid]
    seen = set()
    rss = pss = 0
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            for task in Path(f"/proc/{current}/task").iterdir():
                pending.extend(map(int, (task / "children").read_text().split()))
            for line in Path(f"/proc/{current}/smaps_rollup").read_text().splitlines():
                if line.startswith("Rss:"):
                    rss += int(line.split()[1])
                if line.startswith("Pss:"):
                    pss += int(line.split()[1])
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
    return rss, pss


# Prime executable and fixture caches; no cold-cache or drop_caches claims.
subprocess.run([str(BINARY), "read", str(FIXTURE)], check=True, stdout=subprocess.DEVNULL)
latencies = []
peak_rss = peak_pss = 0
cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
for _ in range(20):
    start = time.perf_counter()
    process = subprocess.Popen([str(BINARY), "read", str(FIXTURE)], stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL)
    while process.poll() is None:
        rss, pss = memory_tree(process.pid)
        peak_rss = max(peak_rss, rss)
        peak_pss = max(peak_pss, pss)
        time.sleep(0.001)
    assert process.returncode == 0, "isolated parse failed"
    latencies.append((time.perf_counter() - start) * 1000)
cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)

with tempfile.TemporaryDirectory(prefix="letterkeeper-benchmark-") as tmp:
    process = subprocess.Popen([str(BINARY), "--state-dir", str(Path(tmp) / "private"),
                                "mcp", "--mail", str(FIXTURE)], stdin=subprocess.PIPE,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(0.3)
        assert process.poll() is None, "MCP startup failed"
        idle = [memory_tree(process.pid) for _ in range(10)]
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()

cpu = cpu_after.ru_utime + cpu_after.ru_stime - cpu_before.ru_utime - cpu_before.ru_stime
print(json.dumps({
    "binary_profile": "debug if using the default path; caller selects profile",
    "fixture_bytes": FIXTURE.stat().st_size,
    "fixture_sha256": hashlib.sha256(FIXTURE.read_bytes()).hexdigest(),
    "runs": 20,
    "cache": "warm executable and fixture",
    "read_with_worker": {"median_ms": round(statistics.median(latencies), 2),
                         "p95_ms": round(sorted(latencies)[18], 2),
                         "sampled_tree_peak_rss_kib": peak_rss,
                         "sampled_tree_peak_pss_kib": peak_pss,
                         "total_child_cpu_seconds": round(cpu, 3)},
    "mcp_preloaded_awaiting_handshake": {"rss_kib": max(x[0] for x in idle),
                                         "pss_kib": max(x[1] for x in idle)},
    "limits": "1ms sampling may miss peaks; no real account, search, authenticated MCP workload or cold-cache result",
}, indent=2))
