# 阶段 B 集成验证

2026-10-13，仅合成邮件及本机受控服务。

## 隔离正文、附件与回复头 RED

`cargo test --offline -p letterkeeper --test worker_composition` 编译成功，4 项全部行为失败：inspect/attachment 接口 stub 尚未实现、内部新入口未路由到隔离检查、SMTP 未输出批准的线程头、附件导入/导出未实现。后续 GREEN 待记录。

## 共享远程 Host RED

`cargo test --offline -p letterkeeper --test remote` 编译成功，4 项全部失败，原因均为远程 Host 尚未装配。测试覆盖跨 TLS 会话读取/回复/草稿同步、权限先于凭据获取、MCP 去活化及拒绝自授权、DATA 后断连与重启状态。

## 账户 CLI RED

`cargo test --offline -p letterkeeper --test account_cli`：初始 1 通过/1 失败，失败为尚无 config-dir/账户命令。发现命令帮助断言只检查子串会把描述中的 account/mail 当成命令，改为检查独立命令行；再次 RED 为 2 项全部失败。测试不调用真实系统凭据库，密码/批准管道必须拒绝。

## 审查与跨模块回归 RED

Sent 文件夹范围测试去掉检查后实际失败（凭据获取 1 次而非 0 次），随后恢复检查。凭据缺失时草稿同步错误地返回 Failed 并永久认领版本，新增 `missing_credentials_do_not_consume_a_draft_sync_revision` 实际断言失败；修复将在 APPEND 认领前完成凭据/CA 本地预检，不联网。

MCP `spawn_blocking` 与协议同步桥接不兼容由实际 TLS 集成暴露，在 protocol 模块补 RED/GREEN。部分初始测试把自身 worker fixture 的身份套用到共享协议 fixture，核对后改为后者真实的 Reply-To 和 Message-ID；没有改动应用输出去迎合错误身份。
