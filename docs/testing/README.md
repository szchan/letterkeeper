# 验证记录索引

本目录中的报告均为**历史验证记录**：它们保留当时的目标 SHA、命令、结果和环境，不自动证明当前集成 revision 通过。当前产品状态以 [STATUS](../STATUS.md) 为准；主代理集成后的完整门禁结果应按对应 SHA 单独确认。

## 架构阶段与集成

- [历史验证：阶段 B 集成](phase-b-integration.md)：阶段 B 工作区及协议回归。
- [历史验证：阶段 C 集成](phase-c-integration.md)：TUI、MCP、发送审批与安全边界。
- [历史验证：完整集成](integration.md)：早期集成基线。
- [历史验证：工作区工作流](workflow-phase-c.md)：草稿、批准和发送状态转换。

## 账户、协议与内容

- [历史验证：账户与 OAuth](accounts.md)：账户配置、凭据与 OAuth。
- [历史验证：账户 CLI](account-cli.md)：账户命令行行为。
- [历史验证：账户诊断](account-diagnostics.md)：只读环境诊断。
- [历史验证：账户引导](account-onboarding.md)：安全创建配置与取消边界。
- [历史验证：协议](protocol.md)：IMAP/SMTP/TLS 行为。
- [历史验证：状态同步](sync.md)：持久状态与同步边界。
- [历史验证：邮件正文](content.md)：MIME/HTML 解析与隐私。
- [历史验证：回复与合成](composition.md)：回复/转发与附件处理。
- [历史验证：核心状态](core.md)：核心存储和授权逻辑。
- [历史验证：适配层](adapters.md)：CLI/TUI/MCP 适配基础。
- [历史验证：阶段 B 适配层](adapters-phase-b.md)：搜索、回复与 MCP 工具。

## TUI、MCP 与隐私

- [历史验证：阶段 C TUI](tui-phase-c.md)：状态机、编辑与终端交互。
- [历史验证：正文隐私工作区](privacy-workspace-phase-c.md)：受控 DNS/HTTP 正向控制和正文无外联。
- [历史验证：Codex 阶段 C](codex-phase-c.md)：实际 MCP 客户端数据路径。
- [历史验证：Codex 托管运行](codex-hosted.md)：托管客户端准备与结果。
- [历史验证：终端托管修复](hosted-terminal.md)：PTY 与窗口缩放观察。
- [历史验证：发送诊断 TUI/CLI 集成](send-diagnostics-app.md)：诊断展示和命令入口。

## 持续集成与重构任务

- [历史验证：托管 CI](ci.md)：Actions 门禁、TDD 与发布产物边界。
- [历史验证：配置兼容](gmail-config-compat.md)：Gmail 配置别名回归。
- [历史验证：有界分页](imap-bounded-paging.md)：IMAP 分页/搜索预算。
- [历史验证：发送诊断集成](send-diagnostics.md)：诊断 feature 的集成证据。
- [历史验证：发送诊断核心](send-diagnostics-core.md)：状态库诊断持久化。
- [历史验证：发送诊断协议](send-diagnostics-protocol.md)：SMTP 阶段与诊断类型。
- [历史验证：发送诊断应用](send-diagnostics-app.md)：应用层查询与 UI 展示。

## 真实服务与环境关卡

- [历史验证：真实服务验收总览](live-acceptance.md)：真实账户操作授权范围。
- [历史验证：GNOME Keyring 与 OAuth](live-keyring-2026-10-15.md)：本机凭据服务与 Google OAuth。
- [历史验证：Gmail 只读 IMAP](live-imap-2026-10-15.md)：认证、分页、搜索、抽样与 Flags 检查。
- [历史验证：续期与真实写入](live-write-2026-10-15.md)：续期、草稿同步与本人投递闭环。
- [历史验证：Ubuntu worker 沙箱](ubuntu-sandbox.md)：托管 Ubuntu 环境和限制。
- [历史验证：公开仓库审计](publication-audit.md)：公开前检查和边界。

## 性能记录与原始数据

- [历史验证：原型性能](performance.md)：阶段 A/B 的本机 debug 样本。
- [历史验证：阶段 C 性能](performance-phase-c.md)：release TUI/MCP 合成工作负载。
- [历史验证：托管性能](hosted-performance.md)：GitHub runner 性能样本。
- [历史数据：托管性能 JSON](hosted-performance.json)：阶段 C 原始合成测量数据。
- [历史数据：阶段 A 托管性能 JSON](hosted-performance-phase-a.json)：阶段 A 原始合成测量数据。

- [结构整理静态测试声明清单](structure-test-inventory.json)：基线/重组源码声明与迁入 benches 的负载；实际执行证据见[集成记录](../development/2026-10-16-project-structure.md)。
