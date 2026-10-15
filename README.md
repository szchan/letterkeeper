# LetterKeeper

**守护你的信件，也守护你的隐私。**

LetterKeeper 是隐私优先、面向人和 Agent 的开源邮件客户端，目标是 IMAP/SMTP、Gmail OAuth、轻量 TUI 与本地 MCP 共用业务能力和隐私策略。

## 当前状态

账户接入新增 `doctor` 环境诊断和 `account add` 交互配置：默认只读、确认后原子创建、拒绝同名覆盖；配置保存与账户认证分开。诊断不读取凭据，也不初始化账户目录。该批远程验收见 [账户接入记录](docs/development/2026-10-14-account-onboarding.md)。

2026-10-14：已完成阶段 C 的**受控 TUI / Agent 人工审批工作流**及账户接入引导增量。严格 TLS IMAP/SMTP、账户/OAuth、分页搜索、安全阅读、回复/转发、附件与显式 Drafts/Sent 同步继续共用后端；内置草稿编辑、完整审阅、人工批准、独立发送及结果查询保留。

272 项已登记 Rust 测试、fmt/Clippy、实际 PTY、可达 HTTP/DNS 观察器和实际 Codex MCP 数据路径已通过；新增发送诊断的 RED/GREEN 与完整集成结果见[诊断验证](docs/testing/send-diagnostics.md)；配置兼容与有界分页的 RED/GREEN 见[配置兼容](docs/testing/gmail-config-compat.md)和[分页回归](docs/testing/imap-bounded-paging.md)，后续集成 revision 仍须最新门禁通过。2026-10-15 已完成[本机 GNOME Keyring 与 Google OAuth](docs/testing/live-keyring-2026-10-15.md)和[真实 Gmail 只读 IMAP 认证、分页、搜索、正文抽样及 Flags 不变检查](docs/testing/live-imap-2026-10-15.md)。后续[真实续期、服务器草稿写入与 CLI SMTP/Sent/本人投递闭环](docs/testing/live-write-2026-10-15.md)已通过；网络恢复后经新的 TUI 人工批准完成单次发送，原失败记录保留。用户确认 Gmail 网页上发送对应的草稿已消失、另一份仍在；不将其记为草稿刷新故障。Hyprland 听写快捷键截获 F9，真实客户端 F9 仍未验收。真实撤销、完整凭据服务故障关卡、公开图形前端及受限自动发送仍待验收，**不能称为全部 P0 已完成或生产可用**。实际完成项和缺口见 [验收状态](docs/STATUS.md)。

## 实现边界

- 六个 Rust crate 共用正文、账户、协议、草稿授权、TUI/MCP 和应用装配能力。
- 收取的原文与头部经 bubblewrap worker；无正文网络、无用户目录挂载，限制大小、CPU、内存和时间；失败拒绝处理。阅读与 MCP 输出保守文本；草稿编辑和人工审阅显示完整终端安全文本，不解释 HTML 或加载资源。
- IMAP 使用 EXAMINE / BODY.PEEK 与 UIDVALIDITY；默认阅读不改变 Seen。账户、文件夹和权限由用户配置，MCP 启动时固定账户，模型无批准、配置、凭据或文件路径工具。
- IMAP 分页/搜索每批扫描最多 4096 个消息序号、每次列表最多 128 批，收齐页面后停止；64 KiB 行、8 MiB 响应与 15 秒协议操作界限保留。大 offset/稀疏搜索超过预算会报错，邮箱发生变化需刷新，不承诺跨调用快照。
- 人工 CLI/TUI 完整审阅正文、全部收件人、线程头和附件元数据；TUI 另显示附件字节 SHA-256。输入完整摘要批准，绑定内容和版本，五分钟有效、单次消费；内容变化取消原批准。批准后另行发送。
- 发送保留精确 MIME 字节；DATA 后断连记录 Uncertain，不盲目重试。Sent 的 ServerManaged/Append 策略明确分开，Append 使用原发送字节且每提交仅认领一次。Accepted 只表示 SMTP 接受，Sent 查询仅提供观察证据。
- 发送诊断仅包含固定阶段、错误种类和可选 SMTP 数值码，与终态原子保存；不保存服务器原始错误文字。CLI 状态查询和 TUI 共用诊断，旧记录没有原因时显示未知，诊断读取故障不触发重发。增量验证见[发送诊断](docs/testing/send-diagnostics.md)。
- TUI 使用各一个网络/本地后台 worker、有界通道、按需正文和 50 项页面；本地编辑保存不等待网络。只在事件/结果/窗口变化时重绘。MCP 每会话最多 8 个 Host 任务，取消请求也保留正在运行任务的配额。

