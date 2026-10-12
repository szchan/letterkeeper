#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the real terminal driver and human approval path with synthetic data."""
import fcntl
import json
import os
import re
from pathlib import Path
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target/debug/letterkeeper").resolve()


def launch(args, width=80, height=24):
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
    before = termios.tcgetattr(slave)
    process = subprocess.Popen([str(BINARY), *map(str, args)], stdin=slave, stdout=slave, stderr=slave,
                               env={**os.environ, "TERM": "xterm-256color"}, start_new_session=True)
    return process, master, slave, before


def receive(process, master, needle, timeout=10):
    data = bytearray()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if select.select([master], [], [], 0.1)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            data.extend(chunk)
            if needle in re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data):
                return bytes(data)
        if process.poll() is not None:
            break
    raise AssertionError(f"terminal did not display expected marker: {needle!r}; exit={process.poll()}; captured={bytes(data)[-3000:]!r}")


def close(process, master, slave):
    if process.poll() is None:
        process.kill()
    process.wait(timeout=5)
    os.close(master)
    os.close(slave)


with tempfile.TemporaryDirectory(prefix="letterkeeper-terminal-") as tmp:
    state = Path(tmp) / "private"
    process, master, slave, before = launch(["--state-dir", state, "tui", "--mail", ROOT / "tests/fixtures/welcome.eml", "--mail", ROOT / "tests/fixtures/second.eml"])
    try:
        receive(process, master, "欢迎使用".encode())
        os.write(master, b"j")
        receive(process, master, "▶".encode())
        os.write(master, b"\r")
        receive(process, master, "宽字符导航测试".encode())
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 24, 0, 0))
        os.kill(process.pid, signal.SIGWINCH)
        os.write(master, b"?")
        receive(process, master, "选择".encode())
        os.write(master, b"q")
        assert process.wait(timeout=5) == 0
        assert termios.tcgetattr(slave) == before, "terminal settings were not restored"
    finally:
        close(process, master, slave)

    created = subprocess.run([str(BINARY), "--state-dir", str(state), "draft", "create", str(ROOT / "tests/fixtures/draft.json")], check=True, capture_output=True)
    draft = json.loads(created.stdout)
    process, master, slave, _ = launch(["--state-dir", state, "draft", "approve", str(draft["id"])])
    try:
        receive(process, master, b"full digest")
        os.write(master, draft["digest"].encode() + b"\n")
        receive(process, master, b"approved")
        assert process.wait(timeout=5) == 0
    finally:
        close(process, master, slave)

print(json.dumps({"result": "passed", "tui_sizes": ["80x24", "24x12"], "checks": ["Chinese and wide text", "j/Enter/help/q", "resize", "terminal restore", "interactive digest approval"], "mail": "synthetic only"}, ensure_ascii=False))
