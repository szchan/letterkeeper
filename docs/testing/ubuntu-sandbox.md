# Ubuntu hosted worker runtime

2026-10-14. GitHub hosted Ubuntu 24.04, pinned Rust 1.99.0; synthetic data only.

## RED

At `2f45f4fbba7f1331ed1eea09ce8ba6310bfe9ca8`, [full CI](https://github.com/szchan/letterkeeper/actions/runs/38534928861) passed namespace preflight, compilation and Clippy, then failed the MCP stdio integration and Codex server connection. [Exact-test TDD](https://github.com/szchan/letterkeeper/actions/runs/38534969060) selected `isolated_worker_reads_chinese_without_active_content` exactly once and failed with `sandbox_or_parse_failed`, Cargo exit 101. No tests were skipped or failures converted to success.

The existing worker reconstructed `/lib64` as `usr/lib`, which matches the verified Arch layout but does not preserve Ubuntu's loader location. The repair read-only mounts the actual `/lib` and, when present, `/lib64` alongside `/usr`. The x86_64 Codex and display observer scripts use the same real library directories. Namespace isolation, resource limits, empty environment, no home mount and refusal on worker failure remain in force; no host system library layout or AppArmor policy is altered.

## GREEN

移除最初过宽的 AppArmor fallback 后，[PR 环境预检](https://github.com/szchan/letterkeeper/actions/runs/38535568409) 与 [GREEN 尝试](https://github.com/szchan/letterkeeper/actions/runs/38535628451) 在进入 Cargo 前因 Ubuntu 的 `RTM_NEWADDR: Operation not permitted` 失败。这些是环境失败，不作为测试断言 RED。针对已观察到的限制，runner 安装脚本仅替换 `/usr/bin/bwrap` 的同名 namespace 例外 profile，保留其他 AppArmor profile 和全局 sysctl，并新增零有效 capabilities 预检。本机和应用不修改系统策略。

Pending exact-test and complete GitHub Actions runs. The existing sandbox suite covers safe Chinese parsing, oversize input, missing isolation, direct worker rejection, and reachable HTTP/DNS plus host-file privacy. The full CI also covers protocol, terminal and public Codex paths. No local Cargo compilation or testing was performed.
