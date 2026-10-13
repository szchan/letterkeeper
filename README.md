# LetterKeeper

**守护你的信件，也守护你的隐私。**

LetterKeeper 是隐私优先、面向人和 Agent 的开源邮件客户端，目标是 IMAP/SMTP、Gmail OAuth、轻量 TUI 与本地 MCP 共用业务能力和隐私策略。

## 当前状态

2026-10-13：已完成阶段 B 的**受控协议原型**，可以通过严格隐式 TLS 的 IMAP/SMTP 服务完成文件夹、分页、搜索、安全阅读、回复/转发草稿、附件、人工批准发送和显式 Drafts/Sent 同步。账户配置、Linux Secret Service 凭据适配及 Gmail OAuth 代码已实现；认证验证使用本机合成 OAuth 服务与注入凭据。

真实 Gmail、真实系统凭据库往返和公开 MCP 客户端尚未验收，**不能称为全部 P0 已完成或生产可用**。实际完成项和缺口见 [验收状态](docs/STATUS.md)，目标见 [需求文档](docs/REQUIREMENTS.md)。本轮没有使用真实邮箱或外发邮件。

## 实现边界

- 六个 Rust crate 共用正文、账户、协议、草稿授权、TUI/MCP 和应用装配能力。
- 原文与头部经 bubblewrap worker；无正文网络、无用户目录挂载，限制大小、CPU、内存和时间；失败拒绝处理。TUI/MCP 输出保守文本，不提供原始 HTML 或链接目标。
- IMAP 使用 EXAMINE / BODY.PEEK 与 UIDVALIDITY；默认阅读不改变 Seen。账户、文件夹和权限由用户配置，MCP 启动时固定账户，模型无批准、配置、凭据或文件路径工具。
- 人工 CLI 完整审阅正文、收件人、线程头、附件元数据并输入完整摘要；批准绑定内容和版本，五分钟有效、单次消费。内容变化取消原批准。
- 发送保留精确 MIME 字节；DATA 后断连记录 Uncertain，不盲目重试。Sent 的 ServerManaged/Append 策略明确分开，Append 使用原发送字节且每提交仅认领一次。Accepted 只表示 SMTP 接受，Sent 查询仅提供观察证据。
- TUI 使用一个后台 worker，按需正文和 50 项页面，支持加载、取消结果、搜索、刷新和窗口变化。MCP 每会话最多 8 个 Host 任务，取消请求也保留正在运行任务的配额。

CLI `read` JSON 保留链接目标，不能交给会自动预览资源的组件。第三方 MCP 客户端的最终展示行为仍需逐一验证。本地 MCP 不代表本地模型处理：交给云端 Agent 的正文仍会传给模型服务商。

## 构建与离线演示

已验证：Arch x86_64、Rust/Cargo 1.99.0、bubblewrap 0.12.0。工作区声明 Rust 1.89（锁定依赖中有 1.89 要求；未单独验收最低版本）、C 编译器和 `/usr/bin/bwrap`。系统须允许用户/网络 namespace，worker 的 `/usr/lib` 布局目前仅在 Arch 验证。Linux 账户认证还需要已解锁的 Secret Service 与私有 `/run/user/UID`。

```sh
cargo build --locked
./target/debug/letterkeeper --help
./target/debug/letterkeeper read tests/fixtures/welcome.eml
./target/debug/letterkeeper tui --mail tests/fixtures/welcome.eml --mail tests/fixtures/second.eml
./target/debug/letterkeeper mcp --mail tests/fixtures/welcome.eml --read-only
```

TUI：`j/k` 或上下键选择/滚动，`Enter/Tab` 阅读，`Esc` 返回或丢弃加载结果，`r` 刷新，`n/p` 分页，`f` 切换文件夹，`/` 搜索，`?` 帮助，`q` 退出。取消不立即强行关闭底层连接；结果被丢弃，底层受 deadline 限制，退出不等待该工作线程。离线 EML 演示目前仍在进入 TUI 前预解析指定文件；账户 TUI 采用后台按需读取。

合成草稿（假设返回 id 为 1）：

```sh
./target/debug/letterkeeper draft create tests/fixtures/draft.json
./target/debug/letterkeeper draft show 1
./target/debug/letterkeeper draft approve 1
```

离线演示发送仅允许显式 `send 1 --smtp-test 127.0.0.1:端口` 与 `.test` / `.invalid` 身份；必须有实际测试服务。自动测试会自行启动服务。批准要求真实交互终端，管道和模型参数不能批准。

## 账户、CLI 与 MCP

[合成配置示例](tests/fixtures/account.example.json)展示配置形状，端口不是已启动的服务。`account import FILE` 导入不含凭据的配置，`account list` 列出本机账户。密码用 `account password ID` 的隐藏终端输入；Gmail 配置使用 `authentication.type=gmail_oauth`、自己的 Desktop client_id 及 client_secret_ref，后者启用时用 `account client-secret ID` 保存秘密。密码、token 和 client secret 均不放在普通配置或命令参数中，凭据库不可用时拒绝认证。

