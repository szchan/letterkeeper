# 安全发送诊断：核心持久化

范围：MAIL-07、DATA-02、MCP-09。仅合成账户、合成内容，不使用真实凭据或邮箱。

## RED

基线 `5a813b2da1179b6115553df17969a943a89007cb`。新增固定类型、可编译桩与行为测试；桩完成旧状态但丢弃诊断。精确远程选择器：`failed_and_uncertain_diagnostics_survive_reopen_without_retry`，crate `letterkeeper-core`，phase `RED`。精确 SHA `b2f17887dfaa41ce4210a8d5053c65a058b65ed6` 的 [RED run](https://github.com/szchan/letterkeeper/actions/runs/38710063986) 已完成为 failure。库存匹配 1 项，测试实际运行 1 项；重开后 getter 返回 `None`，预期 `Some(SendDiagnostic { stage: Submission, kind: Timeout, smtp_code: Some(451) })`，断言失败。`TDD_EVIDENCE` 为 `state=test_finished`、`raw_exit_code=101`。此前手填错误目标 SHA 的 run `38710009970` 已取消，不作为 TDD 证据。

## GREEN 实现

schema 0/1/2 在同一迁移事务升级到 3，独立 `submission_diagnostics` 表以唯一提交外键、固定 stage/kind CHECK 和整数 SMTP 范围 CHECK 限制数据。既有 Submission/ListEntry 与草稿摘要不变。旧 finish 委托新入口且诊断为 None，recover 不推断阶段。

新入口先保留原账户发送权限与 Submitting 状态检查，再校验 Accepted 不带诊断、码值 100..599；状态 UPDATE 与诊断 INSERT 同一即时事务，任何存储失败都回滚。getter 检查 read 和提交账户；不存在与越权 ID 均返回 Denied，允许的旧记录返回 None。Serde 仅接受固定枚举，拒绝未知字段和超界码。

7 项新增集成测试覆盖 Failed/Uncertain 重开、Accepted/旧 finish、范围与序列化、权限/存在性、双向触发器故障回滚、schema 0/1/2 与恢复、SQLite 固定值/唯一外键约束。旧迁移测试按新 schema 更新。首个实现 SHA `e61d04bb80536abeb9e07d191807a16017b8427c` 的 [完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38710458345) 在 Clippy 拒绝 `manual_range_patterns` 与 `collapsible_if`；workspace 测试尚未执行，Codex job 成功。已改用范围匹配和 `Option::is_some_and`，不降低 lint。代码最终 SHA `5c102e536ebf50b775d6c4a6f630748e8fe89e64`：

- [精确 GREEN](https://github.com/szchan/letterkeeper/actions/runs/38729961824) success；同一选择器匹配并运行 1 项，`state=test_finished`、`raw_exit_code=0`。中断恢复后补跑的首实现 SHA e61d04b [GREEN](https://github.com/szchan/letterkeeper/actions/runs/38729891012) 也成功，但最终证据以 5c102e5 为准。
- [完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38729946609) success；PR #16 头为上述 5c102e5，实际 PR 合成合并 checkout 为 `a9580500d79fec9bf59902bfde856d4df60e5a08`（基线 main `5a813b2da1179b6115553df17969a943a89007cb`）。workspace、codex-client 与 ci-gate 均成功。
- 新增 `send_diagnostics.rs` 7 项全部通过；fmt、Clippy `-D warnings`、完整工作区、实际 PTY、编辑/审批/发送闭环、可达 HTTP/DNS 隐私观察器及实际 Codex app-server 路径均通过。

远程环境：GitHub hosted Ubuntu 24.04.5 x86_64，Rust 1.99.0，现有受控 bubblewrap namespace 配置；没有跳过安全项。TDD 实际命令为 `cargo test --locked -p letterkeeper-core failed_and_uncertain_diagnostics_survive_reopen_without_retry -- --exact --nocapture`；完整门禁入口 `.github/workflows/ci.yml`。本结果记录提交仅修改文档，其新 revision 的远程门禁由主代理集成前再次核对，不用旧分支 SHA 替代集成验收。

## 验证边界

本机仅编辑、rustfmt、Git 和远程结果审查；没有执行 Cargo 构建/check/test/Clippy。发送完成与诊断需要原子持久化；旧提交不推断历史失败阶段。诊断只允许固定阶段、固定原因和 100..599 的可选 SMTP 数值码，没有服务器自由文本字段。本任务仅实现核心存储，不推断服务商原因或历史阶段，不涉及真实 SMTP/凭据验收；协议阶段分类和应用展示由相应后续实现集成验证。主代理的新集成 SHA 必须重新通过完整门禁。
