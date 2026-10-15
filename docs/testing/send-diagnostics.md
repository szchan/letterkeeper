# 安全发送诊断集成验证

基线：`5a813b2da1179b6115553df17969a943a89007cb`。代码集成 SHA：`8058877a61f59ab7e21173941f28aa330ba246d6`，[完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38732704692) success。本记录的最终文档 revision 仍由 PR #17 的最新门禁覆盖；Linux 验证产物由独立托管 workflow 生成，不表示已发布发行版。决定见 [ADR 0006](../decisions/0006-send-diagnostics.md)，过程见[开发记录](../development/2026-10-15-send-diagnostics.md)。

## 模块证据

| 单元 | 证据与范围 |
| --- | --- |
| 核心 | [记录](send-diagnostics-core.md)：精确 RED/GREEN、schema 0/1/2 到 3、重开、固定值与码界限、权限、不确定/重复拒绝及事务回滚 |
| 协议 | [记录](send-diagnostics-protocol.md)：认证与纯本地 TLS 预检 RED/GREEN、各阶段拒绝、证书保护、超时、不确定及运行时桥接 |
| 应用与 TUI | [记录](send-diagnostics-app.md)：两个独立 RED、精确 GREEN、真实 PTY F6/F8/F9 到账户 TLS SMTP 三态、持久化查询故障与 40/80 列显示 |

上述分支证据不替代新集成 revision 的完整 CI。所有 Rust 构建/测试/Clippy、PTY、HTTP/DNS 隐私与 Codex 验证只在 GitHub Actions 执行，本机仅 Git 与静态审查。测试、公开日志及产物只包含合成数据。

## 行为与隐私

失败与不确定的新增诊断仅包含固定 stage/kind 和可选 100–599 数值码；不保存服务器原文、地址、正文、令牌或路径。Accepted 无失败诊断，不代表外部投递已确认。旧提交诊断为空，不推断其失败原因。

诊断与终态在同一事务落盘，存储失败保留 Uncertain/state_recorded=false。诊断查询故障不能覆盖已读取的发送状态或触发重发，账户/提交读取拒绝仍拒绝。提交阶段包括内容传输与最终响应，不能凭 Failed 或 wire 存在反推原先执行到了哪条命令。

## 升级与保留边界

schema 3 自动迁移已有状态，不改变批准内容绑定和发送字节。升级前须退出旧进程并保留私有备份；旧二进制拒绝打开 schema 3。旧备份用于迁移失败时恢复，完成新操作后须保留新库，不能用旧备份判断能否重发。后续本机运行使用经过 Actions 构建和校验的新产物，不在本机重新编译。

受控测试不替代真实服务商与系统凭据库验收。Gmail 固定端点限制不能为测试放宽；完整账户 TLS 路径与协议层 XOAUTH2 的受控覆盖范围分别记录。此改进支持后续诊断，不追溯修复没有保存阶段的历史提交。

协议文档 HEAD 的完整 CI [38730660668 首次尝试](https://github.com/szchan/letterkeeper/actions/runs/38730660668/attempts/1) 在既有离线工作区 resize 后等待“未保存”失败；相同 SHA 的[第二次尝试](https://github.com/szchan/letterkeeper/actions/runs/38730660668/attempts/2) 全部通过。保留原失败，未跳过或放宽验证；最终集成仍重新运行完整门禁。

## 完整集成结果

上述源代码 SHA 的 workspace、codex-client、ci-gate 全部 success；fmt、Clippy `-D warnings`、完整工作区、真实终端中文/宽字符与四种尺寸、编辑/完整审阅/批准/发送、可达 HTTP/DNS 正向控制及正文无新增外联、实际 Codex app-server 路径全部通过。新增 29 项登记 Rust 测试，总数 272；helper 登记不视为额外独立产品验收。未改 Cargo.lock、依赖或许可证。模块分支 PR #14/#15/#16 的证据保留，最终集成以 PR #17 最新门禁为准。

本机只做静态清单和文档相对链接检查，未运行 Cargo 构建/check/test/Clippy；受控 SMTP 使用合成数据。构建产物的精确 SHA、manifest 和校验和在托管产物中核对，真实邮箱复验不会自动发送。
