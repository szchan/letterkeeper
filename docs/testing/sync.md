# 回复头与远程同步 TDD

2026-10-13，使用合成身份及临时 SQLite。

RED：`cargo test --offline -p letterkeeper-core --test sync` 编译成功，8 项中 2 通过、6 失败。未实现的同步认领返回 Storage；回复头注入错误地被接受。保留空线程字段的序列化兼容与授权随内容变化失效测试已通过。

补充 RED：同步实现后加入精确 MIME wire 持久化测试，11 项中 10 通过、1 失败，失败为尚未保存 wire。之后实现不可重写的 16 MiB 有界 wire 存储，供 Sent 使用原发送字节，而不是重新生成 Date/边界。

GREEN：`cargo test --offline -p letterkeeper-core` 28 项通过（既有 17 + 新增 11），Clippy `--all-targets -- -D warnings` 通过。schema 1 → 2 原子迁移、旧批准与摘要兼容、线程头注入、单次同步/竞争、Pending/Uncertain 重启保持、作用域、不可变快照和 MIME wire 均有验证。原“未来 schema”测试随当前版本提升到 3，继续验证拒绝未知版本。

同步每版本仅允许一次认领，明确失败也不自动重试；用户需要新的草稿版本。服务器旧草稿不自动删除。应用实际 TLS 与 APPEND 闭环另见阶段 B 集成记录。
