# 阶段 C：人工工作区集成验证

日期：2026-10-13。范围仅为合成身份、本机状态和 PTY；真实 Gmail、系统凭据库及公开客户端仍须另行验收。

## TDD 记录

RED 提交 `71d5c4b` 先添加 `workflow_mcp.rs` 与 `terminal_workflow.rs`。命令：

```sh
CARGO_TARGET_DIR=.worktrees/phase-b/target cargo test --offline -p letterkeeper --test workflow_mcp --no-run
```

实际结果：`E0599` 共 12 处，`AppHost` 尚无 `HumanWorkspace` 的 `list_drafts`、`load_draft`、`save`、`approve`、`create`、`review`、`send`、`list_submissions`。这是接口未实现导致的编译 RED，不是行为断言失败。Cargo 生成的锁文件差异未提交。

## GREEN 与验证路径

集成分支先在共用 debug 目标目录运行下列目标测试，11/11 通过；后续发现多个 worktree 同名包共用目标目录可发生旧构建产物互相覆盖，因此最终 release 基准改用独立 `/tmp/letterkeeper-phase-c-release-target`，完整工作区 debug 检查由集成分支的独立目标目录执行。

```sh
CARGO_TARGET_DIR=/home/szchan/Projects/letterkeeper/.worktrees/phase-b/target cargo test --offline -p letterkeeper --test workflow_mcp --test terminal_workflow -- --test-threads=1
```

`terminal_workflow` 8/8，`workflow_mcp` 3/3；本机 PTY 与 loopback 需要相应沙箱权限，初次在受限沙箱中监听端口返回 `Operation not permitted`，放行本机受控测试后通过。新增超 16 KiB 正文与 HTTP/DNS 观察断言在独立 release 测试二进制上用 `--exact pty_approval_requires_full_digest_and_f9_sends_once` 再次运行，1/1 通过。

- `workflow_mcp` 用同一个 `AppHost` 的 MCP 与人工能力验证：Agent 创建草稿、人工工作区可见、人工编辑 Cc/Bcc/主题/正文、错误摘要拒绝、正确批准不触发发送、模型 `confirm=true` 无授权效力；第二个进程视角通过另一个 Store 打开同目录，修改后旧审阅摘要必须失效。另用 20,001 字节附件与超过 16 KiB 的正文检查完整审阅和 SHA-256，移除附件后旧摘要失效。
- `terminal_workflow` 在真实 PTY 的 120×40、80×24、40×12、20×6 尺寸运行 `run_tui_workspace`，验证编辑状态里的 `q` 是正文字符、脏草稿退出提示、窗口变化及 termios 恢复。子进程只使用合成本地数据。
- 同文件的 SMTP PTY 用受控本机服务分别产生 `Accepted` 与 DATA 后断连的 `Uncertain`；从草稿列表进入完整审阅、F8 输入整串摘要、确认批准没有发送，再按 F9 显式提交，查持久化记录并检查无自动重试。正文嵌入 HTTP 图片和 DNS 预取诱饵，监听器先通过父进程正向控制，编辑/审阅/发送路径不应触发请求。Bcc 应只进入收件信封，不能出现在邮件头。
- `remote_human_workspace_shares_controlled_tls_mail_and_drafts` 使用注入凭据与本机 TLS IMAP，覆盖 RemoteHost 的无 Seen 读取、草稿、完整审阅和关闭发送权限。
- `scripts/verify_workspace.py` 从实际二进制启动离线合成 TUI，在四种尺寸重复相同终端路径。此脚本需要集成分支把离线 TUI 接入 `run_tui_workspace` 后运行。

完整工作区测试、格式检查和 Clippy 由集成分支统一记录。上述受控服务测试不替代真实服务商与公开客户端展示验证。


## 最终独立构建 GREEN

在集成worktree使用独立默认target（无共享CARGO_TARGET_DIR）执行：

```sh
cargo test --offline --locked --workspace
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
python3 scripts/verify_terminal.py target/debug/letterkeeper
python3 scripts/verify_workspace.py target/debug/letterkeeper
python3 scripts/verify_codex.py target/debug/letterkeeper
python3 scripts/verify_privacy_workspace.py target/debug/letterkeeper
```

**210项注册Rust测试通过，0失败，0忽略**；不重复加入子进程probe的filtered运行。fmt/Clippy通过。阶段B的170项保留。

| 模块 | 注册测试 |
| --- | ---: |
| 应用CLI/SMTP/stdio/Host/worker/进程锁/PTY/人工工作区 | 59 |
| 账户/OAuth/私有路径 | 23 |
| TUI/MCP适配 | 36 |
| MIME/正文/回复/附件 | 38 |
| 授权/SQLite/同步/分页 | 33 |
| TLS IMAP/SMTP协议 | 21 |

若干注册项是子进程/基准helper，空环境下立即返回，仅由父测试或独立基准触发；不把它们当作独立业务覆盖，测试数量也不代表全部P0。真实PTY主体还验证了End到16KiB之外的正文尾部、完整摘要批准不发送、F9单次发送、Accepted/Uncertain、Bcc无邮件头与重启查询。

实际命令的四尺寸workspace脚本通过；旧导航/CLI批准脚本通过。初次新脚本在退出处没有读取PTY输出，窗口重绘被管道背压阻塞；补排空输出后通过。旧脚本“选择”帮助标记已不在新帮助页，改为真实标题“帮助”。沒有修改应用去迎合旧标记。

[实际Codex](codex-phase-c.md)验证两个14工具会话，无模型、个人auth/config。[真实resolver/HTTP](privacy-workspace-phase-c.md)脚本正向控制成功后，MCP草稿/编辑/完整审阅无新DNS/HTTP资源请求；既有worker隔离测试仍覆盖原始解析阶段。14工具不含批准/路径/策略能力。

[release性能](performance-phase-c.md)使用从进程启动开始的5ms树采样，8组TUI与4组活跃SDK MCP通过输入p95≤100ms/空闲单核CPU≤1%样本门槛。385项registry清单与锁定metadata匹配，LICENSE未修改。

[真实服务关卡](live-acceptance.md)已准备但未授权执行。真实Gmail/Secret Service、公开图形前端、受限自动发送仍未验收；其它P0缺口见 [STATUS](../STATUS.md)。
