# 账户与 OAuth 验证

2026-10-13；只使用合成身份、内存 fake vault 与受控 loopback 服务。不写真实系统凭据，不启动外部浏览器，不访问真实 Gmail。

RED：`cargo test -p letterkeeper-accounts --test accounts`（实现尚不存在，预期编译失败）；真实结果 exit 101，缺少 AccountConfig/CredentialStore/AccountRepository API，23 个编译错误；不是行为断言失败。配置验证覆盖成功/拒绝、账户命名空间、文件夹权限、凭据分离、符号/硬链接、权限及大小边界。OAuth 测试随后覆盖完整 code/callback/token/refresh/revoke 状态机。

OAuth RED：`cargo test --offline -p letterkeeper-accounts --test oauth`，exit 101，OAuthLogin/GmailOAuth 等 API 缺失（编译 RED）。
