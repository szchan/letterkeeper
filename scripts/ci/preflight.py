#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise actual unprivileged/nested bubblewrap, mounts and network isolation."""
from pathlib import Path
import subprocess


def sandbox():
    arguments = ["/usr/bin/bwrap", "--unshare-all", "--die-with-parent", "--new-session",
                 "--cap-drop", "ALL", "--ro-bind", "/usr", "/usr",
                 "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]
    for directory in ("/lib", "/lib64"):
        if Path(directory).exists():
            arguments += ["--ro-bind", directory, directory]
    return arguments


if __name__ == "__main__":
    probe = ["/usr/bin/python3", "-c",
             "import os; from pathlib import Path; "
             "assert not Path('/home/runner').exists(); "
             "assert open('/proc/net/dev').read().count(':') == 1; print('sandbox_preflight_passed')"]
    subprocess.run(sandbox() + sandbox() + probe, check=True, timeout=30)
