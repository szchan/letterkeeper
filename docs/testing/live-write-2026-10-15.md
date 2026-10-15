# 真实令牌续期与邮件写入验收

日期：2026-10-15。用户明确要求继续验收令牌续期、SMTP 和写入功能，并确认向当前已登录账户本人发送 1 封合成测试邮件、创建/同步 1 个测试草稿，保留测试内容，不执行删除。真实地址、客户端信息、令牌、Message-ID、UID、完整邮件和个人配置不进入公开记录；本机私有驱动与输出仅存放在被忽略的 `.local/`。

沿用[只读 IMAP 重验](live-imap-2026-10-15.md)的 `5fde670f53d724873d1ad298fa1f2b97bb9d748c` GitHub 验证构建，二进制 SHA-256 为 `e19a2c1b88223093b34f493c152f527e8106214a33d01b14f33a3bdf1ce6e841`。运行环境仍为 Linux x86_64 / 已解锁 GNOME Keyring；没有本机 Cargo 构建或测试。

## 已通过：自然到期后的续期

独立读取本次专用账户的钥匙环条目时，access token 已自然过期，refresh token 存在。没有修改 token、到期时间或服务端授权，没有调用浏览器登录或重新授予权限。

同时启动两个产品 CLI 的只读 `mail folders`，由既有生产凭据锁、DH Secret Service 和 OAuth 后端处理认证。两次真实 TLS/XOAUTH2 IMAP 认证均成功。独立进程读回确认：access token 已改变、到期时间推进且在未来、refresh token 仍存在。随后第三次产品认证成功，独立读回的 token bundle 没有再次改变，验证有效缓存复用。

本轮没有捕获 OAuth HTTP 载荷或统计服务端刷新请求，因此不宣称实际恰好发出一次 refresh POST。并发成功和锁的独立受控验证分别记录，不把两者混为服务端请求计数证据。

## 已通过：服务器草稿写入

独立只读 LIST 通过服务商 special-use 属性定位 Drafts 和 Sent，不假定本地化文件夹名称。建立同一账户的独立写入配置和状态库：允许起草/发送，Sent 使用 ServerManaged；原先只读配置和只读 TUI 入口保持不变。

在专用工作区创建 1 个带验收标识的合成草稿，仅当前账户本人为收件人，Cc/Bcc/附件/线程引用为空。产品只执行一次 `sync-draft`，记录 Confirmed；独立只读 SEARCH/EXAMINE/BODY.PEEK 回验：仅有一个匹配 Message-ID，解码主题/正文与准备稿一致，存在 `\Draft` 标记。用户也确认在 Gmail 网页 Drafts 看到了该草稿。

没有为测试幂等性再次 APPEND，没有修改或删除任何既有草稿。没有调用 Sent APPEND。

## 发送关卡：人工批准与 SMTP 仍待完成

用户已打开专用写入 TUI；在 F8 批准输入后遇到 `invalid_input`。只读检查专用状态库确认：没有 SMTP submission、没有有效 approval，草稿 revision/digest 未变，因此尚未尝试发送，也不是已提交后 Uncertain 的重发场景。

当前 F8 对话框没有重复展示待输入值，F6 审阅页仅用“摘要”标注 SHA-256，提示存在可发现性问题。已指导关闭输入框、按 Home 回到完整审阅顶部，用户确认看到对应字符串；人工审阅和批准仍由用户实际操作，不由代理代填摘要或修改 approvals 表绕过。

待用户完成批准和一次发送后，再只读核对持久化 SMTP 状态、原始 wire、Sent 中的精确 Message-ID、副本数和本人 Inbox 回验。未取得证据前不宣称 SMTP 或完整写入闭环通过；出现 Failed/Uncertain 时不自动重发。

## 保留限制

本轮只操作明确授权的单个合成测试草稿/邮件，不做 STORE、删除、移动、EXPUNGE、远程撤销或真实网络故障注入。续期成功不代表 invalid_grant、Testing 授权有效期、所有 Secret Service provider 或失联/锁定场景全部验收。真实测试数据不进入公开 CI；文档提交与合并继续执行 GitHub 完整门禁。
