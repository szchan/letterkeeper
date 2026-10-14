#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
test "${GITHUB_ACTIONS:-}" = true
archive="$RUNNER_TEMP/codex-0.162.1.tar.gz"
curl --fail --location --silent --show-error --retry 3 \
    https://github.com/openai/codex/releases/download/rust-v0.162.1/codex-x86_64-unknown-linux-musl.tar.gz \
    --output "$archive"
printf '%s  %s\n' 86f268d81b898f3e144c5ecff2ad2fda2c5802e042d6195b72538fe0fa125057 "$archive" | sha256sum --check
mkdir -p "$RUNNER_TEMP/letterkeeper-codex"
tar -xzf "$archive" -C "$RUNNER_TEMP/letterkeeper-codex"
"$RUNNER_TEMP/letterkeeper-codex/codex-x86_64-unknown-linux-musl" --version
