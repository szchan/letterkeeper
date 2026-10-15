# 发送诊断开发记录

基线：`5a813b2da1179b6115553df17969a943a89007cb`。目标：发送失败时保留安全的阶段信息，支持重启后的状态查询，避免仅凭 Failed 推断原因。决定见 [ADR 0006](../decisions/0006-send-diagnostics.md)。

主代理负责接口、集成及授权/隐私审查。三个 GPT-6.1-Sol high 子任务分别负责协议报告、核心原子持久化、账户/CLI/TUI 贯通；各有独立分支/worktree。子任务仅访问源码与合成测试，不访问私有真实数据。

| 单元 | 文件范围 | 远程证据 |
| --- | --- | --- |
| 协议阶段报告 | protocol crate 及独立测试记录 | [认证 RED 38709805368](https://github.com/szchan/letterkeeper/actions/runs/38709805368)、[TLS RED 38729860078](https://github.com/szchan/letterkeeper/actions/runs/38729860078)，精确 GREEN 与完整 CI 见[协议记录](../testing/send-diagnostics-protocol.md) |
| 核心诊断与迁移 | core crate 及独立测试记录 | [RED 38710063986](https://github.com/szchan/letterkeeper/actions/runs/38710063986)，精确 GREEN 与完整 CI 见[核心记录](../testing/send-diagnostics-core.md) |
| 账户与 TUI | application、workspace trait、human TUI 及独立测试记录 | [发送 RED 38709952540](https://github.com/szchan/letterkeeper/actions/runs/38709952540)、[查询 RED 38730779434](https://github.com/szchan/letterkeeper/actions/runs/38730779434)，最终精确 GREEN 与完整 CI 见[应用记录](../testing/send-diagnostics-app.md) |

本机仅编辑、Git、rustfmt/静态审查；所有 Rust 构建/测试/Clippy、PTY 与隐私验证在 GitHub Actions。新集成 revision 完整门禁通过之前，不把子分支通过当作集成验收。

实现不得扩大自动发送能力、记录敏感服务器文字或推断旧提交的失败阶段。真实邮箱复验另行进行；不以重新发信作为定位旧失败的办法。

主代理集成 SHA `8058877a61f59ab7e21173941f28aa330ba246d6` 的[完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38732704692) 全部门禁通过。应用状态查询故障另有独立行为 RED，修复后保留已知三态；受控 PTY 使用完整重绘断言，未放宽 SMTP 数值码或单次发送检查。最终源代码与模块边界已经审查，文档 revision 继续经 PR #17 最新门禁；Linux 构建只在托管 runner 生成，真实服务复验仍待执行。
