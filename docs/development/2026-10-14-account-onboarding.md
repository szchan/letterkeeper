# 账户接入开发记录

基线：`d951c846212c12ff95354931dee61b69c879fdce`。用户同意开展 `doctor` 与交互式账户配置，本批不操作真实邮箱或钥匙环条目。

| 任务 | 分支 / worktree | 模型 | 范围 |
| --- | --- | --- | --- |
| 诊断 | feat/account-diagnostics / .worktrees/account-diagnostics | GPT-6.1-Sol high | 无写入的目录、实际 worker、凭据元数据诊断 |
| 配置 | feat/account-onboarding / .worktrees/account-onboarding | GPT-6.1-Sol high | 有界校验、默认权限、原子不可覆盖创建 |
| CLI | feat/account-cli / .worktrees/account-cli | GPT-6.1-Sol medium | 交互引导、doctor 接线、真实 PTY 与拒绝路径 |
| 集成 | feat/account-integration / .worktrees/account-integration | 主代理 | ADR、接口、审查、文档与最终 PR |

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

集成 PR [#6](https://github.com/szchan/letterkeeper/pull/6) 的新 SHA 完整门禁待运行，任务分支成功不替代集成通过。真实账户、真实凭据条目与默认桌面策略验收仍保留。
