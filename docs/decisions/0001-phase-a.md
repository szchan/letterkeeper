# ADR 0001：首轮架构原型

日期：2026-10-12。状态：首轮原型已实现并通过所列合成验证；完整产品验收未完成，见 [STATUS](../STATUS.md)。

用户同意共享核心、安全文本、TUI/MCP、Rust 优先方案，并授权 Git 本地版本管理、worktree 隔离、按任务成本选择子代理、TDD 和开发记录。真实账户收发、公开发布及推送不在本轮范围。

采用 Rust workspace：content 处理有界 MIME/HTML；core 管理 SQLite 草稿、授权快照与发送状态；adapters 提供 TUI 和 MCP；应用负责受限 worker、装配和协议适配。首版不要求常驻 daemon。

解析使用 mail-parser 和 html5ever，无浏览器引擎。Linux 应用入口通过 bubblewrap 建立无网络、无用户文件/凭据的 worker，失败即拒绝解析。父进程保留正常邮件协议联网能力。原型 safe document 为纯文本、编号链接和附件元数据，不承诺完整 HTML 排版。

本轮先以合成 EML 和受控本地 SMTP 完成阅读、草稿、人工授权、发送结果不确定的闭环。真实 IMAP、Gmail OAuth、Sent/Drafts 同步、附件导出、公开 MCP 客户端的安全展示验证仍须后续工作；本轮不能称为可用于真实邮箱的版本。

TUI 使用 Ratatui；MCP 使用官方 Rust SDK stdio；SMTP 使用 lettre，禁用自动重试与任意主机的明文路径。原型允许显式测试模式向 loopback 测试服务投递合成邮件。固定依赖与许可证另见后续依赖记录。

当前发送批准由人工 CLI 路径产生，TUI 尚未提供发送审阅；MCP 无批准或改策略工具。授权绑定全部信封收件人、正文、附件字节和版本。相同 UID 的任意 shell 权限超出普通 MCP 权限边界。SQLite 单次认领不能为 SMTP 提供恰好一次语义，提交中断应保留不确定状态。

验收必须包括：单元边界、恶意 MIME/HTML、HTTP/DNS 观测、沙箱故障关闭、中文与宽字符布局、MCP 客户端协议交互、授权变更/并发、SMTP 接受后断连。每一模块先记录失败测试，再提交实现；未执行的验证明确列为未验证。


## 实现结果与资源界限

固定合成账户 `local-demo`，发送发件人为 `sender@example.test`，只允许 `.test`/`.invalid` 收件人及用户显式提供的 loopback SMTP 端点。这是独立测试路径，不能用作生产明文 SMTP 配置。生产 TLS/OAuth/凭据仍待下一阶段。

正文上限 2 MiB；输出共享 64 Ki Unicode 字符预算，最多 128 MIME parts、64 层 HTML DOM。HTML 构建期间的内存/CPU 由 worker 限制，深度检查包含丢弃的 template 内容。worker 限制 512 MiB 地址空间、5 秒 CPU、8 秒 wall time；MCP 输入帧在解码前最多 128 KiB，工具参数再限 64 KiB。

批准入口要求真实交互终端，展示全量草稿字段与附件元数据、完整摘要，确认后批准有效期五分钟。批准机制不声称能抵抗同 UID 任意 shell 模拟终端。MCP 只有受限 Host 的六个工具，没有本机路径访问、批准或策略修改入口。

代码依赖版本见 [依赖清单](../DEPENDENCIES.md)，测试结果见 [集成验证](../testing/integration.md)。运行数据暂放用户指定的私有状态目录，缺省为 cwd 的 `.local/letterkeeper`；未加密，正式 XDG 数据路径与凭据库仍需后续 ADR。
