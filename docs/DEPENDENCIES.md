# 依赖清单

本清单记录当前原型工作区 `Cargo.lock` 锁定的 crates.io 依赖。机器可读的完整清单在 [dependencies.json](dependencies.json)，每项只包含名称、版本、上游声明的许可证、官方仓库地址及上游声明的 Rust 版本；未声明的 Rust 版本为 `null`。清单按集成工作区 `Cargo.lock` 经 Cargo metadata 得到，共 **384 个 registry package 条目、361 个不同 crate 名称**（同名多版本分别计数）；包括构建、测试及非 Linux 目标依赖，因此不是仅运行时依赖列表。

版本和元数据来自集成 worktree 的 `cargo metadata --offline --locked --format-version 1` 输出；完整 JSON 仅保留 name/version/license/repository/rust_version 五字段，没有绝对路径或包内容。直接依赖用途及已配置安全边界按 manifest、代码和 ADR 0002 记录交叉核对。没有声称依赖版本是最新或已完成安全审计。仓库地址仅作上游来源线索，不代表已审计代码或维护状态。对所有依赖的漏洞、安全、许可证兼容性均未完成审计或法律审查；发布前仍须核对依赖实际许可证原文、传递依赖和目标平台要求。项目原创源码与文档继续采用 `GPL-3.0-only`，不改变 [LICENSE](../LICENSE)。

## 直接依赖

下表记录 Stage A 与 Stage B 当前直接依赖的用途和 Cargo feature。维护证据仅限 Cargo 包元数据给出的上游仓库；没有核查近期提交、响应速度或安全公告，故均待发布前核实。

