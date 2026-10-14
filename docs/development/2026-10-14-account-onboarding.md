# 账户接入开发记录

基线：`d951c846212c12ff95354931dee61b69c879fdce`。用户同意开展 `doctor` 与交互式账户配置，本批不操作真实邮箱或钥匙环条目。

| 任务 | 分支 / worktree | 模型 | 范围 |
| --- | --- | --- | --- |
| 诊断 | feat/account-diagnostics / .worktrees/account-diagnostics | GPT-6.1-Sol high | 无写入的目录、实际 worker、凭据元数据诊断 |
| 配置 | feat/account-onboarding / .worktrees/account-onboarding | GPT-6.1-Sol high | 有界校验、默认权限、原子不可覆盖创建 |
| CLI | feat/account-cli / .worktrees/account-cli | GPT-6.1-Sol medium | 交互引导、doctor 接线、真实 PTY 与拒绝路径 |
| 集成 | feat/account-integration / .worktrees/account-integration | 主代理 | ADR、接口、审查、文档与最终 PR |

开始阶段发现现有 AccountRepository::new 会创建目录，save 会覆盖已有同名配置。因此诊断不能复用该构造入口；交互取消必须在初始化仓库之前完成，新账户需要独立 create-only 操作。

远程 TDD、任务 PR 和集成验证尚待完成；结果不会用历史 CI 替代。
