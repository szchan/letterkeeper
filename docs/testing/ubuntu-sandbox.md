# Ubuntu hosted worker runtime

2026-10-14. GitHub hosted Ubuntu 24.04, pinned Rust 1.99.0; synthetic data only.

## RED

At `2f45f4fbba7f1331ed1eea09ce8ba6310bfe9ca8`, [full CI](https://github.com/szchan/letterkeeper/actions/runs/38534928861) passed namespace preflight, compilation and Clippy, then failed the MCP stdio integration and Codex server connection. [Exact-test TDD](https://github.com/szchan/letterkeeper/actions/runs/38534969060) selected `isolated_worker_reads_chinese_without_active_content` exactly once and failed with `sandbox_or_parse_failed`, Cargo exit 101. No tests were skipped or failures converted to success.

The existing worker reconstructed `/lib64` as `usr/lib`, which matches the verified Arch layout but does not preserve Ubuntu's loader location. The repair read-only mounts the actual `/lib` and, when present, `/lib64` alongside `/usr`. The x86_64 Codex and display observer scripts use the same real library directories. Namespace isolation, resource limits, empty environment, no home mount and refusal on worker failure remain in force; no host system library layout or AppArmor policy is altered.

## GREEN

移除最初过宽的 AppArmor fallback 后，[PR 环境预检](https://github.com/szchan/letterkeeper/actions/runs/38535568409) 与 [GREEN 尝试](https://github.com/szchan/letterkeeper/actions/runs/38535628451) 在进入 Cargo 前因 Ubuntu 的 `RTM_NEWADDR: Operation not permitted` 失败。这些是环境失败，不作为测试断言 RED。针对已观察到的限制，runner 安装脚本仅替换 `/usr/bin/bwrap` 的同名 namespace 例外 profile，保留其他 AppArmor profile 和全局 sysctl，并新增零有效 capabilities 预检。本机和应用不修改系统策略。

Pending exact-test and complete GitHub Actions runs. The existing sandbox suite covers safe Chinese parsing, oversize input, missing isolation, direct worker rejection, and reachable HTTP/DNS plus host-file privacy. The full CI also covers protocol, terminal and public Codex paths. No local Cargo compilation or testing was performed.

At `7e923f414c8f555cb8e5f88a1d17cc2bb7d90738`, [exact-test GREEN](https://github.com/szchan/letterkeeper/actions/runs/38536070896) matched the original sandbox test once and returned 0; the complete CI's Codex job also passed, while fmt correctly rejected the new Rust argument layout. The formatting was corrected using the remote diff.

An independent review identified inherited runner environment in client verification. At `79b447b`, [actual child-environment RED](https://github.com/szchan/letterkeeper/actions/runs/38536633653) injected a synthetic ambient auth variable and failed `client_environment_not_cleared` inside the real bwrap prefix. The repair clears all variables at the sandbox boundary and sets only HOME, PATH and LANG. The observer sandbox uses the same clean-environment policy; its TUI child explicitly adds TERM. No real auth variable value is logged. This run also exposed a separate terminal fixture resize/help failure, tracked independently.
