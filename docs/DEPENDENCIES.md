# 依赖清单

本清单记录当前原型工作区 `Cargo.lock` 锁定的 crates.io 依赖。机器可读的完整清单在 [dependencies.json](dependencies.json)，每项只包含名称、版本、上游声明的许可证、官方仓库地址及上游声明的 Rust 版本；未声明的 Rust 版本为 `null`。清单包括构建、测试及非 Linux 目标依赖，因此不是仅运行时依赖列表。

版本和元数据来自集成工作区的 `cargo metadata --offline --locked --format-version 1` 结果；未从网络核查最新版本或近期维护活动。仓库地址仅作上游来源线索，不代表已审计代码或维护状态。对所有依赖的漏洞、安全、许可证兼容性均未完成审计或法律审查；发布前仍须核对依赖实际许可证原文、传递依赖和目标平台要求。项目原创源码与文档继续采用 `GPL-3.0-only`，不改变 [LICENSE](../LICENSE)。

## 直接依赖

下表用途按当前原型代码和 Cargo 配置描述。维护证据仅限锁定版本与包元数据给出的官方仓库；没有核查近期提交、响应速度或安全公告，故均待发布前核实。

| 依赖（锁定版本） | 用途 | 包元数据许可证 | 维护证据 | 安全影响与边界 |
| --- | --- | --- | --- | --- |
| `mail-parser` 0.11.9 | 解析邮件 MIME、头部及正文部件。 | Apache-2.0 OR MIT | [stalwartlabs/mail-parser](https://github.com/stalwartlabs/mail-parser)；近期活跃度待核实。 | 处理不可信、可能畸形或超大的邮件输入；需要持续设置资源边界并验证解析结果。解析库本身不证明正文无外联。 |
| `html5ever` 0.35.0 | HTML5 文档解析。 | MIT OR Apache-2.0 | [servo/html5ever](https://github.com/servo/html5ever)；近期活跃度待核实。 | 解析阶段必须维持无资源获取行为；不能将解析成功或安全文本转换当作网络隔离证明。 |
| `markup5ever_rcdom` 0.35.0+unofficial | 为 html5ever 提供 RcDom 树表示，以遍历解析结果。 | MIT OR Apache-2.0 | [servo/html5ever](https://github.com/servo/html5ever)；近期活跃度待核实。 | 树中仍可能保留恶意标记和 URL；显示前需要安全转换，并验证下游不执行或加载内容。 |
| `rusqlite` 0.32.1（启用 `bundled`） | 本地状态及草稿数据的 SQLite 访问；bundled feature 构建随依赖打包的 SQLite。 | MIT | [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite)；近期活跃度待核实。 | 增加随包 SQLite 原生代码和构建链；输入仍需参数化，数据库文件权限及本地数据保护另行设计。 |
| `sha2` 0.10.9 | 对草稿版本化内容计算 SHA-256 摘要，用于一致性/幂等状态关联。 | MIT OR Apache-2.0 | [RustCrypto/hashes](https://github.com/RustCrypto/hashes)；近期活跃度待核实。 | 哈希不提供加密、认证或秘密保护；摘要算法用途不能代替发送授权。 |
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
| `tempfile` 3.27.0（开发/测试） | 测试期间创建临时文件和目录。 | MIT OR Apache-2.0 | [Stebalien/tempfile](https://github.com/Stebalien/tempfile)；近期活跃度待核实。 | 临时测试数据应使用合成内容；生产缓存和敏感文件生命周期不由此依赖自动保证。 |

## 系统依赖与平台限制

原型的正文沙箱通过固定路径 `/usr/bin/bwrap` 启动 bubblewrap。当前本机 Arch Linux 的只读包信息确认 bubblewrap 版本为 `0.12.0-1`，许可证字段为 `LGPL-2.1-or-later`，上游仓库为 [containers/bubblewrap](https://github.com/containers/bubblewrap)。这只是本机包元数据证据，不是对其他发行版、安装布局或功能行为的验证。当前固定的 Arch/Linux 路径和启动方式限制了可移植性；非 Linux 目标依赖虽然会出现在 Cargo 锁定清单中，原型沙箱仍未证明能在其上运行。缺少程序、权限或必需隔离能力时应拒绝处理，而不能回退为普通渲染。近期维护状态待核实。

## 适用范围

此清单描述当前原型的锁定依赖，不表示项目已经实现完整邮件客户端，也不构成安全漏洞审计、隐私保证或法律意见。Cargo 清单覆盖锁文件解析到的构建、测试及目标平台依赖；JSON 中未包含本地 workspace crate，因为它们不是第三方 registry 依赖。发布前应重新生成并审查清单，逐项核对许可证原文及依赖树，确认所发布目标的系统依赖和实际沙箱能力。
