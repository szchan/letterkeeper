# 安全发送诊断：核心持久化

范围：MAIL-07、DATA-02、MCP-09。仅合成账户、合成内容，不使用真实凭据或邮箱。

## RED

基线 `5a813b2da1179b6115553df17969a943a89007cb`。新增固定类型、可编译桩与行为测试；桩完成旧状态但丢弃诊断。精确远程选择器：`failed_and_uncertain_diagnostics_survive_reopen_without_retry`，crate `letterkeeper-core`，phase `RED`。精确 SHA `b2f17887dfaa41ce4210a8d5053c65a058b65ed6` 的 [RED run](https://github.com/szchan/letterkeeper/actions/runs/38710063986) 已完成为 failure。库存匹配 1 项，测试实际运行 1 项；重开后 getter 返回 `None`，预期 `Some(SendDiagnostic { stage: Submission, kind: Timeout, smtp_code: Some(451) })`，断言失败。`TDD_EVIDENCE` 为 `state=test_finished`、`raw_exit_code=101`。此前手填错误目标 SHA 的 run `38710009970` 已取消，不作为 TDD 证据。

## GREEN 实现

schema 0/1/2 在同一迁移事务升级到 3，独立 `submission_diagnostics` 表以唯一提交外键、固定 stage/kind CHECK 和整数 SMTP 范围 CHECK 限制数据。既有 Submission/ListEntry 与草稿摘要不变。旧 finish 委托新入口且诊断为 None，recover 不推断阶段。

新入口先保留原账户发送权限与 Submitting 状态检查，再校验 Accepted 不带诊断、码值 100..599；状态 UPDATE 与诊断 INSERT 同一即时事务，任何存储失败都回滚。getter 检查 read 和提交账户；不存在与越权 ID 均返回 Denied，允许的旧记录返回 None。Serde 仅接受固定枚举，拒绝未知字段和超界码。

7 项新增集成测试覆盖 Failed/Uncertain 重开、Accepted/旧 finish、范围与序列化、权限/存在性、双向触发器故障回滚、schema 0/1/2 与恢复、SQLite 固定值/唯一外键约束。旧迁移测试按新 schema 更新。首个实现 SHA `e61d04bb80536abeb9e07d191807a16017b8427c` 的 [完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38710458345) 在 Clippy 拒绝 `manual_range_patterns` 与 `collapsible_if`；workspace 测试尚未执行，Codex job 成功。已改用范围匹配和 `Option::is_some_and`，不降低 lint。精确最终 GREEN SHA、TDD 与完整 CI 结果待远程补充。

## 验证边界

本机仅编辑、rustfmt、Git 和远程结果审查；没有执行 Cargo 构建/check/test/Clippy。发送完成与诊断需要原子持久化；旧提交不推断历史失败阶段。诊断只允许固定阶段、固定原因和 100..599 的可选 SMTP 数值码，没有服务器自由文本字段。GREEN 与完整 CI 尚待实现后验证。
