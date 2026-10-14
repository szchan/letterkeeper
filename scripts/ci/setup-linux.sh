#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
# Never changes the developer's host. Only an ephemeral hosted VM is authorized.
test "${GITHUB_ACTIONS:-}" = true
test "${RUNNER_ENVIRONMENT:-}" = github-hosted
sudo apt-get update
sudo apt-get install --no-install-recommends -y bubblewrap apparmor build-essential pkg-config python3 curl ca-certificates
uname -a
lsb_release -ds
/usr/bin/bwrap --version
# Runner image changes that break namespaces are visible failures, never skips.
# Do not rewrite profiles or disable AppArmor/sysctl based on an arbitrary error.
python3 scripts/ci/preflight.py
