# 实现与验收状态

发送诊断增量正在集成验证：固定阶段/种类/SMTP 数值码与终态原子保存，CLI/TUI 查询共用，旧记录不推测原因；保留人工批准和三态发送语义。核心、协议与应用的远程 RED/GREEN 已取得，最终集成门禁尚待完成，不能据此宣布真实故障已定位。见[诊断验收](testing/send-diagnostics.md)与[ADR 0006](decisions/0006-send-diagnostics.md)。

2026-10-15 增量：已获明确授权并完成本机已解锁 GNOME Keyring 的生产凭据往返、跨进程锁和本次合成条目清理，以及真实 Google OAuth 浏览器同意、代码交换和 token 本机保存/独立读回。随后用户明确授权账户全部邮件的只读 IMAP 验收：严格 TLS/XOAUTH2、文件夹、INBOX/All Mail 分页、ASCII/中文搜索、正文抽样及前后 Flags/UIDVALIDITY 比较通过。旧构建发现的大邮箱列表问题已经远程 TDD 修复，并使用新构建真实重验。该只读阶段没有发送或修改邮件状态。随后真实令牌自然到期后的续期、并发认证及服务器草稿写入通过；首次 CLI SMTP 尝试持久化 Failed，独立观察到握手超时；用户修复网络并通过新的 TUI 人工批准后，CLI 单次发送持久化 Accepted，Sent/本人 Inbox 核对通过。用户确认 Gmail 网页上发送对应的草稿已消失、另一份仍在，不将其记为草稿刷新故障。Hyprland 听写快捷键截获 F9，真实客户端 F9 仍未验收。详细证据见[续期/写入记录](testing/live-write-2026-10-15.md)。范围、构建和环境见[钥匙环/OAuth](testing/live-keyring-2026-10-15.md)及[只读 IMAP](testing/live-imap-2026-10-15.md)。

