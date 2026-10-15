# 发送诊断开发记录

基线：`5a813b2da1179b6115553df17969a943a89007cb`。目标：发送失败时保留安全的阶段信息，支持重启后的状态查询，避免仅凭 Failed 推断原因。决定见 [ADR 0006](../decisions/0006-send-diagnostics.md)。

主代理负责接口、集成及授权/隐私审查。三个 GPT-6.1-Sol high 子任务分别负责协议报告、核心原子持久化、账户/CLI/TUI 贯通；各有独立分支/worktree。子任务仅访问源码与合成测试，不访问私有真实数据。

| 单元 | 文件范围 | 远程证据 |
| --- | --- | --- |
| 协议阶段报告 | protocol crate 及独立测试记录 | RED/GREEN 待完成 |
| 核心诊断与迁移 | core crate 及独立测试记录 | RED/GREEN 待完成 |
| 账户与 TUI | application、workspace trait、human TUI 及独立测试记录 | RED/GREEN 待完成 |

本机仅编辑、Git、rustfmt/静态审查；所有 Rust 构建/测试/Clippy、PTY 与隐私验证在 GitHub Actions。新集成 revision 完整门禁通过之前，不把子分支通过当作集成验收。

实现不得扩大自动发送能力、记录敏感服务器文字或推断旧提交的失败阶段。真实邮箱复验另行进行；不以重新发信作为定位旧失败的办法。
