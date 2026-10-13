# 阶段 B 集成验证

2026-10-13，仅合成邮件及本机受控服务。

## 隔离正文、附件与回复头 RED

`cargo test --offline -p letterkeeper --test worker_composition` 编译成功，4 项全部行为失败：inspect/attachment 接口 stub 尚未实现、内部新入口未路由到隔离检查、SMTP 未输出批准的线程头、附件导入/导出未实现。后续 GREEN 待记录。

## 共享远程 Host RED

`cargo test --offline -p letterkeeper --test remote` 编译成功，4 项全部失败，原因均为远程 Host 尚未装配。测试覆盖跨 TLS 会话读取/回复/草稿同步、权限先于凭据获取、MCP 去活化及拒绝自授权、DATA 后断连与重启状态。
