# Gmail OAuth 配置标签兼容修复

基线 `64dd635e5349f58cec93a164bfd6c4e781ca4269`，独立分支 `fix/gmail-config-alias`；[draft PR #8](https://github.com/szchan/letterkeeper/pull/8)。

公开 README 的手写认证标签与 serde 派生标签不一致。先提交合成配置测试，在精确测试 SHA 上确认远程真实断言 RED 完成后，再给 `GmailOAuth` 添加 `serde(alias = "gmail_oauth")`。已有 `gmail_o_auth` 输入和序列化保持兼容；未知认证标签仍拒绝。

README 补充优先使用 `account add` 类型化引导，并明确两种可接受的手写标签及保存格式。无新依赖、无权限或凭据逻辑变更；未接触真实账户、私有配置、钥匙环或 OAuth 网络操作。

验证 SHA、命令、Actions 证据与边界统一记录在[配置兼容验证](../testing/gmail-config-compat.md)。本任务分支结果不替代主代理集成新 revision 的完整门禁。