账户接入增量 `doctor` 和 `account add` 已通过 [代码 SHA `84b5dee` 的完整集成 CI](https://github.com/szchan/letterkeeper/actions/runs/38583555164)，[PR #6](https://github.com/szchan/letterkeeper/pull/6) 保留最终 revision 的门禁。诊断只读取目录和凭据服务元数据，配置默认只读、确认后原子创建；不表示已登录或真实服务验收完成。详见[本批开发记录](development/2026-10-14-account-onboarding.md)。

2026-10-14 受控基线：阶段 C 的代码与**受控 TUI/Agent 人工审批工作流验证**已完成，**没有完成全部 P0**。下表保留受控结果和缺口；10-15 已授权真实增量单独标明，需求编号未变。源码已公开到 [szchan/letterkeeper](https://github.com/szchan/letterkeeper)，重型构建/测试迁移到 GitHub Actions；[集成分支 `53d0d72` 完整门禁](https://github.com/szchan/letterkeeper/actions/runs/38539664364)及[手动 release/性能样本](https://github.com/szchan/letterkeeper/actions/runs/38537562629)通过。产物不是发行版，真实服务关卡仍保留。

| 需求 | 已实现与验证 | 仍需完成 |
| --- | --- | --- |
| MAIL-01、AUTH-02 | 隐式 TLS IMAP/SMTP、证书与主机名校验、密码/XOAUTH2 接口；错误证书在发送凭据前拒绝；真实 Gmail TLS/XOAUTH2 IMAP 和单次 SMTP 自发邮件通过 | STARTTLS、其他服务商兼容性、更多失败诊断 |
| AUTH-01、AUTH-03、AUTH-04 | Gmail 固定端点、PKCE/state/loopback、代码交换/刷新/撤销、Secret Service DH 后端、UID/账户共享刷新锁；10-15 本机真实凭据往返、协调/合成清理及个人 Desktop client 的 Google OAuth 登录/token 保存、自然到期续期、并发认证和 IMAP 认证通过 | 真实撤销、其他凭据服务及真实锁定/失联拒绝；公开发行 OAuth 流程 |
| MAIL-02、MAIL-03 | 文件夹、有界分页/服务器搜索（UTF-8 同步 literal、明确拒绝）、UIDVALIDITY/PEEK、中文 MIME、回复/全部回复/显式转发、线程头与授权绑定；真实 Gmail 列表/搜索与安全正文抽样通过 | 更多字符集和畸形语料、非 ASCII mailbox 编码/服务商扩展、完整线程视图与常用标记；目前仅 CLI 显式 Seen |
| MAIL-04、PRIV-05 | 附件元数据、隔离 MIME 解码、CLI/TUI 显式保存/导入及移除草稿附件、完整字节摘要审阅、512 KiB 单附件界限、路径/符号链接/覆盖拒绝；附件字节绑定批准 | CID 安全显示、更大附件/整封消息的限额设计；MCP 不提供文件路径工具 |
| MAIL-05 | 显式服务器草稿版本快照；ServerManaged 不 APPEND；Append 单次认领并使用原 SMTP 字节；受控服务验证重启与重复拒绝；真实 Gmail 单次 Drafts APPEND、正文/标记回验及 ServerManaged Sent 单份/无额外 APPEND 通过 | 更多 Gmail 标签语义、远程草稿更新/清理及冲突；Append 版本快照不自动删除旧草稿（本次网页观察未据此认定服务商故障） |
| MAIL-06、PRIV-06 | 默认 EXAMINE/BODY.PEEK、无自动 MDN；TUI/MCP 默认不 Seen，明确 CLI 修改另行执行；真实 Gmail 已读/未读抽样前后 Flags 不变 | TUI 自动标记策略配置、更多服务商 Seen 行为矩阵 |
| MAIL-07、MCP-06、MCP-09 | CLI/TUI 完整审阅与独立发送、版本/内容绑定、单次批准、原子认领、不可变 wire、Accepted/Failed/Uncertain、跨进程发送锁与中断恢复；Sent 查询仅为证据；真实 TUI 人工批准、CLI Failed/Accepted 持久化及 Sent/本人投递观察通过 | 客户端 F9 触发验收、实际服务商恢复体验；同步 Pending/Uncertain 及明确 Failed 暂无同版本恢复/重认领入口；无自动重发 |
| MAIL-08、MAIL-09 | 配置可保存多个账户，CLI 启动选一个，核心账户作用域隔离 | 统一多账户界面、邮件缓存/本地搜索、删除/移动/归档、Gmail 标签 |
| DATA-01、DATA-02 | XDG 配置/数据、0600/0700、descriptor 配置原子替换与 create-only；doctor 固定安全 JSON、目录/实际 worker/凭据元数据诊断，不初始化或读取秘密；上游 IMAP 敏感 TRACE 编译禁用 | 缓存清理、完整日志和文件系统安全审计；真实凭据服务兼容性 |
| DATA-03 | 数据库未加密的限制明确记录 | 静态加密设计与实测 |
| PRIV-01 至 PRIV-04、PRIV-08 | 无资源获取解析器、独立 worker、原文/输出/parts/depth/CPU/RAM/time 界限，HTTP/DNS 正向控制与无新增流量；新 inspect/attachment 入口故障关闭 | 模糊测试、安全审计、其他发行版沙箱与更多攻击语料 |
| PRIV-07、PRIV-09 | 无自动链接/预览/导航/附件打开，TUI/MCP 去活化显示 | 显式打开资源的完整交互；不提供手动联网正文渲染 |
| MCP-01 至 MCP-03 | 官方 SDK stdio、14 工具、分页/截断、解码前帧限制、每会话 8 Host 任务；真实 stdio/TLS Host、实际 Codex app-server 数据路径测试 | 其他公开客户端/图形前端兼容性；完整正文/附件的显式 MCP 能力评估 |
| MCP-04、MCP-05、MCP-07 | 固定启动账户/端点、文件夹与操作权限、只读进一步收窄；无批准/配置/秘密/文件工具；拒绝 confirm/role/path 参数 | 受限自动发送策略、额外 OS 身份隔离；同 UID 任意 shell 不在此授权边界 |
| MCP-08 | 实际Codex app-server接收去活化文本，可达DNS/HTTP的MCP草稿/编辑/审阅正向控制，安全 Host 输出无原始 HTML/活动 URL；正文指令仅作为数据 | 不承诺第三方客户端不重新解释/联网，下游逐一安全展示验证仍待完成 |
| MCP-10 | 未提供公网/HTTP MCP | 后续认证与访问控制设计 |
| UX-01、UX-02 | 中文/宽字符、各一个网络/本地后台 worker、有界页面、搜索/刷新/文件夹、取消过期结果、加载/错误、内置编辑/冲突/附件/完整审批/发送查询、真实 PTY + TLS 往返/窗口/恢复 | F8 摘要提示可发现性、真实客户端 F9 触发验收（环境中 Hyprland consuming 绑定已截获该键）；持续视觉评审、更细错误与即时连接取消；离线 EML 入口仍预解析 |
| UX-03 | CLI 结构化结果、基本快捷键与 account add 交互配置；默认只读、精确确认、取消无写入、未认证状态及后续命令 | 主题/布局/辅助使用 |
| PERF-01 至 PERF-03 | 有界请求/页面/worker、无浏览器、阶段 A/B 同负载和阶段 C release 1k/10k TLS/活跃MCP/大正文编辑测量；该样本输入p95≤100ms、空闲单核CPU≤1% | 真实大邮箱/冷缓存/附件负载、更长空闲采样及最终性能预算 |

## 重要限制

- 首轮 IMAP 文件夹测试使用 ASCII 的 INBOX/Drafts/Sent；配置采用协议返回的名称，不声称已处理所有非 ASCII mailbox 编码。
- worker 依赖 Linux bubblewrap、用户/网络 namespace 和实际系统库目录。Arch 与专用配置的 GitHub Ubuntu runner 已验证，其他发行版及默认 Ubuntu 桌面策略仍待验收；库 API 本身不创建沙箱。应用只通过受限 worker 处理原始正文/头部/附件。
- SQLite 的检查后重新打开仍不是针对同 UID 任意代码的完整 TOCTOU 防护；配置与附件路径使用 descriptor walk，但不能据此声称数据库也完全抵抗此类攻击。
- CLI 原始安全 JSON 中的链接目标必须作为数据使用。MCP与TUI阅读页的保守文本不能代替完整发送审阅；TUI人工审阅页使用完整、可逆转义的文本。公开图形下游仍需独立验证。
- 已解锁 GNOME Keyring 的本机凭据往返通过，不泛化为全部 provider/故障场景通过。系统凭据锁要求存在且私有的 /run/user/UID，缺失时故障关闭。
- 已授权的真实 Google OAuth、本机凭据、只读 IMAP、续期、单个测试草稿写入和本人 SMTP 投递已执行；首次 Failed 保留，网络修复后经新的人工批准，CLI 单次发送 Accepted 并核对 Sent/Inbox。未执行客户端删除操作；用户确认发送草稿已消失，另一份仍在。没有移动、显式修改既有邮件标记或撤销真实授权；SMTP 不自动重试。详情见[续期/写入记录](testing/live-write-2026-10-15.md)。只读验收是抽样而非完整下载；搜索每批 4096 个消息序号、每次最多 128 批，超预算明确报错。公开源码与开发分支推送已有授权，未发布发行版。

当前受控测试为 243 项已登记 Rust 测试：原账户接入基线 231 项、1 项 Gmail 配置标签兼容回归和 11 项有界分页/完成状态协议回归；精确 RED/GREEN 及完整 CI 见[配置兼容](testing/gmail-config-compat.md)和[分页回归](testing/imap-bounded-paging.md)。fmt/Clippy、实际 PTY、独立客户端与隐私门禁保留，新集成 revision 必须重新通过；helper 登记不视为额外独立产品验收。另见[账户接入记录](development/2026-10-14-account-onboarding.md)、[阶段 C 集成](testing/phase-c-integration.md)和[真实服务关卡](testing/live-acceptance.md)。阶段 B/C 历史结果仍保留追溯。
