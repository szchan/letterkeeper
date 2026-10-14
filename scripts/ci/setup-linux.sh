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
if ! python3 scripts/ci/preflight.py; then
    # Ubuntu 24.04 restricts userns and its bwrap profile may forbid nesting.
    # Change only bwrap's executable profile; never disable AppArmor/sysctl globally.
    test "$(cat /sys/module/apparmor/parameters/enabled)" = Y
    for profile in /etc/apparmor.d/bwrap /etc/apparmor.d/bwrap-userns-restrict /etc/apparmor.d/bubblewrap; do
        if test -f "$profile"; then
            sudo apparmor_parser -R "$profile"
        fi
    done
    sudo tee /etc/apparmor.d/letterkeeper-ci-bwrap >/dev/null <<'PROFILE'
# SPDX-License-Identifier: GPL-3.0-only
abi <abi/4.0>,
include <tunables/global>
profile letterkeeper-ci-bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
}
PROFILE
    sudo apparmor_parser -r /etc/apparmor.d/letterkeeper-ci-bwrap
    python3 scripts/ci/preflight.py
fi
