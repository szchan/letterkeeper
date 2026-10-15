# 本机 Secret Service 真实验收

日期：2026-10-15。用户已明确授权本机专用合成条目的保存、读取、协调与清理，以及专用 Gmail 账户的客户端凭据导入和浏览器 OAuth 登录。没有授权读取邮件、发送、修改邮件状态或撤销真实授权。本记录不含真实邮箱、客户端标识、文件名、凭据、授权 URL 或截图；私有配置和验收驱动只保留在被忽略的 `.local/`。

## 环境与构建

- Linux x86_64，实际 Secret Service 进程为 `gnome-keyring-daemon`。
- 系统包：gnome-keyring `1:50.0-1`、libsecret `0.21.7-1`、bubblewrap `0.12.0-1`。
- 使用已下载的 GitHub Actions release 模式验证构建，源码为 `64dd635e5349f58cec93a164bfd6c4e781ca4269`，没有本机 Cargo 编译。
- 二进制 SHA-256：`551d4c98606985d95a151f3c3d1152a4b9f71884d2d4fb58f08964b403d1eb65`；包内 8 项 SHA256SUMS 校验均通过。
- 本机会话的 doctor 报告 runtime 和合成 worker 就绪，默认 collection 已解锁。受限工具执行环境可能阻断 namespace/会话 D-Bus；该限制没有被当作本机服务故障，也没有修改用户系统配置。

## 已执行结果

测试账户以 `letterkeeper-acceptance-` 加随机后缀命名，初始精确属性查询确认不存在条目，只操作本次命名空间。程序属性仍为 `application=org.letterkeeper`、该合成 account 与指定 kind。

| 验证 | 实际结果 |
| --- | --- |
| 产品 CLI 隐藏终端输入保存合成密码 | 成功；输入时核对 ECHO 已关闭，值不在 argv 或输出 |
| 独立进程通过 libsecret 精确读取该条目，与合成值比较 | 相等；仅记录布尔结果 |
| 持有该账户固定运行目录的 auth.lock，再启动第二个产品 CLI | 持锁期间没有进入凭据提示；释放后继续，跨进程协调生效 |
| 第二次保存不同合成值并再次跨进程读取 | 相等 |
| 产品 CLI logout 清理本次合成条目，再精确查询 | 条目不存在；未删除其他个人条目 |
| 专用真实账户的 client secret 经产品 CLI 隐藏输入保存并独立读取比较 | 相等；secret 只进入系统钥匙环 |
| 产品 Gmail 登录入口读取该 client secret 并生成浏览器授权请求 | 成功；生产后端使用固定 DH 会话，未改用明文后端 |
| 真实 Google 浏览器同意、本机 loopback 回调、代码交换 | 成功；访问/刷新令牌均已保存至钥匙环 |
| 新进程独立读回专用 token bundle | access 与 refresh 均非空，access 到期时间在未来；只记录布尔结果 |
| 专用账户配置范围 | 只读，允许 INBOX；本轮没有执行任何邮件读取 |

Google 首次授权页因测试用户准入返回 `403 access_denied`。失败的本地监听已关闭；核对已保存客户端与下载 JSON 一致，并按 JSON 指向的同一项目检查 Audience/Test users。失败尝试没有留下 OAuth token，不将前端准入错误记为 token 交换或邮件认证成功。

用户确认目标账户已在正确项目的测试名单中保存且刷新后仍可见后，重新生成登录请求并完成浏览器同意。产品 CLI 返回授权成功，独立进程读回新 token bundle 的存在与到期检查均通过。此结论是 OAuth 授权及本机令牌保存通过，不代表已经执行 IMAP AUTHENTICATE 或取得邮件读取验收结果。

该实现请求 `https://mail.google.com/` scope；应用中的只读范围不会缩小 Google 令牌本身的权限。本次全程保持配置只读，实际没有调用任何邮件读写入口。

## 剩余关卡

本记录证明这台机器上已解锁 GNOME Keyring 的生产凭据往返和进程协调，不代表所有 Secret Service provider 均兼容。未锁定或停止用户的真实 collection，故锁定/不可用拒绝仍由受控测试覆盖，真实故障环境需要另行隔离验收。

Google OAuth 登录与 token 保存已通过。真实续期、IMAP 身份认证、指定邮件的无状态读取、草稿与单封发送仍分别待验；本次没有人为修改真实 token 到期时间或主动撤销授权。完整关卡见[真实服务清单](live-acceptance.md)。
