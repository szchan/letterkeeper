# ADR 0002：账户、私有配置与 Gmail OAuth

日期：2026-10-13。状态：实现及合成验证通过；只授权合成、受控 loopback 验证，未验证真实 Gmail。

使用独立 accounts crate，账户 ID 是严格 ASCII 命名空间，配置仅保存公开身份、严格 TLS 端点、文件夹、用户拥有的操作权限和认证方法。配置拒绝未知字段，绝不接受密码/token/client secret 内容。额外 CA 仅是公开证书路径。Sent 明确选择 server-managed 或 APPEND，不由协议层猜测。

正式目录为 `$XDG_CONFIG_HOME/letterkeeper`（缺省 `$HOME/.config/letterkeeper`）与 `$XDG_DATA_HOME/letterkeeper`（缺省 `$HOME/.local/share/letterkeeper`），可显式覆盖。目录 0700、账户 JSON 0600，大小上限 64 KiB；读取/替换拒绝符号链接、硬链接及宽权限；以目录 fd 与 O_NOFOLLOW 相对打开文件，临时文件原子替换并同步。不支持同 UID 任意 shell 攻击者或外部进程更换整个祖先目录的威胁模型。

凭据抽象为 Send+Sync CredentialStore，按账户 ID 和 SecretKind 区分。Linux 实际适配选择 maintained `secret-service` 5.2.0（MIT OR Apache-2.0），启用 `rt-async-io-crypto-rust`，每次连接指定 `EncryptionType::Dh`，无 Plain/file fallback。使用系统默认 collection；不可用、锁定、重复项目及交互失败均固定错误。测试只注入内存 fake，不连接或写入用户 keyring。Secret wrapper 使用 `zeroize` 1（MIT OR Apache-2.0），无 Serialize，Debug 只显示 redacted；依赖/HTTP 层内部复制不保证全生命周期清零。

OAuth 使用 `oauth2` 5（MIT OR Apache-2.0，禁用默认 features）生成 state 和 S256 PKCE、构建/解析 token 请求，`reqwest` 0.12（MIT OR Apache-2.0）blocking + rustls；关闭 redirect 和 proxy，超时 15 秒，HTTP 响应上限 64 KiB，不记录错误响应/URL。不自行实现密码学。选择这些已有稳定版本是因 oauth2 5 API 直接支持 custom bounded HTTP closure；cargo 元数据与官方源码核实了 features/license，2026-10-13 仍可获取维护版本。rustix 1（Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT）提供安全 fd 相对文件操作；url 2（MIT OR Apache-2.0）解析 callback。

固定 Google auth/token/revoke URL 和 `https://mail.google.com/` scope。授权返回仅向 trusted CLI 暴露的浏览器 URL，不向 MCP 提供。loopback 127.0.0.1 随机端口，callback 有大小/时间上限，核对 Host/path/state、拒绝重复参数、错误或不足 scope，仅消费一次。token 成功后保存一个 vault bundle（access/refresh/expiry），refresh rotation 原子更新单条 secret；未返回 refresh 时保留旧值。invalid_grant 表示需重新登录；logout 清本地 token，revoke 仅主动调用且成功后清理。本地测试端点只允许通过明确 fixture constructor 传入 loopback URL，不来自账户配置。

官方资料：[Secret Service crate](https://docs.rs/secret-service/5.2.0/secret_service/)、[源仓库](https://github.com/hwchen/secret-service-rs)、[oauth2 5](https://docs.rs/oauth2/5.0.0/oauth2/)、[Google native apps](https://developers.google.com/identity/protocols/oauth2/native-app)、[Gmail XOAUTH2 scope](https://developers.google.com/workspace/gmail/imap/xoauth2-protocol)。Google desktop client 注册、个人使用与公开分发/受限 scope 验证仍待用户独立注册和真实授权验收；实现不借用第三方 client identity。配置 send 权限不替代 core 人工批准。

实现时增加 callback 的账户/client/endpoint owner 绑定与进程内 token mutex。跨进程调用由应用的每账户 auth.lock flock 协调；直接库调用者须承担此协调。测试证据和未授权验证见 [账户验证](../testing/accounts.md)。

跨进程锁命名空间补充：系统凭据按全局账户 ID 保存，不能按可覆盖缓存目录加 auth.lock。CredentialStore 返回 coordination_directory，fake 默认 None；System 固定 `/run/user/{effectiveUID}/letterkeeper/credential-locks/{accountID}`，忽略 HOME/XDG/config/data 覆盖。只接受已经存在、本人拥有且0700、路径无 symlink 的 `/run/user/{UID}`，自身仅返回路径，不创建目录/文件，不回退；应用私有创建 descendants 并持有 flock 覆盖完整 login/refresh/logout/revoke。库直接调用仍须应用协调跨进程。
