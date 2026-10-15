# 真实令牌续期与邮件写入验收

日期：2026-10-15。用户明确要求继续验收令牌续期、SMTP 和写入功能，并确认向当前已登录账户本人发送 1 封合成测试邮件、创建/同步 1 个测试草稿，保留测试内容，不执行删除。真实地址、客户端信息、令牌、Message-ID、UID、完整邮件和个人配置不进入公开记录；本机私有驱动与输出仅存放在被忽略的 `.local/`。

沿用[只读 IMAP 重验](live-imap-2026-10-15.md)的 `5fde670f53d724873d1ad298fa1f2b97bb9d748c` GitHub 验证构建，二进制 SHA-256 为 `e19a2c1b88223093b34f493c152f527e8106214a33d01b14f33a3bdf1ce6e841`。运行环境仍为 Linux x86_64 / 已解锁 GNOME Keyring；没有本机 Cargo 构建或测试。

## 已通过：自然到期后的续期

独立读取本次专用账户的钥匙环条目时，access token 已自然过期，refresh token 存在。没有修改 token、到期时间或服务端授权，没有调用浏览器登录或重新授予权限。

同时启动两个产品 CLI 的只读 `mail folders`，由既有生产凭据锁、DH Secret Service 和 OAuth 后端处理认证。两次真实 TLS/XOAUTH2 IMAP 认证均成功。独立进程读回确认：access token 已改变、到期时间推进且在未来、refresh token 仍存在。随后第三次产品认证成功，独立读回的 token bundle 没有再次改变，验证有效缓存复用。

本轮没有捕获 OAuth HTTP 载荷或统计服务端刷新请求，因此不宣称实际恰好发出一次 refresh POST。并发成功和锁的独立受控验证分别记录，不把两者混为服务端请求计数证据。

## 已通过：服务器草稿写入

独立只读 LIST 通过服务商 special-use 属性定位 Drafts 和 Sent，不假定本地化文件夹名称。建立同一账户的独立写入配置和状态库：允许起草/发送，Sent 使用 ServerManaged；原先只读配置和只读 TUI 入口保持不变。

驱动和专用工作区记录创建 1 个带验收标识的合成草稿，仅当前账户本人为收件人，Cc/Bcc/附件/线程引用为空；产品只执行一次 `sync-draft`，记录一次 Confirmed Draft APPEND。独立只读 SEARCH/EXAMINE/BODY.PEEK 当时查到一个匹配 Message-ID，内容与准备稿一致且带 `\Draft` 标记。另一次只读复核按合成主题查到当前一个匹配，其 Message-ID 与驱动记录的一次 APPEND 相同。用户后来补充其 Gmail 网页原有两份测试草稿：成功发送对应的那份现已从 Drafts 消失，另一份仍在。此网页观察由用户提供；驱动记录的一次 APPEND 和查询不能分别对应网页上的两份草稿，因此不将该观察记为 Gmail 草稿刷新故障。

没有为测试幂等性再次 APPEND，没有修改或删除任何既有草稿。没有调用 Sent APPEND。Append 的版本快照设计不会自动清理旧草稿；这是设计行为，不是本次 Gmail 网页观察的故障结论。

## 人工批准与首次 SMTP 失败

用户已打开专用写入 TUI；在 F8 批准输入后遇到 `invalid_input`。只读检查专用状态库确认：没有 SMTP submission、没有有效 approval，草稿 revision/digest 未变，因此尚未尝试发送，也不是已提交后 Uncertain 的重发场景。

当前 F8 对话框没有重复展示待输入值，F6 审阅页仅用“摘要”标注 SHA-256，提示存在可发现性问题。已指导关闭输入框、按 Home 回到完整审阅顶部，用户确认看到对应字符串；人工审阅和批准仍由用户实际操作，不由代理代填摘要或修改 approvals 表绕过。

随后用户通过 TUI 输入正确摘要，独立只读检查确认存在仍有效、绑定原始 revision/digest 的批准。用户按 F9 后没有观察到提交记录，不能把该次 TUI 发送触发记为通过。后续用户在同一 Ghostty 中运行独立的有界按键脚本，按 F9 得到 `no_input_received`；只读 `hyprctl binds -j` 显示默认 submap 有两条 `modmask=0` 的 consuming F9 绑定，按下描述为 “Start dictation (push-to-talk)”，松开描述为 “Stop dictation (push-to-talk)”。该后续诊断定位了环境原因：Hyprland 截获 F9。未修改系统绑定；真实客户端 F9 触发未验收。代理未代填摘要，未创建或修改批准条目。

经核对仍无既有 submission、人工批准有效、内容与仅本人收件人一致后，通过正常产品 CLI 原子消费这次人工批准，执行一次发送尝试。产品返回并持久化 `Failed`；没有自动重试。独立只读状态核对为一个 Failed submission、批准已消费、原始 SMTP wire 快照存在。随后独立只读查询本次 submission 的精确 Message-ID：Sent 和 Inbox 均无匹配；当时驱动记录的 APPEND 仍有一个草稿匹配。查询无匹配只是当时观察，不单独作为未投递的证明；这一阶段未记为 SMTP、Sent 副本或投递闭环通过。

