# 正文模块验证记录

2026-10-12：在 `feat/content` worktree 先加入公开 API stub 与 18 项可编译集成测试，运行 `cargo test -p letterkeeper-content --offline`，结果为 **0 passed / 18 failed**（真实 RED）。失败覆盖 MIME/字符集、HTML 主动内容与资源、链接、附件 metadata、控制字符和资源边界。测试全部使用虚构身份及内存合成邮件，不含真实邮件。

2026-10-12：完成正文模块原型并补充 4 项回归测试。执行 `cargo test --offline -p letterkeeper-content`，结果为 **22 passed / 0 failed**；`cargo fmt -p letterkeeper-content -- --check` 和 `cargo clippy --offline -p letterkeeper-content --all-targets -- -D warnings` 均通过。

实现边界：`parse_message(raw, limits)` 先检查原文大小，再解析 MIME，并检查包含嵌套邮件的 parts 数量。`multipart/alternative` 优先纯文本，即使纯文本位于嵌套 multipart 中；无纯文本时才选可显示的 HTML。中文 UTF-8、编码头和 ISO-8859-1 quoted-printable 的合成用例已验证。选中的 HTML 经无资源获取能力的 html5ever DOM 解析；主动内容及 SVG/MathML 子树被舍弃，图片仅返回不含来源地址的占位符。链接只保留受限 HTTP(S)/mailto 目标，供用户主动判断，不自动访问。输出为纯文本、编号链接和附件元数据，显示字段共享字符预算；附件元数据作为完整条目纳入预算。原文、MIME parts、HTML DOM 深度和输出字符数都有明确限值，超过限值时拒绝或标记截断。终端控制字符与双向格式控制字符被清除。

这些单元测试不构成运行时 HTTP/DNS 隔离证明。库本身没有网络获取接口；生产调用仍须由应用的独立隔离 worker 执行，并由整合测试观察解析、TUI、MCP 和下游展示期间的 HTTP/DNS 行为。HTML DOM 在深度检查之前已由解析器构建，因此内存和 CPU 上界还依赖 worker 限额；调用方不得将原文 HTML、Markdown 或链接交给会自动预览/获取资源的组件。CID 内容目前不解码或显示，附件仅列元数据，不保存/打开。固定警告文字和条目 ID 不计入字符预算，列表长度由 MIME parts 上限约束。

直接依赖：`mail-parser` 0.11.9 用于 MIME、编码头和字符集解码（Apache-2.0 OR MIT）；`html5ever` 0.35.0 与 `markup5ever_rcdom` 0.35.0+unofficial 用于无资源获取接口的 HTML DOM 解析（MIT OR Apache-2.0）。版本与许可证取自本机离线包元数据；上游近期维护情况、安全审计和完整传递依赖许可证清单尚未核实，发布前仍需检查。安全影响是解析不可信输入及 DOM 构建的资源消耗，因此依赖原文/part/输出限制和生产 worker 的 CPU、内存、网络隔离。
