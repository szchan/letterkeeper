# LetterKeeper

**守护你的信件，也守护你的隐私。**

LetterKeeper 是隐私优先、面向人和 Agent 的开源邮件客户端。目标是标准 IMAP/SMTP、Gmail OAuth、轻量 TUI 与本地 MCP 共用业务能力和隐私策略。

## 当前状态

2026-10-12：已完成首轮 **Linux/Arch 离线架构原型**。目前可以打开用户指定的合成 EML、在 TUI/MCP 中读取安全正文、持久化草稿、人工审阅批准，并向 loopback SMTP 测试服务发送合成邮件。

**尚不是可用于真实邮箱的版本。** IMAP、Gmail OAuth、真实账户凭据存储、服务端 Drafts/Sent 同步、搜索、完整回复线程、附件导出和受限自动发送尚未实现。全部 P0 的状态见 [验收状态](docs/STATUS.md)，产品目标见 [需求文档](docs/REQUIREMENTS.md)。

## 已实现的原型边界

- **共享核心**：Rust workspace，正文、草稿授权与 TUI/MCP 分层；SQLite 保存草稿版本、单次批准和发送状态。
- **安全阅读**：mail-parser / html5ever 解析，正文先经过独立 bubblewrap worker；无正文网络、无用户目录挂载，限制原文、输出、CPU、内存和运行时间。隔离失败即拒绝处理。
- **TUI / MCP**：中文文本、键盘导航及保守文本输出；MCP stdio 使用官方 SDK，在 JSON 解码前限制输入帧，并拒绝未知参数。固定账户 `local-demo`，可选择只读模式。
- **人工发送授权**：终端完整审阅后输入草稿摘要；批准绑定版本、账户、所有收件人、正文和附件字节，五分钟有效、单次消费。MCP 没有批准或改策略工具。
- **不确定发送**：受控 SMTP 在 DATA 后断连会持久化为 `Uncertain`，重启后仍禁止盲目重试。`Accepted` 只表示 SMTP 服务接受，不代表最终送达。

CLI 的安全 JSON 保留链接目标，供明确操作使用；MCP/TUI 默认去活化链接及标记。不能把 CLI JSON 的字符串交给会自动预览资源的组件。第三方 MCP 客户端的展示行为仍需逐一验证。

## 构建与运行

已验证环境：Arch x86_64、Rust/Cargo 1.99.0、bubblewrap 0.12.0。需要 Rust 工具链、C 编译器（编译 bundled SQLite）和 `/usr/bin/bwrap`，且系统允许创建用户/网络 namespace。当前 worker 使用 Arch 的 `/usr/lib` 布局，其他发行版与平台尚未验证。

```sh
cargo build --locked
./target/debug/letterkeeper --help
./target/debug/letterkeeper read tests/fixtures/welcome.eml
./target/debug/letterkeeper tui --mail tests/fixtures/welcome.eml --mail tests/fixtures/second.eml
```

TUI：`j/k` 或上下键选择，`Enter/Tab` 切换阅读，`Esc` 返回，`?` 帮助，`q` 退出。第一轮界面预加载最多 50 封本地邮件，尚无网络同步、编辑器或取消加载。

启动本地 MCP（stdout 仅供 MCP 协议使用）：

```sh
./target/debug/letterkeeper mcp --mail tests/fixtures/welcome.eml --read-only
```

省略 `--read-only` 可允许创建草稿；发送仍须已有人工批准和启动时显式配置的 loopback 测试端点。工具及权限见 [架构决定](docs/decisions/0001-phase-a.md)与[适配层记录](docs/testing/adapters.md)。

草稿示例（假设创建结果的 `id` 为 `1`）：

```sh
./target/debug/letterkeeper draft create tests/fixtures/draft.json
./target/debug/letterkeeper draft show 1
./target/debug/letterkeeper draft approve 1
```

`approve` 要求交互式终端，显示快照后需输入完整摘要；管道或模型参数不能批准。批准后，可使用 `send 1 --smtp-test 127.0.0.1:端口` 向已启动的测试 SMTP 服务提交；该命令要求实际存在的端点，仓库的自动测试会自行启动受控服务。运行时只允许 `.test` / `.invalid` 收件人和固定合成发件人。`status 提交ID` 查询结果。

原型默认状态目录为当前工作目录下的 `.local/letterkeeper/`，可用全局 `--state-dir` 改变。数据库与目录要求 0600/0700；现有宽松权限会被拒绝，不会静默改动。当前草稿数据库未加密；正式运行数据路径、缓存及凭据库将在账户阶段确定。

## 验证与开发

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 scripts/verify_terminal.py target/debug/letterkeeper
python3 scripts/benchmark.py target/debug/letterkeeper
```

测试只使用合成数据与本机 SMTP/HTTP/DNS 服务。全套测试需要可用的 loopback、PTY 和 bubblewrap namespace；受限 CI/执行沙箱中必须提供相应测试权限，不能跳过隔离测试后宣称通过。依赖已缓存时可添加 `--offline`。

开发采用独立 Git worktree、TDD 的 RED/GREEN 提交与模块审查，见 [协作与 TDD 流程](docs/WORKFLOW.md)。

## 文档

- [需求与稳定编号](docs/REQUIREMENTS.md) · [验收状态与缺口](docs/STATUS.md)
- [架构决定](docs/decisions/0001-phase-a.md) · [开发记录](docs/development/2026-10-12.md)
- [集成验证](docs/testing/integration.md) · [性能样本](docs/testing/performance.md)
- [正文测试](docs/testing/content.md) · [授权测试](docs/testing/core.md) · [TUI/MCP 测试](docs/testing/adapters.md)
- [依赖与许可证](docs/DEPENDENCIES.md) · [Agent 开发指引](AGENTS.md)

## 隐私边界与许可证

邮件服务商可知道账户连接和服务端状态；交给云端 Agent 的邮件内容仍会传给模型服务商。本地 MCP 不意味着本地模型处理。拥有同一系统用户任意 shell/文件写入权限的 Agent 超出普通 MCP 授权边界；隔离这类主体需要额外操作系统机制。

原创源码与文档使用 **GNU GPL 第 3 版，仅此版本**（`GPL-3.0-only`），见未修改的 [LICENSE](LICENSE)。第三方依赖保留各自版权和许可证；开源许可证本身不构成隐私或安全保证。
