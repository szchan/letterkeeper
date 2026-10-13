# 账户与 OAuth 验证

2026-10-13；只使用合成身份、内存 fake vault 与受控 loopback 服务。不写真实系统凭据，不启动外部浏览器，不访问真实 Gmail。

RED：`cargo test -p letterkeeper-accounts --test accounts`（实现尚不存在，预期编译失败）；真实结果 exit 101，缺少 AccountConfig/CredentialStore/AccountRepository API，23 个编译错误；不是行为断言失败。配置验证覆盖成功/拒绝、账户命名空间、文件夹权限、凭据分离、符号/硬链接、权限及大小边界。OAuth 测试随后覆盖完整 code/callback/token/refresh/revoke 状态机。

OAuth RED：`cargo test --offline -p letterkeeper-accounts --test oauth`，exit 101，OAuthLogin/GmailOAuth 等 API 缺失（编译 RED）。

## GREEN 与集成验证

`cargo test --offline -p letterkeeper-accounts`：2026-10-13，账户 8 个、OAuth 10 个测试通过（XDG 子进程额外执行两次），无 ignored。`cargo fmt --all -- --check` 与 `cargo clippy --offline -p letterkeeper-accounts --all-targets -- -D warnings` 通过。全 workspace 验证由主分支集成执行；本 worktree 不提交根 manifest/lock。

行为覆盖：有效/未知配置、命名空间、身份控制字符、文件夹范围、TLS 端点、0600/0700、symlink/hardlink、64 KiB 上限、文件名绑定账户 ID、原子替换、XDG 和绝对目录覆盖；凭据按账户隔离，缺失拒绝且 Secret Debug 不泄漏。

受控 OAuth 服务观测实际 POST：S256 verifier、code exchange、cache 不续期、expiry 续期、refresh rotation、服务器不返回 refresh 时保留旧值、invalid_grant 重新授权、权限不足、client secret vault 引用、注销、撤销成功与失败。callback 检查随机 state/port、owner 绑定、重复参数/code/error、Host/path/method、拒绝不完整与超时请求；token 类型、expires 与 scope、超大/错误响应边界。HTTP 302 fixture 的独立目标监听器没有接受到连接。没有真实 keyring 操作、外部浏览器或 Google 请求。

审查追加 RED：`... --test oauth oauth_login_is_bound_to_the_originating_account` 真实 exit 101（Timeout != InvalidConfiguration）；修复绑定 originating account/client/endpoint。`... --test oauth refresh_is_serialized_across_instances_in_one_process` 真实 exit 101（并发请求 OAuthRequestFailed），在延迟 token 服务下复现双 refresh；修复进程内 vault token 状态操作 mutex，GREEN 覆盖两个不同 OAuth 实例只执行一次 refresh。

## 已知限制

- 真实 Google 授权、client 注册/公开发布 scope 审核、系统 Secret Service daemon 解锁兼容性尚未授权或验证；实际 Dh 适配已编译。锁定/unavailable vault 失败关闭，要求用户通过系统 UI 解锁，绝无文件/Plain fallback。
- 库内 mutex 只跨同一进程；生产 CLI/MCP 必须通过集成层每账户 auth.lock 的 flock 串行 get/refresh/set 与 login/logout/revoke，直接库使用者应自行协调多个进程。
- Secret wrapper 和持久化 bundle 字符串会清零；第三方 OAuth/HTTP/DBus 库的内部复制与分配不声称全部清零。代码无日志/println，所有错误为固定字符串；整个程序仍需阻断上游 TRACE 日志以避免 wire 内容泄漏。
- 配置和缓存未静态加密；0700/0600 不保护同 UID 任意 shell 攻击者。目录 fd+NOFOLLOW 防止配置路径中的符号链接；同 UID 主动更换祖先目录不在声明的边界中。
- 没有更改 README/STATUS，也不声称全部 AUTH/P0 或真实邮箱闭环已完成；集成代理负责跨文档状态同步。

最终硬化 RED：callback length 断言真实 exit 101（43 != 44），以及 `private_permissions_reject_special_bits_and_changed_parent_modes` 真实 exit 101（04600 被错误接受）。修复响应长度取 body.len()，精确比较 07777 模式，并在每次 load/save/list 验证配置和数据目录 fd 权限。最终 GREEN 为 8+10 测试及 Clippy；没有跳过失败项。
