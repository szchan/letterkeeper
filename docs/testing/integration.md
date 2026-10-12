# 集成测试记录

2026-10-12，合成输入，无真实账户数据。

## RED：worker 边界

`cargo test --offline -p letterkeeper --test sandbox`：4 项，1 通过 / 3 失败。失败覆盖中文 HTML 隔离解析、内部 worker 不能直接作为无隔离入口、超大原文在启动 worker 前拒绝。失败原因分别为未实现、空 main 错误地成功和缺少大小判断；不是构建错误。

后续 GREEN、网络观测、受控 SMTP 与实际终端验证待补充。
