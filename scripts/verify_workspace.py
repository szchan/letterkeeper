#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""PTY smoke for the installed synthetic workspace; never opens a real account."""
import fcntl
import json
import os
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
FIXTURE = ROOT / "tests/fixtures/welcome.eml"


def receive(proc, master, marker, timeout=8):
    data = bytearray()
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if select.select([master], [], [], 0.05)[0]:
            try:
                data.extend(os.read(master, 65536))
            except OSError:
                break
        if marker in data:
            return bytes(data)
        if proc.poll() is not None:
            break
        assert len(data) < 512 * 1024, "unbounded terminal output"
    raise AssertionError(f"missing {marker!r}, exit={proc.poll()}, tail={data[-600:]!r}")


def run_size(width, height):
    with tempfile.TemporaryDirectory(prefix="letterkeeper-workspace-") as temp:
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        before = termios.tcgetattr(slave)
        proc = subprocess.Popen(
            [str(BINARY), "--state-dir", str(Path(temp) / "private"), "tui", "--mail", str(FIXTURE)],
            stdin=slave, stdout=slave, stderr=slave,
            env={**os.environ, "TERM": "xterm-256color"}, start_new_session=True,
        )
        try:
            receive(proc, master, "LetterKeeper".encode())
            os.write(master, b"c")
            receive(proc, master, "新建".encode())
            os.write(master, b"q")
            assert proc.poll() is None, "q exited the editor"
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", max(height - 1, 4), max(width - 1, 10), 0, 0))
            os.kill(proc.pid, signal.SIGWINCH)
            os.write(master, b"\x1b")
            receive(proc, master, "未保存".encode())
            os.write(master, b"d")
            os.write(master, b"q")
            assert proc.wait(timeout=5) == 0
            assert termios.tcgetattr(slave) == before, "termios was not restored"
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.wait(timeout=5)
            os.close(master)
            os.close(slave)


for size in [(120, 40), (80, 24), (40, 12), (20, 6)]:
    run_size(*size)
print(json.dumps({"result": "passed", "sizes": ["120x40", "80x24", "40x12", "20x6"],
                  "checks": ["editor q", "dirty exit", "resize", "termios"], "account": "synthetic demo"}))
