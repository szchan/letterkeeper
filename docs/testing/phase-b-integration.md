# 阶段 B 集成验证

2026-10-13，仅合成邮件及本机受控服务。

## 隔离正文、附件与回复头 RED

`cargo test --offline -p letterkeeper --test worker_composition` 编译成功，4 项全部行为失败：inspect/attachment 接口 stub 尚未实现、内部新入口未路由到隔离检查、SMTP 未输出批准的线程头、附件导入/导出未实现。后续 GREEN 待记录。

## 共享远程 Host RED

`cargo test --offline -p letterkeeper --test remote` 编译成功，4 项全部失败，原因均为远程 Host 尚未装配。测试覆盖跨 TLS 会话读取/回复/草稿同步、权限先于凭据获取、MCP 去活化及拒绝自授权、DATA 后断连与重启状态。

## 账户 CLI RED

`cargo test --offline -p letterkeeper --test account_cli`：初始 1 通过/1 失败，失败为尚无 config-dir/账户命令。发现命令帮助断言只检查子串会把描述中的 account/mail 当成命令，改为检查独立命令行；再次 RED 待记录。测试不调用真实系统凭据库，密码/批准管道必须拒绝。
