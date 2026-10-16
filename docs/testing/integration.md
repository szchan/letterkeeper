# 集成测试记录

2026-10-12，合成输入，无真实账户数据。

## RED：worker 边界

`cargo test --offline -p letterkeeper --test sandbox`：4 项，1 通过 / 3 失败。失败覆盖中文 HTML 隔离解析、内部 worker 不能直接作为无隔离入口、超大原文在启动 worker 前拒绝。失败原因分别为未实现、空 main 错误地成功和缺少大小判断；不是构建错误。

GREEN、网络观测、受控 SMTP 与实际终端验证见后续章节。

## RED：SMTP 边界

`cargo test --offline -p letterkeeper --test smtp`（获准本机测试服务）：5 项，2 通过 / 3 失败。失败为尚未实现 MIME/信封构造、SMTP 接受状态和 DATA 后断连不确定状态；测试服务只绑定 loopback，身份均为 example.test。

## RED：应用入口

`cargo test --offline -p letterkeeper --test cli`：2 项全失败，分别为尚未提供命令帮助和草稿持久化/拒绝管道批准流程。应用入口实现随后进行。

## GREEN：完整集成

最终命令 `cargo test --offline --locked --workspace`：**71 项通过，0 失败，0 忽略**。分组为 content 24、core 17、adapters 13、CLI 2、真实 MCP stdio 2、worker 5、SMTP/持久化闭环 8。库内与 doc-test 目前为 0，主要测试通过 crate 的公开接口放在 `tests/`；数量不表示所有产品 P0 已完成。

`cargo fmt --all -- --check` 与 `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` 通过。

### worker 与网络

- 输入先限 2 MiB，worker 512 MiB 地址空间、5 秒 CPU、8 秒 wall time、64 文件描述符和 0 文件写入配额；输出读取上限 4 MiB。
- bubblewrap 创建独立网络/PID/mount 等 namespace，清空环境，只读系统运行文件与当前可执行程序，不挂载用户目录或凭据；内部入口缺少 namespace 内标记时拒绝运行。
- HTTP TCP 和 DNS UDP 观察器先收到父进程正向控制；worker 解析包含外部图片、CSS、DNS 预取的合成 HTML，同时主动尝试连接这些端点和读取宿主哨兵文件。解析成功，HTTP/DNS 观察器零新增流量，宿主文件不可读。
- 缺少 sandbox、原文过大、直接调用内部 worker 均拒绝。没有联网或进程内渲染回退。

### 发送与授权

受控服务覆盖 SMTP 接受、明确 550 拒收、DATA 后关闭连接；观察单次连接及真实 RCPT，包括 Bcc，正文头不泄露 Bcc。Host 未批准时不连接；只读 Host 无法消耗批准。批准后断连持久化为 Uncertain，重开 Store/Host 后仍禁止重发；成功提交与不确定提交均不会被新批准绕过。例外恢复仅有核心显式接口，应用尚未提供进程死亡判定与恢复 UI。

### MCP 与终端

官方 rmcp client 通过实际子进程 stdin/stdout 完成初始化、列工具、读取正文、创建草稿及未授权发送拒绝。输出没有 HTML、图片内容块或活动 URL；无 approve/改策略工具，confirm/role 等字段被拒绝。额外实进程测试让 stdin 持续打开并写入超过 128 KiB 的无换行帧，进程关闭而非无限缓存。测试将 stdin handle 从 Child 中取出，避免 `Child::wait` 自动关闭 stdin 导致假通过。

`python3 tests/e2e/verify_terminal.py target/debug/letterkeeper` 已通过：真实 PTY 的 80×24 与 24×12，中文与宽字符输出、j/Enter/?/q、resize、终端属性恢复，以及人工输入完整摘要批准合成草稿。测试处理应用合法的光标控制序列；它不替代实际字体和审美评审。Ratatui TestBackend 另外覆盖极小/空尺寸和多封邮件的选择可见性。

公开 MCP 客户端/GUI 的最终渲染、真实网络协议、OAuth、Sent 同步、性能目标和完整产品行为仍未验收，见 [STATUS](../STATUS.md)。

## 独立审查与回归

已修复并记录回归：template 内容漏计 HTML 深度、消毒后附件名变为 `..` 或空、TUI 选中标题滚出、stdio 在 JSON 解码前缺少输入帧限制。正文、core、适配层各自保留 RED/GREEN 记录。实际终端脚本第一次将分隔中文字符的光标控制序列误判为文本缺失，修正测试观察方式后通过；未将其宣称为应用缺陷。
