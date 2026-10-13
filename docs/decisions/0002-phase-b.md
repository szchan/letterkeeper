# ADR 0002：受控邮件协议闭环

日期：2026-10-13。状态：实施中，尚未验收。

本轮在阶段 A 基线上实现阶段 B 的代码与受控验证，不连接真实邮箱。协议、账户/凭据与正文扩展分别由独立 worktree 的子代理开发；主代理集成共享 Host、CLI/TUI/MCP、持久化同步记录及整体验收。继续使用 TDD，先记录 RED，再实现 GREEN。

## 连接与账户

新增 protocol 与 accounts crate。IMAP 使用 async-imap 的 Tokio 接口、Rustls 严格隐式 TLS，SMTP 复用 lettre。同步 Host 在后台线程执行有 deadline 的操作；正文原文与头部均交给现有 bubblewrap worker。任何证书、主机名或隔离失败均不回退。首轮不提供 STARTTLS 与明文生产配置。

账户配置只保存标识、端点、身份、文件夹范围与操作权限；密码、OAuth token 和 client secret 保存在显式 Secret Service 后端。系统凭据库不可用时拒绝认证，没有文件或 sample-store 降级。默认配置/数据采用 XDG 目录，保留显式路径覆盖及原离线演示命令。

OAuth 使用 oauth2 的随机 state 与 S256 PKCE、临时 loopback 回调；固定 Google 端点。代码交换、刷新、撤销使用禁用重定向、限时有界的 HTTPS 客户端。合成 OAuth 测试只允许独立的 loopback 测试构造，不修改生产端点。注册个人 Desktop client 与公开分发验证尚未完成，不能声称真实 Gmail 已验收。

人工密码/client secret 输入使用 rpassword 7（MIT OR Apache-2.0，上游 [rpassword](https://github.com/conradkleinespel/rpassword)，本轮锁定版本以 Cargo.lock 为准），仅在交互终端提示，不提供秘密参数或环境变量降级。文件导入/导出和运行锁使用已有 rustix 的 openat/NOFOLLOW 与 OS flock；不拼接 shell 命令。

参考：[async-imap](https://docs.rs/async-imap/latest/async_imap/)、[显式 Secret Service backend](https://docs.rs/zbus-secret-service-keyring-store/latest/zbus_secret_service_keyring_store/)、[oauth2](https://docs.rs/oauth2/latest/oauth2/)、[Google 桌面 OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)。各子模块补充选型、许可证与实际锁定版本。

## 邮件身份与同步

远程消息以账户、文件夹、UIDVALIDITY、UID 标识，拒绝旧 UIDVALIDITY。读取使用 EXAMINE 与 BODY.PEEK；模型不能通过参数切换账户范围、权限或标记已读。回复元数据与附件在隔离 worker 内提取；附件仅经用户 CLI 显式导入/导出，不自动打开。

草稿新增可选 In-Reply-To/References，纳入批准摘要；空字段省略以保持已有序列化和摘要兼容。SQLite 原子迁移到 schema 2，增加远程写入记录。在 APPEND 之前持久化单次认领；成功、失败与结果不确定均记录。Pending/Uncertain 不自动重试，进程重启不清除。草稿同步为显式版本快照追加，暂不自动删除旧服务器草稿，避免引入 EXPUNGE 风险。

Sent 策略由用户配置明确选择：ServerManaged 不由客户端 APPEND；Append 在 SMTP Accepted 后显式单次同步。Sent 查询按稳定 Message-ID 返回观察证据，不能将查无记录当成发送失败，也不能仅凭匹配推断最终送达。不提供绕过不确定状态的自动重发入口。

## 交互与验证

MCP 增加受限账户/文件夹列表、分页搜索、回复/转发草稿与显式草稿同步；保留无批准/改策略/文件路径工具的边界。TUI 使用后台读取与有界页面，加载/取消/错误可见，过期响应不替换当前选择。发送批准仍由完整人工 CLI 审阅产生。

验收包括真实本机 TLS IMAP/SMTP 往返、证书拒绝、PEEK/UIDVALIDITY、搜索转义、超大服务端响应、APPEND 与 DATA 后断连、权限拒绝、跨进程状态、OAuth 本机服务、正文 HTTP/DNS 隔离、MCP stdio 与实际终端。全部结果及缺口写入 docs；既有 71 项回归测试必须保持通过。
