#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Release PTY and active MCP measurements, launched by the workspace Cargo bench."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import threading
import time


def pids(root):
    pending, seen = [root], set()
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        try:
            for task in Path(f"/proc/{pid}/task").iterdir():
                pending += [int(item) for item in (task / "children").read_text().split()]
        except (FileNotFoundError, PermissionError):
            pass
    return seen


def usage(root):
    rss = pss = ticks = 0
    for pid in pids(root):
        try:
            for row in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
                if row.startswith("Rss:"):
                    rss += int(row.split()[1])
                if row.startswith("Pss:"):
                    pss += int(row.split()[1])
            stat = Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()
            ticks += int(stat[11]) + int(stat[12])
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
    return rss, pss, ticks


class TreeSampler:
    """Sample app and worker processes throughout each active workload."""

    def __init__(self, root):
        self.root = root
        self.stop_event = threading.Event()
        self.peak_rss = 0
        self.peak_pss = 0
        self.samples = 0
        self.thread = threading.Thread(target=self._run, daemon=True)

    def _run(self):
        while not self.stop_event.is_set():
            rss, pss, _ = usage(self.root)
            self.peak_rss = max(self.peak_rss, rss)
            self.peak_pss = max(self.peak_pss, pss)
            self.samples += 1
            self.stop_event.wait(0.005)

    def start(self):
        self.thread.start()

    def stop(self):
        self.stop_event.set()
        self.thread.join(timeout=2)
        assert not self.thread.is_alive(), "tree sampler did not stop"
        assert self.samples > 0, "tree sampler collected no process samples"


def drain(fd):
    data = bytearray()
    while select.select([fd], [], [], 0)[0]:
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        data += chunk
    return data


def await_marker(fd, marker, timeout=15):
    start = time.perf_counter()
    data = bytearray()
    while time.perf_counter() - start < timeout:
        if select.select([fd], [], [], 0.01)[0]:
            data += drain(fd)
        plain = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data)
        if b"".join(marker.split()) in b"".join(plain.split()):
            return (time.perf_counter() - start) * 1000
        assert len(data) < 2 * 1024 * 1024, "terminal output grew without bound"
    raise AssertionError(f"no marker {marker!r}; captured={data[-500:]!r}")


def run(driver, binary, count, body_bytes, mode):
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    env = {**os.environ, "TERM": "xterm-256color", "LETTERKEEPER_BENCH_COUNT": str(count),
           "LETTERKEEPER_BENCH_BODY_BYTES": str(body_bytes),
           "LETTERKEEPER_BENCH_BINARY": str(binary)}
    test_name = "workspace_benchmark_child"
    if mode == "tls_imap":
        env["LETTERKEEPER_BENCH_TLS_COUNT"] = str(count)
        test_name = "workspace_tls_benchmark_child"
    start = time.perf_counter()
    proc = subprocess.Popen([str(driver), "--child", test_name],
                            stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True)
    sampler = TreeSampler(proc.pid)
    sampler.start()
    try:
        await_marker(master, b"Synthetic subject", timeout=90)
        startup = (time.perf_counter() - start) * 1000
        samples = []
        for _ in range(20):
            drain(master)
            before = time.perf_counter()
            os.write(master, b"?")
            await_marker(master, "禁止自动链接".encode())
            samples.append((time.perf_counter() - before) * 1000)
            drain(master)
            os.write(master, b"?")
            await_marker(master, b"Synthetic subject")
        drain(master)
        os.write(master, b"\r")
        read_ms = await_marker(master, b"xxxx" if mode == "tls_imap" else b"mail-0", timeout=90)
        drain(master)
        os.write(master, b"\x1b")
        await_marker(master, "已返回".encode(), timeout=15)
        drain(master)
        os.write(master, b"d")
        await_marker(master, b"Benchmark editor", timeout=15)
        drain(master)
        os.write(master, b"\r")
        await_marker(master, b"Ctrl+S", timeout=15)
        for _ in range(3):
            os.write(master, b"\t")
            time.sleep(0.05)
            drain(master)
        os.write(master, b"\t")
        await_marker(master, b"Body", timeout=15)
        editor_samples = []
        for char in b"ABCDEFGHIJKLMNOPQRST":
            drain(master)
            before = time.perf_counter()
            os.write(master, bytes([char]))
            await_marker(master, bytes([char]), timeout=15)
            editor_samples.append((time.perf_counter() - before) * 1000)
        boundary_reject_ms = None
        if body_bytes >= 1024 * 1024:
            drain(master)
            os.write(master, b"U")
            boundary_reject_ms = await_marker(master, "编辑字段超过限制".encode(), timeout=15)
        idle_start = time.perf_counter()
        cpu_start = usage(proc.pid)[2]
        while time.perf_counter() - idle_start < 2:
            time.sleep(0.05)
        cpu_end = usage(proc.pid)[2]
        idle_elapsed = time.perf_counter() - idle_start
        hz = os.sysconf("SC_CLK_TCK")
        os.write(master, b"\x11")  # Ctrl+Q, then discard the unsaved benchmark edit.
        await_marker(master, "未保存".encode(), timeout=15)
        os.write(master, b"d")
        proc.wait(timeout=5)
        assert proc.returncode == 0
        sampler.stop()
        ordered = sorted(samples)
        editor_ordered = sorted(editor_samples)
        return {"mode": mode, "mailbox_count": count, "fixture_bytes": body_bytes,
                "startup_warm_ms": round(startup, 2), "help_input_p95_ms": round(ordered[18], 2),
                "editor_initial_body_bytes": min(body_bytes, 1024 * 1024 - 20),
                "editor_redraw_p95_ms": round(editor_ordered[18], 2),
                "editor_boundary_reject_ms": round(boundary_reject_ms, 2) if boundary_reject_ms is not None else None,
                "read_render_ms": round(read_ms, 2), "tree_peak_rss_kib_sampled": sampler.peak_rss,
                "tree_peak_pss_kib_sampled": sampler.peak_pss, "tree_samples": sampler.samples,
                "idle_cpu_percent_one_core": round(100 * (cpu_end - cpu_start) / hz / idle_elapsed, 3),
                "input_budget_100ms_met": ordered[18] <= 100 and editor_ordered[18] <= 100,
                "idle_cpu_budget_1pct_met": 100 * (cpu_end - cpu_start) / hz / idle_elapsed <= 1}
    finally:
        if sampler.thread.is_alive():
            sampler.stop()
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
        os.close(master)
        os.close(slave)


