# 阶段 B 集成验证

2026-10-13，仅合成邮件及本机受控服务。

## 隔离正文、附件与回复头 RED

`cargo test --offline -p letterkeeper --test worker_composition` 编译成功，4 项全部行为失败：inspect/attachment 接口 stub 尚未实现、内部新入口未路由到隔离检查、SMTP 未输出批准的线程头、附件导入/导出未实现。最终 GREEN 与应用装配提交 991ca42 见下文。

## 共享远程 Host RED

`cargo test --offline -p letterkeeper --test remote` 编译成功，4 项全部失败，原因均为远程 Host 尚未装配。测试覆盖跨 TLS 会话读取/回复/草稿同步、权限先于凭据获取、MCP 去活化及拒绝自授权、DATA 后断连与重启状态。

## 账户 CLI RED

`cargo test --offline -p letterkeeper --test account_cli`：初始 1 通过/1 失败，失败为尚无 config-dir/账户命令。发现命令帮助断言只检查子串会把描述中的 account/mail 当成命令，改为检查独立命令行；再次 RED 为 2 项全部失败。测试不调用真实系统凭据库，密码/批准管道必须拒绝。

## 审查与跨模块回归 RED

Sent 文件夹范围测试去掉检查后实际失败（凭据获取 1 次而非 0 次），随后恢复检查。凭据缺失时草稿同步错误地返回 Failed 并永久认领版本，新增 `missing_credentials_do_not_consume_a_draft_sync_revision` 实际断言失败；修复将在 APPEND 认领前完成凭据/CA 本地预检，不联网。

MCP `spawn_blocking` 与协议同步桥接不兼容由实际 TLS 集成暴露，在 protocol 模块补 RED/GREEN。部分初始测试把自身 worker fixture 的身份套用到共享协议 fixture，核对后改为后者真实的 Reply-To 和 Message-ID；没有改动应用输出去迎合错误身份。

## GREEN：最终集成

`cargo test --offline --locked --workspace`：**170 项通过，0 失败，0 忽略**。按顶层注册的测试计数，不重复加上 subprocess probe 子运行。

| 模块 | 数量 |
| --- | ---: |
| 应用 CLI/进程锁/stdio/远程 Host/worker/SMTP/真实 TLS PTY | 36 |
| 账户/路径/OAuth/凭据协调 | 23 |
| TUI/MCP 适配 | 22 |
| MIME/安全文本/回复/附件 | 38 |
| 授权/SQLite/线程头/同步 | 30 |
| TLS IMAP/SMTP 协议 | 21 |

库内目前只有账户 runtime 校验的 1 项单元测试，其余大多通过 crate 公开接口放在 tests/；若干 probe 用于子进程测试，不等于独立业务覆盖。doc-test 为 0。既有阶段 A 的 71 项测试仍在（工具列表从 6 改为真实的 14），总数不代表所有 P0 已完成。

`cargo fmt --all -- --check`、`cargo clippy --offline --locked --workspace --all-targets -- -D warnings` 通过。依赖 JSON 与最终锁文件 384 项 registry 版本/许可证逐项匹配；LICENSE 未修改。

最后的真实 TLS 回归新增 3 项：中文搜索使用 `CHARSET UTF-8` 同步 literal，按 UTF-8 字节数计长，等待 continuation 后才发查询内容；服务器 BADCHARSET 不发查询内容、不降级、不重试。ASCII 搜索保留原格式；显式检查 tagged NO/BAD，避免上游 SDK 将拒绝误报为空结果。RED f966d67 / GREEN 872dd90，详见 [协议记录](protocol.md)。

### 实际协议与权限

通过持久本机 TLS mailbox 验证文件夹、分页/搜索、PEEK 读取、回复元数据、草稿 APPEND；通过 SMTP 受控接收/拒绝/DATA 后断连验证状态及单次尝试。未批准或越界时不获取凭据/联网。只读缩小文件夹范围后不能查询 Sent；缺失凭据不会认领草稿同步版本。

Sent Append 使用 SQLite 中保存的原 SMTP MIME 字节，重读服务器副本逐字节相等；同提交重复同步被拒绝。ServerManaged 不执行客户端 APPEND。按精确 Message-ID 查询仅返回观察证据，不能确认最终送达或把未找到变成发送失败。APPEND Pending/Uncertain 不盲目重试；明确 Failed 同版本仍无重认领入口，旧服务器草稿不自动清理。

人工批准展示收件人、正文、线程头、附件元数据与完整摘要；线程/附件变化使原批准失效。发送进程锁覆盖认领到记录结果，恢复先取得锁以确认无活跃发送；跨进程 flock 实测排斥并在 drop 后释放。系统 OAuth 刷新/CLI 凭据修改采用 UID/账户稳定锁路径，不受数据/config 覆盖影响；真正钥匙环往返未测试。

### 隔离与 MCP

旧 HTTP/DNS canary 测试保留正向控制和 worker 无新增流量；新 inspect/attachment 原文都走同一 namespace launcher，直接内部入口失败。CLI 显式附件文件导入/导出使用 descriptor walk、NOFOLLOW、regular file/限额检查、不覆盖及 0600；FIFO 和 symlink 祖先拒绝，不执行或打开附件。

官方 SDK 的真实 stdio initialize/list/call、解码前帧上限及受控 TLS Host 的 MCP 调用均通过。十四工具的 schema/未知字段/不透明 ID/保守输出/不可信标记保持一致，无 approve、账户配置或文件路径工具。8 作业共享配额验证了 clone、9th 拒绝、取消仍占用、失败与 panic 释放。

### 终端与测试观察

`python3 tests/e2e/verify_terminal.py target/debug/letterkeeper` 通过：真实 PTY 的中文/宽字符、80×24/24×12、j/Enter/?/q、resize、精确终端属性恢复及完整摘要批准。新增 `terminal_network` 使用实际 PTY 和 TLS IMAP、注入合成凭据，验证按需列表/正文、resize/退出/恢复，且无 UID STORE。

新增 PTY 测试第一次把光标控制和跳过空白单元造成的字节差异误认为缺字；诊断确认正文已经显示，修正观察方式后通过，未宣称为应用缺陷。Ratatui 布局/宽字符和慢 Host 取消/单作业仍由独立测试覆盖。进程锁测试最初临时目录没有设 0700，被安全检查正确拒绝；补全夹具权限后通过。

公开客户端最终显示、真实 Gmail、真实 Secret Service、非 ASCII mailbox、服务商 Drafts/Sent 语义与最终性能目标仍未验收，见 [STATUS](../STATUS.md)。性能样本见 [performance](performance.md)。
