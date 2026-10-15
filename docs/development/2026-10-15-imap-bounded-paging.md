# 大邮箱有界分页修复

需求：MAIL-02、MAIL-06、MCP-03、PERF-01。真实服务的只读验收暴露了大邮箱列表先获取全部 UID、再分页的问题。此记录仅含合成数据，不记录真实邮箱规模或内容。

独立分支 `fix/imap-bounded-paging`，基线 `a2c862268c60e6d9ce64595f1e6cba0d94630640`。先新增 TLS 合成服务回归；实现尚未开始，等待准确 SHA 的远程真实断言 RED。

计划保留 async-imap 解析和 Wire 64 KiB 行 / 8 MiB 总响应 / 15 秒操作界限。EXAMINE 后按最多 4096 个消息序号窗口执行串行 UID SEARCH，按 UID 升序丢弃 offset 项，只保留 limit+1 项。UTF-8 同步 literal 与 CHARSET、严格 tagged 完成继续保留。每次列表最多 128 窗口；稀疏或无匹配查询超过预算明确返回 Limit，不返回伪造的末页。搜索时邮箱序号或身份变化故障关闭；获取头部继续 BODY.PEEK。

协议依据：[RFC 3501 UID 命令](https://www.rfc-editor.org/rfc/rfc3501.html#section-6.4.8) 和 [EXPUNGE 响应](https://www.rfc-editor.org/rfc/rfc3501.html#section-7.4.1)。UID SEARCH 中的消息序号条件仍是消息序号，结果为 UID；UID SEARCH 允许 EXPUNGE，因此不能忽略导致窗口移位的更新。不同列表调用不承诺跨会话快照。

本机仅编辑、rustfmt、Git 与远程结果审查；未执行 Cargo。
