# 核心与协议源码职责拆分

日期：2026-10-16。基线：`3cf6b01697a3dd31adbeaccb2a9cfdae27e5d279`。本次为已授权结构整理的核心/协议子任务，只移动源码、收窄入口并保留 crate 根公开路径；没有新增行为、依赖、数据库版本或 SQL 变化，也没有真实账户操作。

## 文件职责

- `letterkeeper-core/src/lib.rs` 保留原公开类型、错误、权限和 `Store` 的显式重导出；数据类型、权限、错误及输入验证分别归入独立模块。
- `store/mod.rs` 拥有连接和策略，保留打开存储及策略装配；迁移、私有路径、草稿、人工审批、提交及同步各有模块。事务与批准消费的原有方法保持完整，没有按步骤切断事务。
- `letterkeeper-protocol/src/lib.rs` 保留原公开类型、`ImapClient`、SMTP 两个发送入口及 TLS 验证函数的显式重导出。
- 协议公共数据、同步/异步运行时桥接、严格 TLS、输入验证、IMAP 和 SMTP 各有独立模块。`wire.rs` 保持原有有界传输实现；同一 trait 的实现没有分散。SMTP 固定阶段记录仅向 crate 内 TLS 模块开放设置方法。

所有新增 Rust 文件保留 `GPL-3.0-only` SPDX。内部跨模块所需权限限于 crate/父模块，不新增外部 API。原公开函数名、类型、字段、Serde 表示、错误内容、数据库迁移、协议命令、超时与响应边界不变。

## 验证与证据

这是纯重构，按照本次授权使用原有行为回归和完整远程门禁，不为文件位置编写人工失败的测试，不把静态搬移检查称作 RED。

- 基线完整 CI：[`38736285052`](https://github.com/szchan/letterkeeper/actions/runs/38736285052)，基线 SHA 见本次任务记录。
- 本机只执行 `rustfmt --edition 2024 --check`、`git diff --check` 和轻量源码审查；没有 Cargo 构建、check、测试、Clippy 或安装编译。
- 轻量审查将原根文件和拆分后的 Rust 函数分词比较：核心 42 个、协议 32 个函数的签名和函数体 token 全部保留，忽略导入、空白、注释和函数外可见性限定；这只能证明机械搬移一致，不替代编译或行为验收。
- 子任务分支：`refactor/structure-core-protocol`。远程完整 CI 当前待验证；结果以精确 SHA 和 Actions run 为准。

远程门禁覆盖 fmt、Clippy、完整 workspace 回归、真实 PTY、工作区审批/发送、可达 HTTP/DNS 隐私观察器及实际 Codex MCP 路径。子任务通过不能代替主代理集成后新 SHA 的完整门禁，也不增加产品或真实服务验收结论。
