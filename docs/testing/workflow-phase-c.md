# 阶段 C：受信人工工作流后端

2026-10-13。范围：`HumanWorkspace` 的离线与账户 Host 实现、同一 SQLite Store 的账户作用域分页、草稿审阅与批准。全部测试使用合成身份和本地文件，不接触真实邮箱或系统凭据库。

## RED 与 GREEN

先提交测试 `c2b44f0`。`cargo test -p letterkeeper --test workflow --offline` 初次编译失败：`AppHost` 尚无 `HumanWorkspace` 的 `capabilities/create/list_drafts/review/approve/send` 等方法（E0599，共 21 处）。`cargo test -p letterkeeper-core --test workspace --offline` 初次编译失败：`Store` 尚无 `list_drafts/list_submissions`（E0599，共 5 处）。这两个 RED 是真实编译失败，不是断言失败。

实现后，`cargo test -p letterkeeper --test workflow --offline` 的 6 个测试和 `cargo test -p letterkeeper-core --test workspace --offline` 的 3 个测试通过。覆盖只读权限先于文件访问、账户作用域与 50 项分页、完整草稿审阅、附件精确字节 SHA-256、乐观版本冲突、旧摘要失效、双 Store 争抢、过大 MIME 批准拒绝，以及离线不可用操作的明确结果。既有 `letterkeeper-core/tests/state.rs` 另覆盖批准到期与单次消费；此处复用同一状态机而不另建批准表。

## 边界

`HumanWorkspace` 只注入受信的人类终端。MCP 保持原有 Host 工具集合，不暴露批准、附件路径或配置。列表在 SQLite 内按账户筛选并只返回必要元数据；不把隐藏账户正文或附件字节加载到列表结果。批准时重新读取当前草稿，要求精确输入完整摘要，并在生成授权前用发送 MIME 序列化器检查 2 MiB 上限；到期时间为当前时间加 300 秒。批准不触发发送。附件导入/导出复用 NOFOLLOW、大小、目录权限与禁止覆盖的现有文件工具。离线回复和服务器同步返回 `Unsupported`；离线发送仍仅能使用显式 loopback `smtp_test` 与合成地址。账户 Host 复用既有 TLS SMTP、发送锁、不可变提交字节、恢复及显式同步路径。

受信 TUI 的交互、公开 MCP 客户端展示、真实 Gmail/Secret Service 和实际账户状态修改由各自独立验收记录负责；本记录不将这些关卡写成完成。
