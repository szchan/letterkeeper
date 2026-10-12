# TUI / MCP adapter 原型验证

2026-10-12。对应 UX-01、MCP-01/03/04/05/07/08/09、PRIV-07/08。

此模块仅负责共享 Host 的适配；Host 固定账户范围、凭据保护和发送授权。MCP 不提供批准、账户切换、修改策略、文件路径、链接目标、附件内容或读取本机文件的工具。第一轮草稿输入只有 `to`、`subject`、`body`；cc/bcc、附件及回复线程仍留待后续。`send_draft` 只请求发送已被人工 Host 批准的草稿，模型的 `confirm`、`role` 或任何未知字段均被拒绝。失败响应采用固定无秘密文案，不回显 Host 错误。

协议由 [官方 Rust SDK](https://github.com/modelcontextprotocol/rust-sdk) `rmcp = 3.5.1` 实现，使用 stdio；无自制 JSON-RPC/MCP framing。2026-10-12 通过 crates.io `cargo info rmcp@3.5.1` 核实版本、官方仓库、Apache-2.0 和 Rust 1.88 最低要求。只打开 server/transport-io 功能，开发测试打开 client；未启用 HTTP、OAuth、资源加载或客户端 HTTP 功能。Ratatui 0.29.0 (MIT)、crossterm 0.28.1 (MIT) 用于终端布局和输入，官方 SDK 与两者均是维护中的 upstream 项目。依赖不能取代应用的隔离边界。

所有正文、主题、发件人、草稿响应字符串进入保守 `inert_text` 路径：过滤终端控制和双向控制，将 Markdown/HTML 和链接所需 ASCII 标点转换为全角或可见替代符，链接只返回内部编号，目标完全不进入默认响应。此转换故意损失文本保真，不能当作 HTML 消毒器或证明上游解析没有联网。响应标明 `untrusted`，服务器指示客户端按纯文本显示并禁用预览。未知第三方客户端仍可能做额外解析、联网或把邮件当作指令；本原型不声称已证明这些客户端安全。MCP 协议响应没有 image/resource 内容块。

消息默认每页 20，上限 50；正文默认 4096 个 Unicode scalar，上限 16384；显式 offset 分页。输入 JSON 最多 64 KiB，正文输入最多 32 KiB，内部 ID 最多 128 ASCII 字符，仅字母数字、连字符、下划线。列表或正文最后一页的 `next_offset` 为 null；列表恰好填满一页时，下一页可能为空。Host 返回字符串再次限制长度，任何 adapter 裁剪通过 `adapter_truncated` 标注；TUI 阅读页顶部提示正文截断，不能据此完成全量草稿审阅；附件只呈现数量。正文页偏移过大、未知字段、未知 ID 与越界参数都会拒绝。生产 Host 自身仍必须有有界解析和存储。

TUI 在进入 raw mode 前预加载最多 50 封离线合成邮件，之后绘制和输入循环不执行 Host 操作。上下/j/k 列表导航或阅读滚动，Enter/Tab 切换，Esc 返回，? 帮助，q 退出。RAII guard 负责常规返回或 panic unwind 的终端恢复。网络加载、异步刷新、取消、分页 UI、TUI 草稿编辑及人工批准界面未实现；不能将此预加载设计直接用于真实网络账户。

## TDD 与执行结果

`818e5c5` 是 RED 测试提交：实际执行 `cargo test -p letterkeeper-adapters --offline` 因尚不存在 `inert_text`/`UiState` 而失败。实现后同一测试通过，并扩充边界和官方 SDK 协议测试。新增末页正文测试实际 RED：预期 null，但旧实现返回 104095；随后修复。

可执行验证：

```sh
cargo test -p letterkeeper-adapters --offline
```

已通过 10 个集成测试：

- 注入 raw HTML、Markdown image、URL、ANSI 与中文，输出不含可执行格式或原链接方案，按字符数截断。
- 空列表、j/k、Enter/Tab、q 的键盘状态及有界选择。
- Ratatui TestBackend 的 100×30、24×12、1×1、0×0，在列表/阅读模式渲染中文及 emoji，不 panic，不输出控制序列或 URL。
- 严格参数解码拒绝 confirm/role/path 风格 ID；检查工具白名单、分页默认值与上限、未知邮件 ID 和正文截断；拒绝参数不会调用 Host 发送。
- Tokio duplex 上使用官方 rmcp client/server 完成 initialize、tools/list、tools/call、错误返回及 shutdown；没有自行拼接协议报文。

TestBackend 是终端 cell 验证，不是实际字体、真实终端键盘或视觉精致度验收。没有在此模块声称 HTTP/DNS 无外联完整证明：应用 worker 隔离及受控网络观测由应用/content 测试负责，未验证公开 MCP 客户端下游展示。全部数据为合成身份和恶意样本，没有真实邮箱、凭据或私有邮件。

额外覆盖六条工具路由、Host 未授权发送返回协议 tool error、大小限制、附件/approve 字段拒绝、最后/空列表页、长草稿裁剪声明、TUI 顶部截断提示及重复键盘边界。`cargo clippy -p letterkeeper-adapters --all-targets --offline -- -D warnings` 用于静态质量检查。

## 选中邮件可见性回归

后续审查发现列表每项实际两行，旧版却按 `selected * 3` 滚动，80×15 终端在选择第二封时会隐藏其主题及选择箭头。`f53bcc5` 加入 TestBackend 回归并实际 RED：选中主题和箭头未出现在输出 cells 中。修复使用 Ratatui `List`/`ListState`，由 widget 根据每项高度维护选中项可见性，不手工估算行数。主题/发件人按终端宽度裁切；完整主题仍可进入阅读页查看。

回归覆盖 12 封中文邮件，80×15 和 24×15，两种宽度连续 j 下移八次并 k 上移，逐次断言选中箭头与主题可见。全部 11 个测试、rustfmt 和 clippy `-D warnings` 通过。实际字体、键盘和视觉评审限制不变。

## stdio 解码前帧大小限制

官方 rmcp 3.5.1 `AsyncRwTransport` 的 `receive` 使用 `read_until` 累积字节，源码未提供每行长度上限；工具参数的 64 KiB 检查发生在 JSON 解码之后，不能保护初始帧。因此 `run_mcp` 在官方 SDK 的 AsyncRead 输入前安装 `BoundedReader`，每行最多 128 KiB（包括换行），跨 poll 累计计数并在换行后重置。计数超限返回固定 `InvalidData` I/O 错误且后续读取永久失败，SDK 负责关闭 transport。计数器不解析 JSON、不实现 MCP framing，输出和协议解码仍由官方 SDK 完成；无新增依赖。

reader 使用 4 KiB 固定栈缓冲，只有验证通过的 chunk 才交给 SDK；未终止的超长输入也会在达到上限后拒绝，不需等待 EOF 或换行。空目标缓冲返回成功而不消费输入，EOF 保持标准 AsyncRead 行为。此上限限制单帧累积，不替代下游结构/正文大小限制或业务层资源约束。

RED `88bbd81`：实际测试因 `BoundedReader` 尚不存在而编译失败。实现后 13 个测试通过，其中覆盖无换行超长输入、4-byte duplex 导致的跨 poll 累积、超限后永久失败、合法多帧计数重置、空目标缓冲与 EOF。官方 SDK initialize/list/call 测试也通过同一 reader wrapper。rustfmt 与 clippy `-D warnings` 通过。实际子进程超长 stdin 验证由 root 应用测试补充。
