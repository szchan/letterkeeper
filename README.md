# LetterKeeper

**守护你的信件，也守护你的隐私。**

LetterKeeper 是隐私优先、面向人和 Agent 的开源邮件客户端。它以 Rust workspace 实现 IMAP/SMTP、Gmail OAuth、TUI、CLI 与本地 MCP，并让这些入口共用业务能力和隐私策略。

## 状态

阶段 C 的受控 TUI/Agent 人工审批工作流已完成，账户、OAuth、收发与真实服务的部分关卡也已验证。真实撤销、完整凭据服务故障关卡、公开图形前端、客户端 F9 触发和受限自动发送仍待验收；项目不能称为全部 P0 已完成或生产可用。逐项状态、证据和限制见 [验收状态](docs/STATUS.md)。

## 快速开始

在支持 bubblewrap、用户/网络 namespace 与 Secret Service 的 Linux 环境中，先从 [Actions 验证构建](docs/testing/ci.md) 获取对应提交的构建产物，再按 [使用指南](docs/user-guide.md) 运行。离线帮助示例：

```sh
./target/debug/letterkeeper --help
./target/debug/letterkeeper read tests/fixtures/welcome.eml
./target/debug/letterkeeper tui --read-only --mail tests/fixtures/welcome.eml
```

本机开发会话不运行 Cargo build/check/test/Clippy 或 release 基准；这些工作由 GitHub Actions 执行。操作限制与离线演示见 [使用指南](docs/user-guide.md)。

## 文档

- [文档总览](docs/README.md)：需求、用户指南、架构决定、开发与验证记录索引。
- [需求](docs/REQUIREMENTS.md) · [验收状态](docs/STATUS.md) · [开发协作流程](docs/WORKFLOW.md)
- [架构决定](docs/decisions/README.md) · [开发记录](docs/development/README.md) · [验证记录](docs/testing/README.md)
- [许可证](LICENSE)：GPL-3.0-only；开源许可证本身不构成隐私或安全保证。