`account login ID` 打印供用户在系统浏览器打开的 OAuth URL，使用 S256 PKCE、state 和临时 loopback 回调；支持续期、`account logout ID` 本地注销及 `account revoke ID` 远程撤销。没有注册/借用其他客户端身份；个人和公开发布的 OAuth client 流程仍需真实验收。

配置后的账户操作用全局 `--account ID`：

- `mail folders` 返回允许的文件夹及不透明 id；`mail list --folder ID` 和 `mail search 查询 --folder ID` 使用该 id，不使用模型提供的任意文件夹名。中文查询使用 UTF-8 同步 literal；服务器不支持该字符集时明确报错。
- `mail read 消息ID` 安全读取；`mail reply 消息ID --body-file FILE` 创建回复；`--mode reply_all` 全部回复，`--mode forward --to 地址` 显式转发。
- `draft update ID --revision 版本 FILE` 乐观更新；`draft add-attachment ID FILE` 显式添加附件；`mail save-attachment 消息ID 部件ID DIRECTORY` 显式保存，不打开/执行附件。
- `draft approve ID` 人工审阅，`send ID` 严格 TLS 单次发送，`status 提交ID` 查询。真实账户不能混用 `--smtp-test`。
- `mail sync-draft 草稿ID` 显式版本快照追加；`mail sync-sent 提交ID` 仅用于 Append 策略。`mail sync-status 同步ID`、`mail sent-evidence 提交ID` 查询；`mail recover-send 提交ID` 在取得发送进程锁后把中断提交保留为 Uncertain。
- `mail mark-seen 消息ID` 是显式 CLI 操作，需要起草写权限；默认 TUI/MCP 不标记已读，MCP 不暴露此工具。

账户的 `tui` / `mcp` 使用同一 Host；`mcp --read-only` 可进一步收窄配置。MCP 有 14 个工具，工具参数不能改变账户、权限或端点；没有附件文件访问或批准工具。离线演示仅支持其中适用的工具。

账户默认配置/数据路径为 XDG_CONFIG_HOME/letterkeeper 与 XDG_DATA_HOME/letterkeeper（未设置时 ~/.config、~/.local/share）；可用 `--config-dir` / `--data-dir` 覆盖，每账户单独状态库。`--state-dir` 仅用于原离线演示，缺省 cwd 的 `.local/letterkeeper`。系统凭据刷新锁按 UID/账户固定在 `/run/user/UID/letterkeeper/credential-locks`，不随数据目录覆盖改变。

私有目录/文件要求 0700/0600，配置使用 descriptor walk 和原子替换；SQLite 的既有路径重新打开限制仍在。草稿数据库未加密。读取/发送原文上限 2 MiB，单附件导入/导出 512 KiB；SafeDocument 显示预算 64 Ki 字符，回复元数据另有字段限制。同步 Pending/Uncertain 不自动重试；目前明确 Failed 同步也不能重认领同一版本，旧服务器草稿不会自动删除。

## 验证与开发

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 scripts/verify_terminal.py target/debug/letterkeeper
python3 scripts/benchmark.py target/debug/letterkeeper
```

测试使用合成邮件、本机 TLS IMAP/SMTP/OAuth、HTTP/DNS 观察器与 PTY；不调用真实凭据库。全套测试须有 loopback、PTY 和 bubblewrap 权限，不能跳过隔离项后宣称通过；缓存依赖时可添加 `--offline`。真实 TLS 终端往返也包含在 Rust 集成测试中。

开发继续使用独立 worktree、TDD 和按任务选择模型的子代理，见 [协作流程](docs/WORKFLOW.md)。

## 文档与许可

- [需求](docs/REQUIREMENTS.md) · [验收状态](docs/STATUS.md) · [Agent 指引](AGENTS.md)
- [阶段 A 决定](docs/decisions/0001-phase-a.md) · [阶段 B 决定](docs/decisions/0002-phase-b.md)
- [阶段 A 记录](docs/development/2026-10-12.md) · [阶段 B 记录](docs/development/2026-10-13.md)
- [阶段 B 集成](docs/testing/phase-b-integration.md) · [协议](docs/testing/protocol.md) · [账户/OAuth](docs/testing/accounts.md)
- [正文/回复](docs/testing/composition.md) · [同步](docs/testing/sync.md) · [适配层](docs/testing/adapters-phase-b.md)
- [性能样本](docs/testing/performance.md) · [依赖及许可证](docs/DEPENDENCIES.md)

同 UID 任意 shell/文件写入权限超出普通 MCP 权限边界，隔离这类主体需要额外 OS 机制。原创代码与文档采用 **GPL-3.0-only**，见未修改的 [LICENSE](LICENSE)；第三方依赖按各自许可证处理，开源许可证本身不构成隐私保证。
