# 正文模块验证记录

2026-10-12：在 `feat/content` worktree 先加入公开 API stub 与 18 项可编译集成测试，运行 `cargo test -p letterkeeper-content --offline`，结果为 **0 passed / 18 failed**（真实 RED）。失败覆盖 MIME/字符集、HTML 主动内容与资源、链接、附件 metadata、控制字符和资源边界。测试全部使用虚构身份及内存合成邮件，不含真实邮件。

实现与后续 GREEN 验证待完成。
