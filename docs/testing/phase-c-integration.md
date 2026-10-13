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