CLI `read` JSON 保留链接目标，不能交给会自动预览资源的组件。第三方 MCP 客户端的最终展示行为仍需逐一验证。本地 MCP 不代表本地模型处理：交给云端 Agent 的正文仍会传给模型服务商。

## 远程构建与离线演示

2026-10-14 起，用户授权建立公开仓库 [szchan/letterkeeper](https://github.com/szchan/letterkeeper)，并将重型构建和验证迁移到 GitHub Actions。本机保留编辑、Git/worktree、静态检查与结果审查，不执行 Cargo build/check/test/Clippy、release 基准或安装编译；编辑器自动 Cargo check、保存时构建也应在当前项目会话关闭，不能由 Agent 修改无关系统配置。

公开仓库已建立，main 保护已启用：必须通过 PR、最新 `ci-gate`，管理员同样受约束，禁止强推和删除。首次 workflow bootstrap 使用了未保护的 main；后续开发通过独立分支和 PR 集成。[集成分支完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38539664364) 已通过，精确版本和后续结果见[开发记录](docs/development/2026-10-14-hosted-ci.md)。

每个子任务使用独立分支/worktree；先推送测试提交，在远程记录精确 SHA 的真实 RED，再实现和推送 GREEN。测试、PTY、HTTP/DNS 隐私观察器、实际 Codex 数据路径和性能测量均在远程受控环境运行，不能删减安全测试以适配 runner。操作入口及环境要求见 [CI 验证](docs/testing/ci.md) 与 [协作流程](docs/WORKFLOW.md)。远程通过之前，阶段 C 的本机历史结果不能视为新环境通过。

以下 Cargo 命令供 **远程 runner** 使用；手动构建 workflow 的对应 commit 产物完成且可下载后，可在兼容 Linux 环境运行后续离线命令。workflow 产物属于该提交的验证构建，不表示发行版已经发布；未生成产物时不能声称已有可下载构建。不要在本机为演示重新编译。

已验证：Arch x86_64，以及配置了专用 bwrap namespace 例外的 GitHub-hosted Ubuntu 24.04.5；Rust/Cargo 1.99.0，bubblewrap 分别为 0.12.0 / 0.9.0。工作区声明 Rust 1.89（未单独验收最低版本）、C 编译器和 `/usr/bin/bwrap`。系统须允许用户/网络 namespace；worker 只读挂载系统实际库目录，隔离不可用时拒绝处理。[Ubuntu 验证边界](docs/testing/ubuntu-sandbox.md)不代表默认桌面策略已验收。Linux 账户认证还需要已解锁的 Secret Service 与私有 `/run/user/UID`。

```sh
cargo build --locked
./target/debug/letterkeeper --help
./target/debug/letterkeeper read tests/fixtures/welcome.eml
./target/debug/letterkeeper tui --mail tests/fixtures/welcome.eml --mail tests/fixtures/second.eml
./target/debug/letterkeeper tui --read-only --mail tests/fixtures/welcome.eml
./target/debug/letterkeeper mcp --mail tests/fixtures/welcome.eml --read-only
```

TUI：`j/k` 或上下键选择/滚动，`Enter/Tab` 阅读，`r` 刷新，`n/p` 分页，`f` 文件夹，`/` 搜索，`?` 帮助。宽窗口采用文件夹/列表/正文布局，窄窗口切换单页。

`c` 新建、`d` 本地草稿、`s` 提交记录，阅读页 `R/A/F` 回复/全部回复/转发，`x` 显式保存邮件附件。编辑器中 `Tab` 切换 To/Cc/Bcc/主题/正文，`Ctrl+S` 保存，`F2/F3` 添加/移除附件，`F4` 显式草稿同步；`q` 是普通字符。`Esc` 返回和 `Ctrl+Q` 退出会检查未保存内容；冲突保留输入，可重新加载或另存新草稿。

`F6` 进入完整、可滚动审阅；`F8` 输入完整摘要批准，`F9` 单独发送。提交页 `Enter` 查询、`e` Sent 观察证据、`u` 中断恢复、`F4` 显式 Sent 同步，`v` 查询最近同步。Accepted 仅代表 SMTP 接受，Uncertain 不自动重发。

离线草稿工作区可直接运行 `tui`，不必指定 EML。离线 EML 模式仍预解析最多 50 个文件，回复/导出和服务器同步功能需账户后端；账户 TUI 后台按需读取。正文编辑上限 1 MiB、撤销 20 次，危险粘贴拒绝。浏览取消丢弃过期结果，不强行终止连接；退出不宣称正在发送已取消，重启后查询持久化状态。
合成草稿（假设返回 id 为 1）：

```sh
./target/debug/letterkeeper draft create tests/fixtures/draft.json
./target/debug/letterkeeper draft show 1
./target/debug/letterkeeper draft approve 1
```

离线演示发送仅允许显式 `send 1 --smtp-test 127.0.0.1:端口` 与 `.test` / `.invalid` 身份；必须有实际测试服务；离线 TUI 可显式使用 `tui --smtp-test 127.0.0.1:端口`。自动测试会自行启动服务。批准要求真实交互终端，管道和模型参数不能批准。

## 账户、CLI 与 MCP

下载包含本批功能的 Actions 构建并解包后，可先诊断再创建配置：

```sh
./letterkeeper doctor
./letterkeeper account add
```

`doctor` 输出固定字段 JSON 和安全建议，检查私有配置/数据目录、固定 `/run/user/UID`、合成邮件的实际沙箱 worker，以及默认凭据 collection 的锁定元数据；可选系统工具为 `/usr/bin/busctl`。不创建目录、不搜索凭据条目、不解锁服务、不连接邮箱。环境未就绪时退出非零，`authentication: not_checked` 始终表示认证未检查；沙箱 ready 只表示 worker 能启动并解析，完整网络隔离另由隐私测试验证。

`account add` 要求输入和输出都是真实终端，支持 `/cancel` 或 EOF 取消。选择密码账户或自己的 Gmail Desktop client ID，填写文件夹并选择权限，默认只读。最终审阅后只有精确输入 `yes` 才保存，空白、`y` 或其他回答均取消且不初始化存储。创建同名账户会拒绝覆盖，`account import` 仍用于显式导入/更新配置。保存结果为 `configuration_saved`、`authentication: unverified`，并提示后续凭据和登录命令；引导本身不收集密码/token/client secret、不创建账户数据库、不启动 OAuth 或网络认证。

引导目前接受 ASCII dot-atom 邮箱地址、DNS/IP 端点及中文文件夹；带引号或国际化邮箱地址不在该入口支持范围。隐式 TLS 默认 IMAP 993 / SMTP 465，不把其他端口自动当作 STARTTLS。Gmail 默认文件夹名可覆盖，实际标签和非 ASCII mailbox 协议兼容性仍待独立验收。显式选择发送权限仍需既有人工批准流程。

[合成配置示例](tests/fixtures/account.example.json)展示配置形状，端口不是已启动的服务。`account import FILE` 导入不含凭据的配置，`account list` 列出本机账户。新建账户优先使用 `account add` 的类型化引导，避免手写配置标签。密码用 `account password ID` 的隐藏终端输入；Gmail 手写配置接受 `authentication.type=gmail_oauth` 或兼容的 `gmail_o_auth`，保存时仍输出 `gmail_o_auth`。填写自己的 Desktop client_id 及 client_secret_ref，后者启用时用 `account client-secret ID` 保存秘密。密码、token 和 client secret 均不放在普通配置或命令参数中，凭据库不可用时拒绝认证。

旧 `64dd635` 下载构建的手写配置仅接受 `gmail_o_auth`；其 `account add` 可正常生成配置，已用于上述本机验收。别名兼容修复需使用包含该变更的新构建。

旧构建的大邮箱列表也可能因全量 SEARCH 响应超过上限而失败；`5fde670` 验证构建已修复并通过真实只读 IMAP 重验，下载来源和二进制校验值见[验收记录](docs/testing/live-imap-2026-10-15.md)。本机新旧构建分别保留。

`account login ID` 打印供用户在系统浏览器打开的 OAuth URL，使用 S256 PKCE、state 和临时 loopback 回调；支持续期、`account logout ID` 本地注销及 `account revoke ID` 远程撤销。使用用户自己的 Desktop client；本机个人 OAuth 登录、保存及自然到期后的续期已通过；真实撤销及公开发布的 OAuth client 流程仍待验收。

配置后的账户操作用全局 `--account ID`：

- `mail folders` 返回允许的文件夹及不透明 id；`mail list --folder ID` 和 `mail search 查询 --folder ID` 使用该 id，不使用模型提供的任意文件夹名。中文查询使用 UTF-8 同步 literal；服务器不支持该字符集时明确报错。
- `mail read 消息ID` 安全读取；`mail reply 消息ID --body-file FILE` 创建回复；`--mode reply_all` 全部回复，`--mode forward --to 地址` 显式转发。
- `draft update ID --revision 版本 FILE` 乐观更新；`draft add-attachment ID FILE` 显式添加附件；`mail save-attachment 消息ID 部件ID DIRECTORY` 显式保存，不打开/执行附件。
- `draft approve ID` 人工审阅，`send ID` 严格 TLS 单次发送，`status 提交ID` 查询。真实账户不能混用 `--smtp-test`。
- `mail sync-draft 草稿ID` 显式版本快照追加；`mail sync-sent 提交ID` 仅用于 Append 策略。`mail sync-status 同步ID`、`mail sent-evidence 提交ID` 查询；`mail recover-send 提交ID` 在取得发送进程锁后把中断提交保留为 Uncertain。
- `mail mark-seen 消息ID` 是显式 CLI 操作，需要起草写权限；默认 TUI/MCP 不标记已读，MCP 不暴露此工具。

账户的 `tui` / `mcp` 使用同一 Host；`tui --read-only` / `mcp --read-only` 可进一步收窄配置。TUI 人工能力单独注入交互终端，MCP 无法批准。MCP 有 14 个工具，工具参数不能改变账户、权限或端点；没有附件文件访问或批准工具。离线演示仅支持其中适用的工具。

账户默认配置/数据路径为 XDG_CONFIG_HOME/letterkeeper 与 XDG_DATA_HOME/letterkeeper（未设置时 ~/.config、~/.local/share）；可用 `--config-dir` / `--data-dir` 覆盖，每账户单独状态库。`--state-dir` 仅用于原离线演示，缺省 cwd 的 `.local/letterkeeper`。系统凭据刷新锁按 UID/账户固定在 `/run/user/UID/letterkeeper/credential-locks`，不随数据目录覆盖改变。

私有目录/文件要求 0700/0600，配置使用 descriptor walk 和原子替换；SQLite 的既有路径重新打开限制仍在。草稿数据库未加密。读取/发送原文上限 2 MiB，单附件导入/导出 512 KiB；SafeDocument 显示预算 64 Ki 字符，回复元数据另有字段限制。同步 Pending/Uncertain 不自动重试；目前明确 Failed 同步也不能重认领同一版本，旧服务器草稿不会自动删除。

发送诊断将状态库自动迁移到 schema 3，保留既有草稿、批准、提交和发送字节；旧记录不会补造诊断。升级前退出旧进程并备份状态库；旧构建不能打开升级后的数据库。旧备份用于迁移失败时恢复，发生新发送后须保留新库，不能依据旧备份重发。设计见 [ADR 0006](docs/decisions/0006-send-diagnostics.md)。

## 验证与开发

下面命令在远程受控 runner 执行，完整流程见 [CI 验证](docs/testing/ci.md)。公开 Actions 的日志与产物只使用合成邮件、虚构身份和受控服务；不得上传 `.local/`、用户配置或真实凭据。

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 scripts/verify_terminal.py target/debug/letterkeeper
python3 scripts/verify_workspace.py target/debug/letterkeeper
python3 scripts/verify_privacy_workspace.py target/debug/letterkeeper
# 需要已安装 Codex；临时配置、禁网，无模型调用
python3 scripts/verify_codex.py target/debug/letterkeeper
python3 scripts/benchmark.py target/debug/letterkeeper
```

测试使用合成邮件、本机 TLS IMAP/SMTP/OAuth、HTTP/DNS 观察器与 PTY；不调用真实凭据库。全套测试须有 loopback、PTY 和 bubblewrap 权限，不能跳过隔离项后宣称通过；缓存依赖时可添加 `--offline`。真实 TLS 终端往返也包含在 Rust 集成测试中。

release 的 1k/10k 合成邮箱、32 KiB/2 MiB 原文与 32 KiB/1 MiB 编辑输入实测见 [阶段 C 性能](docs/testing/performance-phase-c.md)。测量采用从进程启动开始的子进程树采样；输入与空闲 CPU 门槛在该样本上通过，不等于真实服务性能承诺。

已记录 [GitHub runner 性能基线](docs/testing/hosted-performance.md)及原始合成 JSON，不能直接用 Arch 本机历史样本认定远程回归通过。真实服务按独立授权关卡分步执行；已完成的本机凭据往返与 OAuth 登录及剩余范围见[真实验收记录](docs/testing/live-keyring-2026-10-15.md)。

开发继续使用独立分支/worktree、远程独立编译目录、TDD 和按任务选择模型的子代理，见 [协作流程](docs/WORKFLOW.md)。真实服务操作清单见 [独立验收关卡](docs/testing/live-acceptance.md)。

## 文档与许可

- [账户接入决定](docs/decisions/0005-account-onboarding.md) · [开发记录](docs/development/2026-10-14-account-onboarding.md) · [诊断验证](docs/testing/account-diagnostics.md) · [配置后端验证](docs/testing/account-onboarding.md) · [CLI 验证](docs/testing/account-cli.md)

- [需求](docs/REQUIREMENTS.md) · [验收状态](docs/STATUS.md) · [Agent 指引](AGENTS.md)
- [远程 CI 决定](docs/decisions/0004-hosted-ci.md) · [远程 CI 开发记录](docs/development/2026-10-14-hosted-ci.md) · [CI 验证](docs/testing/ci.md)
- [公开前历史检查](docs/testing/publication-audit.md) · [GitHub 性能与源码产物](docs/testing/hosted-performance.md)
- [阶段 A 决定](docs/decisions/0001-phase-a.md) · [阶段 B 决定](docs/decisions/0002-phase-b.md)
- [阶段 A 记录](docs/development/2026-10-12.md) · [阶段 B 记录](docs/development/2026-10-13.md)
- [阶段 B 集成](docs/testing/phase-b-integration.md) · [协议](docs/testing/protocol.md) · [账户/OAuth](docs/testing/accounts.md)
- [正文/回复](docs/testing/composition.md) · [同步](docs/testing/sync.md) · [适配层](docs/testing/adapters-phase-b.md)
- [阶段 C 决定](docs/decisions/0003-phase-c.md) · [阶段 C 开发](docs/development/2026-10-13-phase-c.md) · [阶段 C 集成](docs/testing/phase-c-integration.md)
- [实际 Codex 客户端](docs/testing/codex-phase-c.md) · [展示 HTTP/DNS](docs/testing/privacy-workspace-phase-c.md)
- [性能样本](docs/testing/performance.md) · [阶段 C 性能](docs/testing/performance-phase-c.md) · [依赖及许可证](docs/DEPENDENCIES.md)

同 UID 任意 shell/文件写入权限超出普通 MCP 权限边界，隔离这类主体需要额外 OS 机制。原创代码与文档采用 **GPL-3.0-only**，见未修改的 [LICENSE](LICENSE)；第三方依赖按各自许可证处理，开源许可证本身不构成隐私保证。
