# SMTP 类型化诊断：协议层验证

日期：2026-10-15。范围：MAIL-01、MAIL-07、AUTH-02、DATA-02、MCP-09 的协议诊断增量；只使用合成身份、邮件和本地受控 TLS 服务。不代替真实 Gmail、系统凭据库或应用层持久化/展示验收。

## 接口与边界

`send_smtp_report` 与原 `send_smtp` 使用相同参数；原接口作为兼容 wrapper，仅返回相同的 Accepted/Failed/Uncertain 三态。Accepted 的 diagnostic 为 None，失败与不确定返回固定枚举 stage/kind 和可选数值 smtp_code。无服务器原始回复、地址、正文、凭据或底层错误字符串。

阶段为 Preparation、TlsConfiguration、Connection、TlsHandshake、Greeting、Authentication、MailFrom、Recipient、DataCommand、Submission。Greeting 包含问候与 EHLO；Submission 包含 DATA 内容传输及最终响应，不声称能进一步区分其间的网络失败。数值码只从 lettre 的 typed Code / Error::status 提取，限制在 100..599，不解析错误显示字符串。错误类别为 InvalidInput、Connection、Timeout、Rejected、UnexpectedResponse。

整个网络操作继续使用同一个 15 秒 deadline 与既有 runtime bridge；原子阶段记录在失败或外层超时后读取，不跨网络 await 持锁。接收 354 后的非明确拒绝错误保持 Uncertain；提交后的明确 4xx/5xx 仍是 Failed。不会重试。Failed 本身不能证明从未发送 DATA 内容。

`validate_smtp_tls` 只检查端点、ServerName、额外 CA PEM 和 ClientConfig 构建，不 DNS、连接、认证或读取文件/凭据。它与 IMAP/SMTP 连接共用内部配置 helper，保留默认 roots 加 extra CA、64 KiB PEM 上限、证书/主机名验证等信任策略。校验通过不代表实际握手或认证通过。

## 远程 TDD 证据

所有测试在 GitHub-hosted Ubuntu 24.04.5、Rust/Cargo 1.99.0 运行；本机只编辑、rustfmt、Git 与日志审查，不运行 Cargo build/check/test/Clippy。

| 性质 | 被测完整 SHA | Actions | 实际结果 |
| --- | --- | --- | --- |
| AUTH 拒绝必须保留阶段/类别/535 | `d02d79210aca067da67ad087acd6d9c25ecd1c66` | [初始 RED](https://github.com/szchan/letterkeeper/actions/runs/38709805368) | 构建成功，恰好 1 测试；None 与 Authentication/Rejected/535 的预期断言失败；原始 exit 101，conclusion failure |
| 本地 TLS 校验拒绝无效 PEM | `9271b376039a01c798c44df1379a204a4e5bcf5f` | [校验 RED](https://github.com/szchan/letterkeeper/actions/runs/38729860078) | 构建成功，恰好 1 测试；stub 的 Ok(()) 与 Err(InvalidInput) 断言失败；原始 exit 101，conclusion failure |
| AUTH 诊断初步实现 | `9271b376039a01c798c44df1379a204a4e5bcf5f` | [中间 GREEN](https://github.com/szchan/letterkeeper/actions/runs/38729864887) | 恰好 1 测试，原始 exit 0，conclusion success；不代表该中间 revision 全门禁通过 |
| 本地 TLS 校验实现 | `01c35ac45377c7d12eaaaf7198a43e0c2d125fb2` | [校验 GREEN](https://github.com/szchan/letterkeeper/actions/runs/38730154681) | 恰好 1 测试，原始 exit 0，conclusion success |
| AUTH 最终代码回归 | `01c35ac45377c7d12eaaaf7198a43e0c2d125fb2` | [AUTH GREEN](https://github.com/szchan/letterkeeper/actions/runs/38730160675) | 恰好 1 测试，原始 exit 0，conclusion success |
| 最终代码完整门禁 | `01c35ac45377c7d12eaaaf7198a43e0c2d125fb2` | [完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38730136954) | workspace、codex-client、ci-gate 全部 success；协议 42/42，完整工作区测试、fmt/Clippy、构建、实际 PTY、HTTP/DNS 正向控制与实际 Codex 客户端均通过 |

中间 SHA `9271b376039a01c798c44df1379a204a4e5bcf5f` 的[完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38710445819) 如实保留失败：协议 40 通过、2 失败。一项为尚未实现的 TLS 校验 stub；另一项为测试过度规定突然 TLS EOF 必定产生 UnexpectedResponse，实际依赖报告 Connection。已修正断连测试：传输失败和不完整回复均保留固定类别，强制检查 Submission、无数值码、Uncertain 和一次 DATA；没有改变生产发送语义或跳过该测试。

精确测试命令供远程 runner 使用：

```sh
cargo test --locked -p letterkeeper-protocol smtp_report_preserves_auth_rejection_stage_without_server_text -- --exact --nocapture
cargo test --locked -p letterkeeper-protocol smtp_tls_validation_rejects_invalid_configuration_without_network -- --exact --nocapture
cargo test --locked -p letterkeeper-protocol
```

workflow_dispatch 使用 main 上的 `tdd.yml`、上述完整 commit、crate `letterkeeper-protocol`、精确测试名与 RED/GREEN 标签。以日志中的 TDD_EVIDENCE commit、matched_tests、raw_exit_code 核验被测版本；dispatch 事件自身 headSha 不是目标 checkout 的替代。

文档补充后的任务分支最新 revision 仍由 [PR #14](https://github.com/szchan/letterkeeper/pull/14) 自动重新运行完整 CI；该门禁必须通过后才集成。主代理集成产生的新 SHA 也须独立通过完整门禁，不能以此分支旧 SHA 的 GREEN 替代。

## 受控覆盖

- 接受成功；EHLO/AUTH/MAIL/RCPT/DATA 拒绝；提交最终 550/451；异常正响应和畸形响应；数值码与固定类型的脱敏输出。
- 不可信 CA、错误主机名在 TLS 握手失败，服务端未收到凭据或命令；无效输入、凭据、PEM、端点在连接前拒绝；合法系统 roots 与额外合成 CA 校验成功。
- 连接拒绝、TLS/AUTH/提交阶段超时；提交后断连/异常最终响应保持 Uncertain，明确最终拒绝为 Failed；全部单次尝试。
- 普通 std thread、Tokio entered runtime 与 spawn_blocking 的 SMTP 调用；兼容原 SMTP 三态测试，保留完整 IMAP/网络边界回归。

既有历史提交没有阶段信息，本增量不能推导或回填其根因。应用层需要单独消费并保存诊断；本任务没有修改其持久化、CLI、TUI 或 MCP 输出。
