<!-- SPDX-License-Identifier: GPL-3.0-only -->
# 阶段 B 适配层验证

2026-10-13；合成数据，未访问真实账户。

RED：`cargo test --offline --locked -p letterkeeper-adapters --test phase_b`，3 测试中 2 失败；可编译 BrowserState stub 不产生首个请求，新增 MCP 路由返回 unknown tool。拒绝参数测试先通过，因为 stub 拒绝所有新增路由；GREEN 时另验成功路径及 Host 调用记录。
