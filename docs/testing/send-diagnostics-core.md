# 安全发送诊断：核心持久化

范围：MAIL-07、DATA-02、MCP-09。仅合成账户、合成内容，不使用真实凭据或邮箱。

## RED

基线 `5a813b2da1179b6115553df17969a943a89007cb`。新增固定类型、可编译桩与行为测试；桩完成旧状态但丢弃诊断。精确远程选择器：`failed_and_uncertain_diagnostics_survive_reopen_without_retry`，crate `letterkeeper-core`，phase `RED`。SHA 与运行结果待远程完成后补充。

## 验证边界

本机仅编辑、rustfmt、Git 和远程结果审查；没有执行 Cargo 构建/check/test/Clippy。发送完成与诊断需要原子持久化；旧提交不推断历史失败阶段。诊断只允许固定阶段、固定原因和 100..599 的可选 SMTP 数值码，没有服务器自由文本字段。GREEN 与完整 CI 尚待实现后验证。
