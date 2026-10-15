# IMAP 有界分页回归

基线 `a2c862268c60e6d9ce64595f1e6cba0d94630640`。远程分页 RED 已完成：测试提交 `396ef2646309e53c03a6af5275915debc78e2557`，[run 38677060953](https://github.com/szchan/letterkeeper/actions/runs/38677060953)，准确选择一个测试，`large_mailbox_first_page_uses_bounded_search_instead_of_all_uids` 在 `.unwrap()` 收到 `Limit`，raw exit 101 / test_finished；编译与环境成功。首次误填 SHA 的 run 38677029165 已取消，不作为证据。GREEN 尚待实现。

检查上游流处理发现 FETCH 同样丢弃 tagged NO/BAD，因此增加 3 封合成小邮箱回归，确保完整头部之后的拒绝、错误 tag 或 EXPUNGE 不能掩盖失败；先独立取得远程 RED 再实现该加固。测试提交 `1dda793923b13238d292e2ce192a4d3bb007d7e9`：[tagged 完成 RED](https://github.com/szchan/letterkeeper/actions/runs/38677246732)、[头部后漂移 RED](https://github.com/szchan/letterkeeper/actions/runs/38677250785) 均准确选择一个测试，raw exit 101 / test_finished；旧实现意外返回成功 MessagePage。环境与编译均通过。

合成 TLS 服务直接生成 20,000 个十位 UID；旧 ALL 响应约 215 KiB，超过 64 KiB 单行上限但低于 8 MiB 总量，首屏应成功并只搜索首个有界窗口。UID 间隔证明窗口采用消息序号而非假定连续 UID。

回归覆盖：offset 跨窗口、准确末页和 more、空邮箱、中文与引号/反斜杠同步 literal、稀疏查询多窗口、拒绝/重复/零/超量 UID、错误 tag/缺失 SEARCH、超长响应、EXPUNGE/EXISTS/UIDVALIDITY 漂移、无匹配请求预算、缺失/重复/未请求 FETCH。现有 EXAMINE/PEEK、字符集拒绝、TLS、大小界限和其他安全测试全部保留。

计划界限为 4096 个序号/窗口、128 窗口/次列表，保持原 64 KiB 行、8 MiB 总响应和 15 秒期限。不匹配邮箱或大 offset 超过预算会返回 Limit；分页不是跨调用事务快照，邮箱变化应刷新重试。真实服务与系统凭据库验收由主代理独立进行，不进入公开测试。


实现按消息序号窗口搜索，UID 搜索仍由 async-imap/imap-proto 解析。每个窗口拒绝重复、零值、超量、跨窗口非递增 UID 与多个 SEARCH 响应；只在准确的 OK tag 完成后使用结果。头部 FETCH 使用相同上游类型并验证准确 tagged OK、全量预期 UID、无重复/未请求 UID，完成前的 EXPUNGE/EXISTS/UIDVALIDITY 漂移故障关闭。未修改读正文、Message-ID 证据查询或其他 API 的解析路径。

发布基准的合成服务器此前忽略 SEARCH 的序号条件；现在按条件返回交集并识别插入序号条件后的 UTF-8 literal。1k/10k 负载规模、原文长度、性能断言不变；由主代理的远程 release 验证覆盖。

GREEN 与完整 CI 待准确实现 SHA 的远程运行完成后补充。本机只执行 rustfmt / diff 检查，未执行 Cargo。
