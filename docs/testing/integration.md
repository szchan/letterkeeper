# 集成测试记录

2026-10-12，合成输入，无真实账户数据。

## RED：worker 边界

`cargo test --offline -p letterkeeper --test sandbox`：4 项，1 通过 / 3 失败。失败覆盖中文 HTML 隔离解析、内部 worker 不能直接作为无隔离入口、超大原文在启动 worker 前拒绝。失败原因分别为未实现、空 main 错误地成功和缺少大小判断；不是构建错误。

后续 GREEN、网络观测、受控 SMTP 与实际终端验证待补充。

## RED：SMTP 边界

`cargo test --offline -p letterkeeper --test smtp`（获准本机测试服务）：5 项，2 通过 / 3 失败。失败为尚未实现 MIME/信封构造、SMTP 接受状态和 DATA 后断连不确定状态；测试服务只绑定 loopback，身份均为 example.test。

## RED：应用入口

`cargo test --offline -p letterkeeper --test cli`：2 项全失败，分别为尚未提供命令帮助和草稿持久化/拒绝管道批准流程。应用入口实现随后进行。