def run_mcp(driver, binary, count, raw_bytes):
    env = {**os.environ, "LETTERKEEPER_BENCH_MCP_COUNT": str(count),
           "LETTERKEEPER_BENCH_BODY_BYTES": str(raw_bytes),
           "LETTERKEEPER_BENCH_BINARY": str(binary)}
    proc = subprocess.Popen([str(driver), "--child", "workspace_tls_mcp_benchmark_child"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            env=env, bufsize=0)
    sampler = TreeSampler(proc.pid)
    sampler.start()
    def exchange(message, wanted_id):
        start = time.perf_counter()
        proc.stdin.write(json.dumps(message, separators=(",", ":")).encode() + b"\n")
        proc.stdin.flush()
        until = start + 90
        while time.perf_counter() < until:
            if not select.select([proc.stdout], [], [], 0.1)[0]:
                assert proc.poll() is None, "MCP fixture exited"
                continue
            line = proc.stdout.readline()
            if not line:
                raise AssertionError("MCP stream closed")
            try:
                response = json.loads(line)
            except json.JSONDecodeError:
                raise AssertionError("MCP driver emitted a non-protocol frame")
            if response.get("id") == wanted_id:
                assert "error" not in response, response
                return (time.perf_counter() - start) * 1000, response["result"]
        raise AssertionError(f"MCP response {wanted_id} timed out")
    try:
        startup_ms, _ = exchange({"jsonrpc":"2.0","id":1,"method":"initialize",
                                  "params":{"protocolVersion":"2025-06-18","capabilities":{},
                                            "clientInfo":{"name":"phase-c-benchmark","version":"0.1"}}}, 1)
        proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()
        def tool(request_id, name, arguments):
            latency, result = exchange({"jsonrpc":"2.0","id":request_id,"method":"tools/call",
                                        "params":{"name":name,"arguments":arguments}}, request_id)
            assert not result.get("isError"), result
            return latency, json.loads(result["content"][0]["text"])
        list_ms, listing = tool(2, "list_messages", {"offset":0,"limit":50})
        messages = listing["messages"]
        assert len(messages) == 50
        search_ms, _ = tool(3, "search_messages", {"query":"Synthetic","offset":0,"limit":50})
        read_ms, _ = tool(4, "read_message", {"id":messages[0]["id"]})
        sampler.stop()
        return {"mode":"tls_imap_active_mcp_stdio", "mailbox_count":count, "raw_eml_bytes":raw_bytes,
                "initialize_ms":round(startup_ms,2), "list_50_ms":round(list_ms,2),
                "search_50_ms":round(search_ms,2), "read_ms":round(read_ms,2),
                "tree_peak_rss_kib_sampled":sampler.peak_rss,
                "tree_peak_pss_kib_sampled":sampler.peak_pss, "tree_samples":sampler.samples}
    finally:
        if sampler.thread.is_alive():
            sampler.stop()
        if proc.poll() is None:
            proc.stdin.close()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--driver", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary, driver = args.binary.resolve(), args.driver.resolve()
    assert binary.is_file() and driver.is_file()
    results = [run(driver, binary, count, body, mode) for mode in ("injected_host", "tls_imap")
               for count in (1000, 10000) for body in (32768, 2 * 1024 * 1024)]
    mcp_results = [run_mcp(driver, binary, count, body) for count in (1000, 10000)
                   for body in (32768, 2 * 1024 * 1024)]
    result = {"profile": "release Cargo bench target with explicit application binary", "fixture": "synthetic sparse metadata; controlled TLS RemoteHost includes sandbox worker",
                      "cache": "warm OS caches; no drop_caches", "tui_workloads": results,
                      "mcp_workloads":mcp_results,
                      "limits": "fixture_bytes denotes injected SafeDocument text in injected_host mode and whole raw EML in tls_imap mode; TLS SafeDocument text is capped at 64 Ki characters. No real service, attachment, public MCP client, or cold cache. Background /proc polling every 5ms begins immediately after spawn and can still miss peaks shorter than a sampling interval. MCP driver emits SDK protocol frames directly."}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    assert all(row["input_budget_100ms_met"] and row["idle_cpu_budget_1pct_met"] for row in results), "performance budget exceeded"
