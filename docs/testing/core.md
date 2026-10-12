# Core 草稿与发送状态验证

2026-10-12，GPL-3.0-only。全部测试仅使用合成账户和内容，无真实账户、网络或外发。

## RED → GREEN

在 `feat/core` worktree 执行 `cargo test -p letterkeeper-core --offline`。RED 提交 `078d739`：公开 API 的占位实现可编译；集成测试 10 项，9 项失败、1 项通过，失败由未实现操作触发。

GREEN 执行以下命令：

```sh
cargo test -p letterkeeper-core --offline
cargo clippy -p letterkeeper-core --offline --all-targets -- -D warnings
```

最终结果：17 项集成测试通过，0 项单元测试、0 项文档测试，合计 17 项；Clippy 无警告，`cargo fmt --package letterkeeper-core -- --check` 通过。覆盖：

- 创建/查询、版本 CAS、版本改变撤销授权。
- 相同版本下逐项改变账户、From、To/Cc/Bcc、主题、正文、附件名称/类型/字节，SHA-256 摘要均改变；相同内容与版本产生相同摘要。
- 批准摘要匹配、过期时刻不可用、单次消费；失败后必须重新批准。
- 两个独立 SQLite 连接同时认领同一批准，只有一个成功。
- 草稿、批准、提交快照和不确定结果重启后保留；打开连接不自动恢复其他 worker。
- 已接受/失败/不确定的终态不能改写；提交中/已接受/不确定的草稿不能编辑、重新批准或发送。
- 默认拒绝，账户和操作范围在草稿、查询提交、更新、批准、认领、完成和恢复路径生效。
- 控制字符、头注入、附件路径、正文/附件/收件人数量边界；错误不返回输入原文。
- 私有目录/文件权限、软/硬链接拒绝、未知 schema 版本拒绝。

## API 与授权边界

`Store::open(path)` 和 `Store::memory()` 默认拒绝全部操作。可信应用 host 通过消费式 `with_policy(AccessPolicy::new(accounts, Permissions { read, draft, send }))` 装配账户/操作范围；工具参数、正文或模型不得设置策略。

`approve(draft_id, expected_digest, expires_at)` 只可由应用的人工 CLI/TUI 审阅路径调用，不能导出为 MCP 工具。`send` 权限允许账户范围内消费已有批准，不能替代批准。核心库本身不能证明调用者是人；该边界必须由 host 验证。拥有同 UID 任意代码执行能力的人不属于 MCP 权限隔离范围。

批准绑定账户、完整信封、正文、附件字节和版本。`claim_send` 使用 SQLite `BEGIN IMMEDIATE`：检查批准并原子删除，保存发送快照和稳定 Message-ID，提交事务后返回。SQLite 随机字节只用于生成不含身份信息的关联 Message-ID（`letterkeeper.invalid`）；不作为授权凭据。

SMTP 在数据库事务外执行。协议适配器随后调用 `finish_submission`，只有 `Submitting` 可转为 `Accepted`、`Failed` 或 `Uncertain`。`Failed` 必须表示可以证明没有接受该邮件，否则使用 `Uncertain`。失败后重新批准才可创建新提交；其他状态禁止再次认领。查询返回原有提交 ID、Message-ID、状态和原始快照。

`recover_interrupted(submission_id)` 是显式的 `Submitting → Uncertain`；host 必须先确认相应 sender 已退出，不能因为时间过去或重启便调用。查询不确定结果后仍禁止重试；与邮箱服务商核验投递的恢复/解决界面尚未实现。

## 存储与输入边界

磁盘存储当前仅支持 Unix。缺失的父目录和文件以 0700/0600 创建；已有父目录必须 0700、文件必须 0600，并拒绝软链接或多硬链接。SQLite 使用 `NOFOLLOW`、DELETE journal、FULL synchronous、schema user_version 1；未知版本拒绝打开。不会 chmod 用户已有宽松目录。用户账户数据库应位于专用私有目录，不能直接放在共享临时目录。

输入当前只接受不带显示名称的 ASCII addr-spec，暂不支持 SMTPUTF8/注释/引号邮箱；此校验是受限原型输入接口，不实现邮件协议。主体允许 LF/tab，拒绝其他控制字符和方向控制字符。限制：账户 128 字节、邮箱 254 字节（local part 64）、主题 998 字节、正文 1 MiB、全部收件人 100 个、附件 16 个、附件总字节 10 MiB、单文件名 255 字节、媒体类型 127 字节。附件只保留名称/类型/字节，不读取路径、不导出或执行。

数据库包含明文邮件内容，权限保护不等于静态加密。错误采用固定说明，不携带 SQL、邮箱地址、正文或底层错误详情。本轮未实现 IMAP、SMTP、服务器 Drafts/Sent 同步、加密、凭据管理、迁移工具、数据库损坏恢复、真实账户测试或服务商的已发送查询；这些需要后续适配器与集成验证。
