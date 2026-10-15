# IMAP 有界分页回归

基线 `a2c862268c60e6d9ce64595f1e6cba0d94630640`。远程分页 RED 已完成：测试提交 `396ef2646309e53c03a6af5275915debc78e2557`，[run 38677060953](https://github.com/szchan/letterkeeper/actions/runs/38677060953)，准确选择一个测试，`large_mailbox_first_page_uses_bounded_search_instead_of_all_uids` 在 `.unwrap()` 收到 `Limit`，raw exit 101 / test_finished；编译与环境成功。首次误填 SHA 的 run 38677029165 已取消，不作为证据。GREEN 尚待实现。

检查上游流处理发现 FETCH 同样丢弃 tagged NO/BAD，因此增加 3 封合成小邮箱回归，确保完整头部之后的拒绝、错误 tag 或 EXPUNGE 不能掩盖失败；先独立取得远程 RED 再实现该加固。

合成 TLS 服务直接生成 20,000 个十位 UID；旧 ALL 响应约 220 KiB，超过 64 KiB 单行上限但低于 8 MiB 总量，首屏应成功并只搜索首个有界窗口。UID 间隔证明窗口采用消息序号而非假定连续 UID。

回归覆盖：offset 跨窗口、准确末页和 more、空邮箱、中文与引号/反斜杠同步 literal、稀疏查询多窗口、拒绝/重复/零/超量 UID、错误 tag/缺失 SEARCH、超长响应、EXPUNGE/EXISTS/UIDVALIDITY 漂移、无匹配请求预算、缺失/重复/未请求 FETCH。现有 EXAMINE/PEEK、字符集拒绝、TLS、大小界限和其他安全测试全部保留。

计划界限为 4096 个序号/窗口、128 窗口/次列表，保持原 64 KiB 行、8 MiB 总响应和 15 秒期限。不匹配邮箱或大 offset 超过预算会返回 Limit；分页不是跨调用事务快照，邮箱变化应刷新重试。真实服务与系统凭据库验收由主代理独立进行，不进入公开测试。
