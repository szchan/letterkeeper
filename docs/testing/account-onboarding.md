# 账户配置向导后端验证

对应 AUTH-03、AUTH-04、DATA-01、MCP-04 与 MAIL-05。只使用合成身份，纯配置构建不创建目录、不连接邮箱、不操作凭据库；只有显式提交后调用 create-only 仓库 API。发送权限仍受已有人工审批流程约束。

本次测试覆盖默认只读、隐式 TLS 默认端口、明确起草/发送范围、Gmail 自有 Desktop client 配置与固定端点、中文文件夹、输入边界、拒绝重复配置/符号链接/目录、并发创建仅一个赢家。Gmail 文件夹默认值只是建议，真实服务商标签与本地化名称尚未验收。

## RED

精确 SHA `823f9a25a30310252cb744a02cf53d7f67db91ed` 的 [TDD RED](https://github.com/szchan/letterkeeper/actions/runs/38579787132) 已完成并保留 failure。`letterkeeper-accounts` 的 `onboarding_defaults_and_create_only_contract` 恰好匹配一个测试；可编译 stub 在 `offline account setup should validate` 断言处返回 `InvalidConfiguration`，`raw_exit_code=101`、`state=test_finished`。不是编译或 runner 环境失败。完成并核对日志后才开始实现。

远程命令为 `cargo test --locked -p letterkeeper-accounts onboarding_defaults_and_create_only_contract -- --exact --nocapture`，运行 Ubuntu 24.04 / Rust 1.99.0；本机只执行 rustfmt 与 `git diff --check`，不执行 Cargo 构建、测试或 Clippy。

## GREEN 与限制

GREEN 精确 SHA、Actions URL 与完整 CI 结果待运行后补充。`create` 使用完整文件写入、file fsync、原子 `RENAME_NOREPLACE` 和目录 fsync；任何既有目标类型均拒绝覆盖，临时文件在失败时清理。发布后的目录 fsync 若失败返回 `AccountPersistenceUncertain`，配置可能已经存在，调用者先重新读取核实，不能盲目重试。

onboarding 接受无歧义 ASCII dot-atom 邮箱地址、DNS 名称或 IP 端点；不支持带引号/国际化邮箱地址，中文文件夹仍支持。Gmail 的默认文件夹可以覆盖；这些默认名和 ServerManaged 选择不表示真实 Gmail 已验收。Generic 的 SentCopyStrategy 必须由用户明确选择，后端没有自动猜测服务商。

本模块没有取消/终端交互：前端在最终确认前只构建输入，不实例化会创建目录的 AccountRepository。没有创建账户数据库、凭据条目或 OAuth 连接，实际 Gmail 与系统凭据库仍是另行授权的独立关卡。
