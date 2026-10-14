# 账户接入开发记录

基线：`d951c846212c12ff95354931dee61b69c879fdce`。用户同意开展 `doctor` 与交互式账户配置，本批不操作真实邮箱或钥匙环条目。

| 任务 | 分支 / worktree | 模型 | 范围 |
| --- | --- | --- | --- |
| 诊断 | feat/account-diagnostics / .worktrees/account-diagnostics | GPT-6.1-Sol high | 无写入的目录、实际 worker、凭据元数据诊断 |
| 配置 | feat/account-onboarding / .worktrees/account-onboarding | GPT-6.1-Sol high | 有界校验、默认权限、原子不可覆盖创建 |
| CLI | feat/account-cli / .worktrees/account-cli | GPT-6.1-Sol medium | 交互引导、doctor 接线、真实 PTY 与拒绝路径 |
| 集成 | feat/account-integration / .worktrees/account-integration | 主代理 | ADR、接口、审查、文档与最终 PR |
| 只读复审 | review/account-onboarding / .worktrees/account-review | GPT-6-Luna high | 新增输入、终端审阅、目录与 D-Bus 边界，以及文档一致性 |

开始阶段发现现有 AccountRepository::new 会创建目录，save 会覆盖已有同名配置。因此诊断不能复用该构造入口；交互取消必须在初始化仓库之前完成，新账户需要独立 create-only 操作。

任务均已取得可编译 stub 的真实断言 RED，完成后再实施。新增对象路径兼容性与严格确认问题也分别取得回归 RED 后修复。

| 单元 | RED | GREEN / 任务结果 |
| --- | --- | --- |
| 目录诊断 | `5d9be0f`，[38579536219](https://github.com/szchan/letterkeeper/actions/runs/38579536219)：Unsafe 与 NotInitialized 断言不符 | `08df9fb`，[38580912691](https://github.com/szchan/letterkeeper/actions/runs/38580912691)；完整 CI [38580870187](https://github.com/szchan/letterkeeper/actions/runs/38580870187) |
| 配置 builder / create | `823f9a2`，[38579787132](https://github.com/szchan/letterkeeper/actions/runs/38579787132)：合法输入被 stub 拒绝 | `792433a`，[38580502933](https://github.com/szchan/letterkeeper/actions/runs/38580502933)；含文档最终任务 SHA `339a062` 完整 CI [38581075032](https://github.com/szchan/letterkeeper/actions/runs/38581075032) |
| 真实 PTY 引导 | `0454bec`，[38579620252](https://github.com/szchan/letterkeeper/actions/runs/38579620252)：确认后未生成配置 | `af77ce9`，[38581028596](https://github.com/szchan/letterkeeper/actions/runs/38581028596) |
| 精确 yes | `9ea4b17`，[38581347480](https://github.com/szchan/letterkeeper/actions/runs/38581347480)：空格确认仍写目录 | `ef18111`，[38581798573](https://github.com/szchan/letterkeeper/actions/runs/38581798573) |
| 合法 collection 路径 | `c8df0a3`，[38581107016](https://github.com/szchan/letterkeeper/actions/runs/38581107016)：合法自定义路径被拒绝 | `51b0184`，[38581540839](https://github.com/szchan/letterkeeper/actions/runs/38581540839) |

每个精确 TDD 运行匹配一项，RED exit 101、GREEN exit 0、均为 `test_finished`；不吞掉预期失败。完整 SHA 与命令见各 testing 文档。初版 CLI 的全套 CI 因 export 排序导致 fmt 失败，后续修正，未将该运行当作完整通过。

各子任务 PR：[诊断 #4](https://github.com/szchan/letterkeeper/pull/4)、[CLI #5](https://github.com/szchan/letterkeeper/pull/5)、[配置后端 #7](https://github.com/szchan/letterkeeper/pull/7)。集成仅挑选本批已审查提交，保留测试和实现的提交顺序；不合并其他旧分支。

独立复审发现 U+061C、U+200E/F 尚可进入账户终端审阅，主代理新增 builder 与引导的分别精确回归测试后再修复。复审也指出 STATUS 未同步本批待验收状态，已补充。

CLI 第二次完整 CI [38581758074](https://github.com/szchan/letterkeeper/actions/runs/38581758074) 的 CLI 测试通过，随后受控诊断夹具未留下调用记录而失败。没有将缺失记录当作正常通过。诊断子代理新增确定性持有写 FD 的夹具回归，证明直接 exec 临时脚本会遇到 ETXTBSY；原始失败没有 errno 信息，不能据此断定其精确起因。修复仅限测试夹具的脚本启动方式，不改变生产 busctl 或时间/输出边界。

代码集成 SHA `84b5deeb4c2bc7638fc33e96be1210773fe31667` 的[完整 CI 38583555164](https://github.com/szchan/letterkeeper/actions/runs/38583555164) 已通过，workspace、codex-client、ci-gate 均为 success；231 项登记 Rust 测试（app 67、accounts 36、adapters 36、content 38、core 33、protocol 21），其余 11 项 CI 编排和 8 项 Codex readiness Python 测试、fmt/Clippy、实际 PTY、HTTP/DNS 及 Codex 数据路径保留。登记含既有子进程/helper，不将其重复算作独立产品覆盖。子任务通过未替代本次新 SHA 集成验证。

双向格式标记 RED 为 `01f39f96bbe043523f5b469756391ea1f2732c84`：[builder 38582972917](https://github.com/szchan/letterkeeper/actions/runs/38582972917) 断言非法文件夹被接受，[引导 38583026664](https://github.com/szchan/letterkeeper/actions/runs/38583026664) 断言应提前返回安全错误却返回取消，均匹配一项、exit 101。修复后精确 SHA `84b5deeb4c2bc7638fc33e96be1210773fe31667` 的[builder GREEN 38583605713](https://github.com/szchan/letterkeeper/actions/runs/38583605713)和[引导 GREEN 38583605653](https://github.com/szchan/letterkeeper/actions/runs/38583605653) 均匹配一项、exit 0、`test_finished`。未改变合法中文文件夹支持。

集成 PR [#6](https://github.com/szchan/letterkeeper/pull/6) 后续只补各模块的实测证据和状态文档；最终 revision 合并前仍须其最新完整门禁通过。真实账户、真实凭据条目与默认桌面策略验收仍保留。本机未执行 Cargo 构建、检查、测试、Clippy 或性能负载。
