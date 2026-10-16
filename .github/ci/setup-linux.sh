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
# The observed Ubuntu RTM_NEWADDR denial comes from restricted user namespaces.
# On this disposable VM, replace only the system bwrap executable's named
# profile. All other profiles and global sysctls remain unchanged.
if test -r /proc/sys/kernel/apparmor_restrict_unprivileged_userns &&
   test "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns)" = 1; then
    test "$(cat /sys/module/apparmor/parameters/enabled)" = Y
    sudo tee /etc/apparmor.d/letterkeeper-ci-bwrap >/dev/null <<'PROFILE'
# SPDX-License-Identifier: GPL-3.0-only
abi <abi/4.0>,
include <tunables/global>
profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
}
PROFILE
    sudo apparmor_parser -r /etc/apparmor.d/letterkeeper-ci-bwrap
fi
python3 "$(dirname -- "${BASH_SOURCE[0]}")/preflight.py"
