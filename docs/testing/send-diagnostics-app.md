# 账户发送诊断：应用、CLI 与 TUI

本批覆盖 MAIL-01、MAIL-07、AUTH-03、DATA-02、MCP-09、UX-01。只使用临时目录、虚构身份、合成秘密及受控 loopback TLS SMTP；没有访问真实邮箱、个人配置或历史失败数据，也不据此定位真实 SMTP 失败原因。

RED：`5c87e656fb715b1c6e8d4d2657403608b05654a9`，[Actions 38709952540](https://github.com/szchan/letterkeeper/actions/runs/38709952540)。精确运行 `failed_send_has_safe_persistent_diagnostic`，crate `letterkeeper`；checkout SHA 一致，`matched_tests=1`、`raw_exit_code=101`、`state=test_finished`。真实 TLS SMTP RCPT 返回 550 后状态为 Failed，但旧版 JSON diagnostic 为 Null，未满足固定 `recipient/rejected/550` 断言。失败不是编译、环境或测试未选中。

实现使用核心同事务 finish：状态和诊断一起提交，返回值直接携带实际报告；发送后的诊断查询故障不覆盖已知三态。准备、TLS 文件/证书配置、凭据准备按序执行；TLS 配置失败不读取凭据。协议阶段/种类穷尽映射为核心固定枚举，不保存服务器原始文本。状态落盘故障保留 Uncertain、`state_recorded=false`、`diagnostic=null`。

CLI `send` / `status` 的 `diagnostic` 为 `{stage,kind,smtp_code}` 或 `null`。null 表示没有诊断信息，不能根据旧记录追溯推断失败阶段。Accepted 不附失败诊断。TUI 共用持久化查询，显示固定阶段/种类/可选数值码；没有诊断或查询失败显示 unknown，不重发。既有 SubmissionInfo 及模型批准边界保留；HumanWorkspace 新查询默认 None，兼容离线与 mock Host。

新增受控验证：

- 真实 RemoteHost TLS RCPT 拒绝，重开 Host 仍查询同一诊断；无批准拒绝且凭据读取为零。
- CA 文件缺失、无效 PEM 均不读取凭据；凭据不可用拒绝发送且返回固定 Credentials/Unavailable。
- 合成 SMTP 接受、最终 DATA 拒绝及提交后断连；独立 CLI status 进程查询同一三态/诊断，发送只发生一次。
- 服务器错误故意含合成地址和 token，send/status JSON 与 TUI 失败输出均不得出现原文。
- 实际 PTY F6 完整审阅、F8 全摘要批准、F9 单独发送经 RemoteHost 到真实受控 TLS SMTP；覆盖 Accepted、Failed、Uncertain，以及中文和宽字符；审批本身不联网发送。
- 网络 worker 诊断查询错误仍保留 Accepted，仅显示 unknown；存储故障不宣称诊断持久化。旧 finish_submission 记录为 null。

该 TLS 账户路径使用 PLAIN 合成凭据，不放宽 Gmail 固定生产端点验证。XOAUTH2 由协议层独立受控测试覆盖，不能声称本地模拟完成 Gmail 账户全链。

GREEN 与完整 CI：等待核心/协议依赖提交后运行，当前未验收。所有 Cargo 构建、测试与 Clippy 均留在 GitHub Actions；本机仅编辑、rustfmt、Git 和静态审查。既有四尺寸 PTY、正文隔离 HTTP/DNS 与 Codex 路径门禁必须保留；任务分支通过不替代集成 revision 的完整门禁。
