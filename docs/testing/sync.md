# 回复头与远程同步 TDD

2026-10-13，使用合成身份及临时 SQLite。

RED：`cargo test --offline -p letterkeeper-core --test sync` 编译成功，8 项中 2 通过、6 失败。未实现的同步认领返回 Storage；回复头注入错误地被接受。保留空线程字段的序列化兼容与授权随内容变化失效测试已通过。实现与 GREEN 尚待记录。
