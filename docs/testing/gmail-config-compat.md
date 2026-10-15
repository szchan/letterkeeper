# Gmail OAuth 配置标签兼容验证

2026-10-15。覆盖 AUTH-01、UX-03 的配置入口；仅使用合成配置，不执行 OAuth、凭据库或真实邮箱操作。

README 指导手写 `gmail_oauth`，而 `Authentication::GmailOAuth` 的 `snake_case` 派生标签为 `gmail_o_auth`，导致前者在 JSON 反序列化时被拒绝。本修复添加反序列化别名，保留既有保存格式。

回归测试 `gmail_oauth_documented_and_legacy_tags_preserve_serialization` 使用 `support::config()`，分别断言旧标签可读、未知标签拒绝、文档标签可读、两者产生相同有效配置，且序列化始终输出 `gmail_o_auth`。没有改变未知字段拒绝、权限或配置校验。

## 远程 RED

- SHA：`ce3096b8b63c0e1df1729c55226c249afb1065e3`（仅测试）。
- [TDD evidence run](https://github.com/szchan/letterkeeper/actions/runs/38674698600)：completed / failure。
- crate：`letterkeeper-accounts`；精确测试名如上；phase：RED。
- 目标 SHA 已由 workflow 校验，`matched_tests=1`、`state=test_finished`、`raw_exit_code=101`。
- 真实断言失败：`the documented gmail_oauth authentication tag must deserialize`；不是编译或环境失败。

## GREEN 与完整 CI

- 实现 SHA：`da65ada7c4392311bff3d152842e6c1b5069ef28`。
- [GREEN TDD evidence](https://github.com/szchan/letterkeeper/actions/runs/38674916147)：completed / success；相同精确测试，`matched_tests=1`、`state=test_finished`、`raw_exit_code=0`。
- [该实现 SHA 的完整 CI](https://github.com/szchan/letterkeeper/actions/runs/38674919136)：completed / success，workspace、codex-client、ci-gate 均通过。包括 fmt、Clippy、workspace 测试、实际 PTY、编辑/审阅/批准/提交闭环、可达 HTTP/DNS 隐私观察器与实际 Codex MCP 数据路径。
- 运行环境：GitHub-hosted Ubuntu 24.04，仓库固定 Rust 1.99.0 与既有 sandbox 预检。

本机只执行 rustfmt 和 `git diff --check`，未执行 Cargo 构建、测试、check 或 Clippy。后续文档提交及主代理集成 revision 仍须各自的当前完整门禁；上述链接明确对应实现 SHA。

远程结果只证明配置兼容性与既有受控回归；不替代真实 Gmail、系统凭据库或发行验收。
