# 大邮箱有界分页修复

需求：MAIL-02、MAIL-06、MCP-03、PERF-01。真实服务的只读验收暴露了大邮箱列表先获取全部 UID、再分页的问题。此记录仅含合成数据，不记录真实邮箱规模或内容。

独立分支 `fix/imap-bounded-paging`，基线 `a2c862268c60e6d9ce64595f1e6cba0d94630640`。先新增 TLS 合成服务回归；分页与 FETCH 完成校验均先保留远程真实断言 RED，链接与精确 SHA 见 [验证记录](../testing/imap-bounded-paging.md)。

实现保留 async-imap 解析和 Wire 64 KiB 行 / 8 MiB 总响应 / 15 秒操作界限。EXAMINE 后以每窗口最多 4096 个消息序号执行串行 UID SEARCH，按 UID 升序丢弃 offset 项，只保留 limit+1 项。UTF-8 同步 literal 与 CHARSET、严格 tagged 完成继续保留。每次列表最多 128 窗口；稀疏或无匹配查询超过预算明确返回 Limit，不返回伪造的末页。搜索时邮箱序号或身份变化故障关闭；获取头部继续 BODY.PEEK。

协议依据：[RFC 3501 UID 命令](https://www.rfc-editor.org/rfc/rfc3501.html#section-6.4.8) 和 [EXPUNGE 响应](https://www.rfc-editor.org/rfc/rfc3501.html#section-7.4.1)。UID SEARCH 中的消息序号条件仍是消息序号，结果为 UID；UID SEARCH 允许 EXPUNGE，因此不能忽略导致窗口移位的更新。不同列表调用不承诺跨会话快照。

本机仅编辑、rustfmt、Git 与远程结果审查；未执行 Cargo。

另外检查 async-imap 0.12 的 FETCH stream 时发现其同样忽略 tagged NO/BAD。经主代理确认，用上游解析后的 Response / AttributeValue 实现列表头部的准确完成检查，新增小邮箱 RED 证明完整头部不能覆盖后续拒绝或漂移；不扩大到其他 API。主代理批准额外调整 release 合成性能服务使其遵守序号 SEARCH 条件，保持原负载和性能门槛。


实现 `e90fddcc119cb61b8210c370897aa4ecbb22b278` 三项精确 GREEN 全部通过。补充跨窗口 UID 回退、ASCII 匹配与空结果、乱序响应后，`2c4c019a4f3028a14af09524f7bfe017dcad6715` 的[完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38677574186) 全部通过；协议集成测试新增 11 项，共 32 项通过。真实服务重试与集成版本验收由主代理继续独立记录，本分支结果不替代它们。
