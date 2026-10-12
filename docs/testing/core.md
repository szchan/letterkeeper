# Core 草稿与发送状态验证

2026-10-12，GPL-3.0-only。全部测试仅使用合成账户和内容。

## RED

在 `feat/core` worktree 执行 `cargo test -p letterkeeper-core --offline`。
公开 API 的占位实现可编译；集成测试 10 项，9 项失败、1 项通过，失败均由未实现的核心操作触发。测试覆盖 CAS、授权摘要、Bcc/附件变更、过期、默认拒绝、持久化、并发认领和不确定发送。

实现与 GREEN 结果待补充。