### 连接阶段诊断

独立 SMTP 诊断在 TLS/欢迎语之前超时，尚未执行 AUTH、MAIL、RCPT 或 DATA。进一步分层观察：

| 检查 | 结果 |
| --- | --- |
| 465 TCP | connect 返回成功 |
| 465 严格 TLS | 握手超时，未验证证书或取得 SMTP 欢迎语 |
| 587 TCP / SMTP 欢迎语 | connect 返回成功，读取欢迎语超时；没有执行 STARTTLS 或认证 |
| DNS 返回地址的 IPv4 / IPv6 路径 | 两类均在 TLS 阶段超时 |
| 通过既有代理建立 CONNECT 后再验证 TLS | CONNECT 成功，TLS 仍超时 |
| 用户切换 VPN 节点后重测握手 | 465/587 仍未恢复；未执行认证或发送 |

用户澄清 Thunderbird 的成功是前一晚的结果，目前也不能发送。当前观察支持优先检查实际 SMTP 链路/出口策略，但不能确定具体 VPN、代理或防火墙根因。产品的 `Failed` 本身没有携带安全阶段字段，因此不能仅凭它精确判断原始尝试的失败步骤；连接阶段定位来自独立诊断，不冒充产品内部追踪。

465 SSL / 587 STARTTLS 与服务端点见 [Google 官方协议说明](https://developers.google.com/workspace/gmail/imap/imap-smtp)。产品目前仅支持隐式 TLS，没有为了通过验收关闭证书校验、降级明文、改为不支持的 STARTTLS 或改用其他发送接口。

用户当时选择继续调整节点或代理规则并在准备好后通知；SMTP 验收在这一阶段等待该外部条件。网络恢复后先做不含凭据/邮件的握手检查，认证和再次发送分别继续验证；原批准已被失败尝试消费，后续发送仍需新的真实人工批准。Failed 记录与现有测试草稿保留，不删除、不自动重发。

## 网络恢复后的复验

用户随后明确通知 SMTP 网络已修复并恢复验收。首先仅检查握手：465 TCP、严格 TLS/证书校验、SMTP 欢迎语通过，587 欢迎语也已恢复；没有在 587 上进行 STARTTLS 或发信。随后在 465 上独立验证 EHLO、XOAUTH2 声明与实际认证，均通过，未执行 MAIL/RCPT/DATA。

独立只读检查本机专用工作区仍只有原 Failed submission、没有有效批准、草稿 revision/digest 未变。用户重新通过 TUI 审阅和批准原草稿，再按一次 F9，仍反馈“没有反应”。独立状态检查确认新的人工批准有效且与原草稿绑定，但没有新 submission。结合上述按键观察，Hyprland 的 consuming 绑定解释了环境中 F9 被截获；客户端 F9 触发仍未通过验收。没有新建或再次 APPEND 草稿。

在再次核对原 Failed 是唯一历史提交、新批准有效、草稿与仅本人收件人的原内容一致后，通过正常产品 CLI 执行一次明确重新批准的发送，原子消费该批准。没有改写 approvals 表，没有自动重试原失败提交。产品返回 Accepted 且 state_recorded=true；历史 Failed 记录保留，新 Accepted 仅一条，不存在 Uncertain。

## 已通过：SMTP、Sent 与本人收件箱闭环

产品 `status` 独立查询返回持久化 Accepted，`mail sent-evidence` 返回一个匹配。随后独立只读 EXAMINE/UID SEARCH/BODY.PEEK 核对：

| 检查 | 结果 |
| --- | --- |
| 接受提交的 revision/digest 与内容快照 | 与原准备草稿一致；不可变 SMTP wire 存在 |
| 人工批准 | 已单次消费；没有残留批准 |
| 服务商 Sent | 本次 Message-ID 仅一个匹配 |
| 本人 Inbox | 本次 Message-ID 仅一个匹配；解码主题、正文和本人收件人正确 |
| ServerManaged | 没有本地 Sent APPEND 同步记录 |
| 驱动查询的测试草稿 | 只读复核当前一个匹配，与驱动记录的 APPEND 一致 |

Accepted 仍仅表示 SMTP 接受；本人收件箱观察是本次自发邮件的额外实际投递证据，不泛化为所有外部收件人的投递保证。客户端回验为只读；本轮客户端未执行删除、移动或 EXPUNGE。这不表示 Gmail 未移除发送对应的草稿；用户确认该草稿已消失、另一份仍在。

本轮自然续期、真实服务器草稿写入、TUI 人工批准和 CLI 单次发送/结果查询/Sent/本人投递闭环通过。F8 摘要提示的可发现性仍待改善；F9 环境截获原因已定位，但客户端 F9 触发仍未验收，不能把 CLI 成功写成 TUI F9 成功。

## 保留限制

本轮只操作明确授权的单个合成测试草稿/邮件，不做 STORE、删除、移动、EXPUNGE、远程撤销或真实网络故障注入。续期成功不代表 invalid_grant、Testing 授权有效期、所有 Secret Service provider 或失联/锁定场景全部验收。真实测试数据不进入公开 CI；文档提交与合并继续执行 GitHub 完整门禁。
