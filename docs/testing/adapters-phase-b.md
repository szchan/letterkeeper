<!-- SPDX-License-Identifier: GPL-3.0-only -->
# 阶段 B 适配层验证

2026-10-13；合成数据，未访问真实账户。

RED：`cargo test --offline --locked -p letterkeeper-adapters --test phase_b`，3 测试中 2 失败；可编译 BrowserState stub 不产生首个请求，新增 MCP 路由返回 unknown tool。拒绝参数测试先通过，因为 stub 拒绝所有新增路由；GREEN 时另验成功路径及 Host 调用记录。

GREEN：`cargo test --offline --locked -p letterkeeper-adapters`，原有 13 项和新增 7 项通过。测试通过官方 SDK duplex client 完成 initialize/list_tools/call_tool，检查 14 个工具、reply 模式 enum、additionalProperties=false 和不可信文本去活化；MockHost 记录实际 folder/query/page/reply/sync 参数，拒绝账号切换、批准、路径、未知模式和超限 body/query。Frame 前 128 KiB 和工具参数 64 KiB 限制保持不变。新增 query 上限 1024 字节，不允许控制字符，reply body 上限 32768 字节；列表分页最多 50、offset 最多 100000，ID 仅 1–128 ASCII 字母数字/短横线/下划线。安全输出中的合法 id / *_id 值保留，其他字符串继续去活化；数组最多 50。Host 旧方法和无 folder 的 list_messages 行为保留；新增方法默认返回 unsupported。

TUI 不再预加载 50 封正文：进入终端后单个后台线程按需读取文件夹、50 条摘要或当前选择正文；UI 线程仅轮询 bounded channel 和键盘。两个 channel 容量各 1，同一时间仅一项任务在途，操作只替换一个 pending 意图，不创建额外线程。Esc 递增 generation，清除 pending 与阅读视图；旧响应丢弃。协议请求本身继续至 Host deadline，取消不意味着服务器请求已停止。q 不等待阻塞线程；Drop 不 join，结果通道关闭后线程在原操作结束时退出。默认不修改 Seen。

快捷键：j/k、上下键选择/滚动；Enter/Tab 按需阅读/返回；Esc 取消显示；r 刷新；n/p 50 条分页；f 轮换已列出的文件夹；/ 输入搜索（Enter 提交，Esc 清空取消），? 帮助，q 退出。分页、刷新、切文件夹和搜索均通过共享 Host 固定账户调用，无 UI 线程网络或发送批准接口。旧离线 Host 的 unsupported list_folders 回退既有默认列表；真实错误固定显示，不把 token 或 Host 错误带到终端。

新增慢 Host 测试以 channel 阻塞服务，证明 Esc/q 和 100 次刷新不阻塞主状态循环且服务只调用一次；页面过期响应、空列表、中文/宽字符及 100×25、24×10、1×1、0×0 渲染均验证。实际 PTY 键盘/终端恢复由集成代理运行，不用 TestBackend 替代真实终端验收。此模块测试不证明真实 IMAP/TLS/OAuth 或第三方 MCP 客户端展示安全；网络隔离、账户策略、服务器同步及完整链路由 Host/应用集成验证。

`cargo clippy --offline --locked -p letterkeeper-adapters --all-targets -- -D warnings` 和 `cargo fmt -p letterkeeper-adapters -- --check` 通过；初次 Clippy 指出 nested if 和 Option slice 的风格问题，已改为 let-chain / as_slice 后重新验证。搜索输入模式中的 q 为搜索字符；先 Esc 退出输入模式后 q 退出。

转发收件人边界回归：`cargo test --offline --locked -p letterkeeper-adapters --test phase_b forward_requires` 曾失败，证明初始实现允许无收件人的 forward。Host::reply_draft 最终参数为 `(id, mode, body, to: &[String])`。MCP optional `to` 最多 20 个、每项最多 254 字节且不含控制字符；forward 必须显式提供非空收件人，reply/reply_all 不得覆盖已有 Reply-To。实际邮箱地址合法性由共享 Host/core 再验证。无新增账号或批准字段。
