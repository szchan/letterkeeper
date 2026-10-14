#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
test "${GITHUB_ACTIONS:-}" = true
# Official public dist manifest checked on 2026-10-14; available since 2026-10-01.
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
rustup override set 1.99.0
rustc --version --verbose
cargo --version
