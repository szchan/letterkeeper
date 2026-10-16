# 项目结构整理记录

基线：`3cf6b01697a3dd31adbeaccb2a9cfdae27e5d279`，[完整远程 CI](https://github.com/szchan/letterkeeper/actions/runs/38736285052) success。开始前工作目录干净，现有 worktree 保留。已登记 272 项 Rust 测试（包括 3 个基准辅助入口，不把 helper 数量当成产品覆盖）。

纯模块移动复用完整回归；共享测试设施及正式基准入口先提交调用方测试，预期 RED 为缺失模块和开发依赖导致的编译失败，不能称为业务断言失败。新增检查覆盖显式路径、拒绝遗漏/重复/未知参数、带空格路径、子进程失败传播，以及共享受控 TLS 场景的实际往返和无 STORE。全部 Rust 执行在 GitHub worker。

## RED 证据

- 正式基准入口：`0172d118ec2c368716fa8dfb7e5d665c7dc6af24`，[TDD 38842619615](https://github.com/szchan/letterkeeper/actions/runs/38842619615)，failure，`test_inventory_failed` / Cargo exit 101；准确 checkout 后缺少 `benches/support/entry.rs`，属于编译 RED。
- 共享设施：`78e6d54de677fdbb6e7928001c1449faeca9024e`，[TDD 38843068306](https://github.com/szchan/letterkeeper/actions/runs/38843068306)，failure，`test_inventory_failed` / exit 101；准确 checkout 后缺少 `letterkeeper_test_support` 开发依赖。
- 一次占位输入 dispatch 被输入校验拒绝（38842606951），不当作 TDD 证据。

## 本地产物迁移

三个历史下载构建 `64dd635`、`5fde670`、`a6fbd5f` 已从根 `output/` 完整移动至 `target/artifacts/`。迁移前原 SHA256SUMS 每包 8 项全部通过；迁移后每包全部 9 个文件逐项 SHA-256 与迁移前一致，共 27 个文件。空 `output/` 已移除。仅更新被忽略的私有启动/升级脚本中的二进制路径，未执行脚本、登录、同步或收发真实邮件。账户状态和其他历史 worktree 保留。下载可重建且会被 `cargo clean` 清除。

## 测试与基准清单

原 `terminal_workflow` 的 `workspace_benchmark_child`、`workspace_tls_benchmark_child`、`workspace_tls_mcp_benchmark_child` 已迁入 `benches/workspace/workloads.rs`，不再登记为普通测试。保留 8 个 TUI 样本（injected/TLS、1k/10k、32KiB/2MiB）、4 个活跃 MCP 样本，以及 read 20 次/预载 MCP 样本、进程树采样和原指标字段。正式目标 `read`/`workspace` 均 `harness=false`，显式 `--binary`/`--output`，普通 test 不运行基准负载。新增的 3 个基准入口/路径检查与 1 个共享 TLS 实际往返检查独立登记，不用测试总数代替原安全覆盖。

共享设施只依赖既有 rcgen/rustls/tokio，无生产 crate 依赖；通过工作区 dev-dependencies 消费，无循环。协议测试中的类型转换留在本 crate 的 tests/support；单独应用 SMTP 辅助服务仍在应用 tests/support。未升级注册表依赖、工具链或 schema。

## GREEN 与兼容性

实现 SHA `c47a2ade50eddb649113de0b7a66f8383822b938` 的精确 GREEN：基准进程边界 [38843440687](https://github.com/szchan/letterkeeper/actions/runs/38843440687)、共享 TLS [38843443155](https://github.com/szchan/letterkeeper/actions/runs/38843443155) 均 success。新 `.github/ci/` TDD 入口验证旧布局基线 `3cf6b01697a3dd31adbeaccb2a9cfdae27e5d279` 的 `folders_and_read_are_tls_peek`：[38843445825](https://github.com/szchan/letterkeeper/actions/runs/38843445825) success；准确目标 SHA、单一匹配与原始退出码均由 workflow 记录。

首次完整 GREEN 实现 CI [38843418037](https://github.com/szchan/letterkeeper/actions/runs/38843418037) 在 Clippy 编译协议测试时发现本 crate 适配 helper 遗漏 `Server::start_shared`。共享设施与基准的精确测试已通过，但不能替代全 workspace 编译；补回委托原 Mailbox 场景的 helper 后重新跑完整门禁。该遗漏是搬移错误，不是新的产品行为 RED。

本地静态检查：fmt、Python AST、shell `bash -n`、全部相对 Markdown 链接；CI Python 11 项与 readiness 8 项分别从项目根和 `/tmp` 运行通过。未发现运行中的 rust-analyzer/Cargo/rustc 自动构建进程，未改全局编辑器或系统配置。

## 独立审查与路径回归

核心/协议分支最终 SHA `50dbf68709cd31317f97ab42d5795937fec2f822` 的 [38843099344](https://github.com/szchan/letterkeeper/actions/runs/38843099344)、适配/正文/应用分支最终 SHA `a8ef645c5a915a21b8648bbe5d5b947af80c2b87` 的 [38843415945](https://github.com/szchan/letterkeeper/actions/runs/38843415945) 均完整通过。集成 `2a74c883ac9822143d72baeffc1328695c302ad9` 的 [38843865264](https://github.com/szchan/letterkeeper/actions/runs/38843865264) 完整通过，原基准参数 RED 对应的精确 [GREEN 38843893315](https://github.com/szchan/letterkeeper/actions/runs/38843893315) 通过。

独立审查逐字确认共享性能服务和协议场景实现保持原样，只替换生产专用类型与凭据构造。没有生产依赖环。审查发现 Cargo 会在 crate 根目录启动基准，文档 `target/...` 相对路径不能按 cwd 解释。回归 SHA `87ba551bd29486238c6323b15070a66b22303da1` 的 [精确 RED 38844237242](https://github.com/szchan/letterkeeper/actions/runs/38844237242) 正确选中并执行 1 个测试，因找不到 workspace-relative 合成二进制失败；`test_finished` / raw exit 101。等待真实 failure 后才修复：两类显式路径统一以 workspace 根目录解析，绝对路径不变。release 原本传绝对路径，既有负载、指标和门槛未因此改变。

## 集成源码验收

源码 SHA `6321cfe4730b513ef389584875dea274fd7be184` 的 [完整 CI 38844476241](https://github.com/szchan/letterkeeper/actions/runs/38844476241) success，含 fmt、Clippy all-targets、workspace 测试、真实 PTY、人工工作区、HTTP/DNS 观察器和实际 Codex 路径；路径回归的 [精确 GREEN 38844514059](https://github.com/szchan/letterkeeper/actions/runs/38844514059) success。PTY 覆盖 120×40、80×24、40×12、20×6，另保留 24×12 浏览；HTTP/DNS 正向控制可达，正文新增 DNS/HTTP 请求均为 0，未修改宿主网络配置。

[源码测试声明清单](../testing/structure-test-inventory.json) 保存基线 272 个和重组后 273 个 `#[test]`/`#[tokio::test]` 函数的路径及名字；差集恰为 3 个迁到 benches 的旧 helper 和 4 个新检查。该清单是静态声明，不等于新增独立产品覆盖，也不累加子进程再次运行 helper 时打印的 harness 结果。原有成功、拒绝、边界、状态/并发和隔离回归保留。385 个 registry lock 条目逐项与基线相等，原 LICENSE 字节未变。

首轮正式 Cargo bench/release 验证在 `2a74c883ac9822143d72baeffc1328695c302ad9` 上的 [38843890485](https://github.com/szchan/letterkeeper/actions/runs/38843890485) success，执行 release workspace 测试、`cargo bench --bench read` 和 `--bench workspace`，打包 source/vendor/原始 notices/manifest/SHA256SUMS 的既有白名单。此预合并样本不替代合并后 main 的新构建。

最终文档 revision 的完整 `ci-gate` 及合并后 main 的新门禁/构建以 [PR #20](https://github.com/szchan/letterkeeper/pull/20) 的不可变 SHA 与 Actions 链接继续登记；不因文档或分支旧 SHA 通过而跳过新 revision。全部子任务保留独立分支/worktree：核心/协议使用 Sol high，常规模块使用 Sol medium，文档索引使用 Luna，主代理负责共享设施/基准/CI、接口审查和集成。没有真实账户操作或发行版发布，原 P0 缺口不变。