| 依赖（锁定版本） | 用途 | 包元数据许可证 | 维护证据 | 安全影响与边界 |
| --- | --- | --- | --- | --- |
| `mail-parser` 0.11.9 | 解析邮件 MIME、头部及正文部件。 | Apache-2.0 OR MIT | [stalwartlabs/mail-parser](https://github.com/stalwartlabs/mail-parser)；近期活跃度待核实。 | 处理不可信、可能畸形或超大的邮件输入；需要持续设置资源边界并验证解析结果。解析库本身不证明正文无外联。 |
| `html5ever` 0.35.0 | HTML5 文档解析。 | MIT OR Apache-2.0 | [servo/html5ever](https://github.com/servo/html5ever)；近期活跃度待核实。 | 解析阶段必须维持无资源获取行为；不能将解析成功或安全文本转换当作网络隔离证明。 |
| `markup5ever_rcdom` 0.35.0+unofficial | 为 html5ever 提供 RcDom 树表示，以遍历解析结果。 | MIT OR Apache-2.0 | [servo/html5ever](https://github.com/servo/html5ever)；近期活跃度待核实。 | 树中仍可能保留恶意标记和 URL；显示前需要安全转换，并验证下游不执行或加载内容。 |
| `rusqlite` 0.32.1（启用 `bundled`） | 本地状态及草稿数据的 SQLite 访问；bundled feature 构建随依赖打包的 SQLite。 | MIT | [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite)；近期活跃度待核实。 | 增加随包 SQLite 原生代码和构建链；输入仍需参数化，数据库文件权限及本地数据保护另行设计。 |
| `sha2` 0.10.9 | 对草稿版本化内容计算 SHA-256 摘要，用于授权快照一致性关联。 | MIT OR Apache-2.0 | [RustCrypto/hashes](https://github.com/RustCrypto/hashes)；近期活跃度待核实。 | 哈希不提供加密、认证或秘密保护；摘要算法用途不能代替发送授权。 |
| `serde` 1.0.229 | Rust 数据结构序列化与反序列化。 | MIT OR Apache-2.0 | [serde-rs/serde](https://github.com/serde-rs/serde)；近期活跃度待核实。 | 不可信数据反序列化仍需限制输入大小并校验业务约束。 |
| `serde_json` 1.0.154 | CLI、MCP 与内部数据的 JSON 编解码。 | MIT OR Apache-2.0 | [serde-rs/json](https://github.com/serde-rs/json)；近期活跃度待核实。 | JSON 编解码不会自动清理邮件内容；输出消费者仍须把不可信正文当文本处理。 |
| `thiserror` 2.0.21 | 定义 Rust 错误类型。 | MIT OR Apache-2.0 | [dtolnay/thiserror](https://github.com/dtolnay/thiserror)；近期活跃度待核实。 | 错误消息不应携带邮件正文、凭据或其他私有数据。 |
| `tokio` 1.53.2 | 异步运行时、MCP stdio、进程及 I/O 支持。 | MIT | [tokio-rs/tokio](https://github.com/tokio-rs/tokio)；近期活跃度待核实。 | 启用的功能含运行时、同步、时间、I/O 和测试/进程路径；外部进程必须使用参数数组并保持权限边界。 |
| `ratatui` 0.29.0 | 终端界面绘制。 | MIT | [ratatui/ratatui](https://github.com/ratatui/ratatui)；近期活跃度待核实。 | 邮件正文含控制字符和双向文本等风险；绘制层仍须转义或安全呈现。 |
| `crossterm` 0.28.1 | 终端事件和终端后端。 | MIT | [crossterm-rs/crossterm](https://github.com/crossterm-rs/crossterm)；近期活跃度待核实。 | 终端控制序列可能改变终端状态或伪造界面；不可信正文不能原样输出。 |
| `rmcp` 3.5.1（精确锁定；server/transport-io，测试含 client） | MCP stdio 服务与测试客户端。 | Apache-2.0 | [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk/)；近期活跃度待核实。 | MCP 工具参数不是用户授权；邮件内容必须视为不可信输入，账户范围和发送策略由本地核心策略实施。 |
| `clap` 4.6.7（derive） | 命令行参数与子命令解析。 | MIT OR Apache-2.0 | [clap-rs/clap](https://github.com/clap-rs/clap)；近期活跃度待核实。 | 命令行参数可能出现在进程列表或历史记录；不得通过参数传递密码、token 或正文秘密。 |
| `lettre` 0.11.23（builder、SMTP、Rustls、ring、webpki-roots） | 构造邮件并通过 SMTP 发送；配置使用 Rustls 和内置 WebPKI 根。 | MIT | [lettre/lettre](https://github.com/lettre/lettre)；近期活跃度待核实。 | 处理 SMTP 凭据和收件人等敏感数据；网络超时可能导致发送结果不确定，调用方不能盲目重试。此依赖不代表当前已验证真实账户收发。 |
| `libc` 0.2.190 | Unix 资源限制接口，供 Linux 沙箱 worker 设置资源上限。 | MIT OR Apache-2.0 | [rust-lang/libc](https://github.com/rust-lang/libc)；近期活跃度待核实。 | 是底层系统接口封装；资源限制调用失败必须被视为失败，不能假定隔离仍然有效。 |
| `tempfile` 3.27.0（应用与测试） | 创建沙箱只读标记的临时文件，以及合成测试目录。 | MIT OR Apache-2.0 | [Stebalien/tempfile](https://github.com/Stebalien/tempfile)；近期活跃度待核实。 | 临时测试数据应使用合成内容；生产缓存和敏感文件生命周期不由此依赖自动保证。 |

### Stage B 新增直接依赖

以下用途和安全边界按集成工作区 manifest、代码及 ADR 0002 交叉核对；上游链接来自 Cargo 包元数据。除 ADR 已明确记录的来源核验外，近期活跃度、安全公告与漏洞状态未独立审查，不能据此声称已完成安全审计。

| 依赖（锁定版本） | 用途 | 包元数据许可证 | 维护证据 | 安全影响与边界 |
| --- | --- | --- | --- | --- |
| `async-imap` 0.12.0 | IMAP 协议解析和会话处理，运行在受限 TLS transport 之上。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/async-email/async-imap)；近期维护状态未审计。 | 处理不可信服务器响应；操作上限和 deadline 仍由 protocol 层负责。 |
| `futures-util` 0.3.34 | IMAP adapter 使用的 Stream 和 Future 工具。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/rust-lang/futures-rs)；近期维护状态未审计。 | 异步组合本身不提供 protocol 资源上限。 |
| `tokio-rustls` 0.26.6 | 为 IMAP TLS 将 Tokio stream 接入 rustls。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/rustls/tokio-rustls)；近期维护状态未审计。 | 发送凭据前必须完成证书链和主机名验证。 |
| `rustls` 0.23.45 | TLS 客户端配置和证书验证，启用 ring 与 TLS 1.2。 | Apache-2.0 OR ISC OR MIT | [上游仓库](https://github.com/rustls/rustls)；近期维护状态未审计。 | 不绕过验证；额外 PEM 根仅增加信任锚。 |
| `rustls-pemfile` 2.2.0 | 解析用户配置的附加 PEM 信任锚。 | Apache-2.0 OR ISC OR MIT | [上游仓库](https://github.com/rustls/pemfile)；近期维护状态未审计。 | 无效证书输入应拒绝，不能关闭常规验证。 |
| `webpki-roots` 1.0.9 | 用于服务器证书验证的 Mozilla 派生根证书集。 | CDLA-Permissive-2.0 | [上游仓库](https://github.com/rustls/webpki-roots)；近期维护状态未审计。 | 根集合影响信任决定；许可证来自包元数据声明，尚未核对实际许可证文件。 |
| `zeroize` 1.9.1 | Secret wrapper 在 drop 时清理，以及 protocol 凭据处理。 | Apache-2.0 OR MIT | [上游仓库](https://github.com/RustCrypto/utils)；近期维护状态未审计。 | 尽力清除内存；HTTP/TLS 依赖内部副本不保证清零。 |
| `log` 0.4.34 | 编译时将 protocol 诊断日志最高级别设为 debug。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/rust-lang/log)；近期维护状态未审计。 | 启用了 debug 级别；日志调用不得输出凭据、正文、URL 或上游错误文本。 |
| `secret-service` 5.2.0 | Linux Secret Service 凭据后端，使用 async-io 与 RustCrypto 加密 features。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/hwchen/secret-service-rs)；近期维护状态未审计。 | 要求加密 DH；服务不可用或锁定时失败关闭，不回退到明文或文件存储。 |
| `oauth2` 5.0.0 | OAuth 授权、state/PKCE、token 请求构造和解析；禁用默认 features。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/ramosbugs/oauth2-rs)；近期维护状态未审计。 | token 保留在凭据库；state/PKCE 和严格端点/响应限制由应用负责。 |
| `reqwest` 0.12.28 | OAuth 使用的 blocking rustls HTTP transport；禁用默认 features。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/seanmonstar/reqwest)；近期维护状态未审计。 | 关闭 redirect 和 proxy；调用端实施 deadline 与 64 KiB 响应上限。 |
| `url` 2.5.8 | 解析和校验 OAuth callback URL。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/servo/rust-url)；近期维护状态未审计。 | URL 解析本身不代表 callback 可信；仍核对固定 loopback host/path/state。 |
| `rustix` 1.1.5 | 私有账户存储与锁使用的 fd-relative filesystem/process 原语。 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [上游仓库](https://github.com/bytecodealliance/rustix)；近期维护状态未审计。 | 安全边界依赖调用点的 no-follow、owner、mode 和目录 fd 检查。 |
| `rpassword` 7.5.4 | 从终端交互读取凭据且不回显。 | Apache-2.0 | [上游仓库](https://github.com/conradkleinespel/rpassword)；近期维护状态未审计。 | 仅用于交互终端读取；值不得进入日志、参数或普通配置。 |
| `rcgen` 0.14.10 | 为合成 loopback protocol 测试生成临时 TLS 证书，仅用于开发/测试。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/rustls/rcgen)；近期维护状态未审计。 | 仅测试使用；私钥临时生成，不应提交到仓库。 |
| `rustls-pki-types` 1.15.1 | 测试 TLS 配置使用的 PKI 类型。 | MIT OR Apache-2.0 | [上游仓库](https://github.com/rustls/pki-types)；近期维护状态未审计。 | 仅测试证书/key 表示，不单独定义生产信任策略。 |

## 系统依赖与平台限制

原型的正文沙箱通过固定路径 `/usr/bin/bwrap` 启动 bubblewrap。当前本机 Arch Linux 的只读包信息确认 bubblewrap 版本为 `0.12.0-1`，许可证字段为 `LGPL-2.1-or-later`，上游仓库为 [containers/bubblewrap](https://github.com/containers/bubblewrap)。这只是本机包元数据证据，不是对其他发行版、安装布局或功能行为的验证。当前固定的 Arch/Linux 路径和启动方式限制了可移植性；非 Linux 目标依赖虽然会出现在 Cargo 锁定清单中，原型沙箱仍未证明能在其上运行。缺少程序、权限或必需隔离能力时应拒绝处理，而不能回退为普通渲染。近期维护状态待核实。

## 适用范围

此清单描述 Stage B 集成工作区的锁定依赖，不表示项目已经实现完整邮件客户端，也不构成安全漏洞审计、隐私保证或法律意见。源码已核对的事实包括 OAuth、凭据存储及 IMAP/TLS 选型与配置；尚未完成真实 Gmail/服务商端到端验证、近期维护/公告审查、全部传递依赖许可证原文核对及目标平台审查。Cargo 清单覆盖锁文件解析到的构建、测试及目标平台依赖；JSON 中未包含本地 workspace crate，因为它们不是第三方 registry 依赖。发布前应重新生成并审查清单，逐项核对许可证原文及依赖树，确认所发布目标的系统依赖和实际沙箱能力。
