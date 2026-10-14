# Git、worktree 与远程 TDD 协作流程

2026-10-12 用户要求 Git 管理版本、worktree 隔离工作、主代理指导子代理、按任务复杂度控制模型成本、开发记录写入 docs 与 TDD。2026-10-14 用户授权公开 GitHub `szchan/letterkeeper` 与开发分支推送，并要求重型构建/验证迁移到 GitHub Actions。选择及限制见 [ADR 0004](decisions/0004-hosted-ci.md)。

## 分工与模型

主代理负责需求、接口、集成、隐私/授权审查和最终验收。协议、解析、状态机优先使用 GPT-6.1-Sol high；界面/适配常规实现可用 GPT-6.1-Sol medium；文档、清单、已有代码的测试运行优先 GPT-6-Luna。遇到容量限制可用 GPT-6-Sol 接续。模型名称是已记录选择，不是对成本或可用性的永久承诺。复杂问题升级给主代理，不默认让所有任务使用最昂贵模型。

每个子任务必须给定独立分支、worktree、可改文件范围、公开 API 和验收测试。可以只读其他 worktree 核对接口，但不跨 worktree 编辑。不要覆盖用户改动；不明来源改动留在原处，集成时只挑选已审查提交。子代理不得共用主代理分支、直接写 main 或复用另一任务 worktree。

## 开始与集成

1. 依次阅读 README、需求、LICENSE，再看当前 ADR 和 STATUS。
2. 确认 Git 状态，从已记录基线创建独立任务分支及 `.worktrees/任务名`。目录被忽略；本机私有运行数据只放 `.local/`，不能提交或上传。
3. 本机只编辑、执行 Git、静态检查和审查远程结果。不执行 Cargo build/test/check/Clippy、release 基准或安装编译；当前项目的编辑器自动 Cargo 检查/保存构建也应关闭。不因此修改用户全局编辑器配置或无关系统配置。
4. 接口变化先通知相关任务。依赖版本由集成分支统一锁定；子代理不提交独立解析生成的 Cargo.lock。远程每次任务使用独立 checkout/target；共享依赖缓存不能当成跨 worktree 编译产物。
5. 推送各自任务分支后立即创建 draft PR；main push 和所有 PR 触发 CI，后续任务分支提交由 PR synchronize 自动验证。首个 PR 创建前可在该分支手动 dispatch CI，避免漏验；无需同时为每次 push 和 PR 重复运行。等待并记录对应精确 SHA 的结果。子代理提交及远程验证后，主代理审查并 cherry-pick 到集成分支。
6. 集成产生的新 SHA 必须再次推送并跑完整 CI；任务分支的 GREEN 不替代集成结果。PR 合并前要求该 PR 当前集成 revision 的 `ci-gate` 通过；main 保护要求 PR、最新 required check，并禁止强推和删除。冲突解决、重构或新增提交后重新验收。
7. 创建公开仓库与本次开发推送已有用户授权；发行版、真实账户收发/删除/状态改变仍单独授权。公开仓库不等于完成产品验收，main 保护实际配置与远程结果应记录，不能只凭文档声称已生效。

## 远程 TDD

- 根据稳定需求编号先写行为/边界测试并提交 `test:`。尽可能用可编译 stub 让 RED 来自断言；API 尚不存在造成的编译 RED 必须如实记录，不能伪装为行为失败。
- 将测试提交推送到任务分支，取得完整 40 位 SHA。使用 `TDD evidence` 的 workflow_dispatch 指定该 SHA、crate、精确 test 名称和 RED。检查 workflow 实际 checkout SHA 与请求一致，测试确实被选中执行，失败来自预期缺口。
- RED run 必须保留真实失败 conclusion、退出码和日志；不能用 `continue-on-error`、吞掉返回码或把预期失败改成绿色。TDD evidence 不因下一次推送自动取消；先等 RED 完成并记录 run URL，再写实现。配置/环境失败先修复，不能当作行为 RED。
- 实现最小改动，提交 `feat:`/`fix:` 并推送。对新 SHA dispatch GREEN；再等待完整 CI 门禁。实际执行入口、参数、选择器和结果检查见 [CI 验证](testing/ci.md)。GREEN 和集成 SHA 的结果各自记录，不以分支名称或某次历史成功代替。
- 新发现 bug 先有可复现回归测试。正向控制也要验证：网络监听器能接收父进程流量；测试子进程保持 stdin 打开，避免 EOF 假通过。
- 每个逻辑单元覆盖正常、拒绝、边界、状态转换。跨进程持久化、并发争抢、断连及下游适配用集成测试验证。不盲目追求覆盖率；文件位置或测试数量不代替安全性质、协议往返和终端行为。

远程操作示例（先将自己的分支推送；替换为真实 SHA 和精确测试名）：

```sh
git push -u origin test/example
gh pr create --repo szchan/letterkeeper --draft --base main --head test/example \
  --title "test: example" --body-file /tmp/letterkeeper-pr.md
# workflow 来自 main，commit 是被测任务分支上的提交
gh workflow run tdd.yml --repo szchan/letterkeeper --ref main \
  -f commit=完整40位SHA -f crate=letterkeeper -f test=精确测试名 -f phase=RED
gh run list --repo szchan/letterkeeper --workflow tdd.yml
gh run view RUN_ID --repo szchan/letterkeeper --log
```

run list/view 后核对输入与 checkout 输出，而非把“最新 run”默认为自己的任务。Actions run 的事件 SHA 与 dispatch 的被测 commit 可能不同；以被测 checkout SHA 为准并同时记录 run URL。

## 完成条件

远程完整门禁运行 `cargo test --locked --workspace`、fmt、Clippy 无警告；真实 PTY、正文 sandbox HTTP/DNS/宿主文件检查、工作区发送授权闭环及实际 Codex MCP 数据路径保留。发送相关回归包含内容绑定、并发、重启和 Uncertain。无 loopback、PTY、bubblewrap/namespace 或 Codex 时报告阻塞并修复 runner，不跳过失败项来让汇总通过。

公开 Actions 日志、摘要、产物只用合成邮件、虚构身份及受控服务；不挂载/上传本机邮箱、用户配置、`.local/`、钥匙环、token 或 client secret。不将真实账户凭据配置成公开 CI secrets 来绕过独立验收。

性能也在远程测量，记录 runner/系统/工具链、负载、冷热缓存和完整进程树；建立新环境基线，不直接与 Arch 本机历史样本混比。真实 Gmail、真实 Secret Service 与公开图形前端仍见 [独立关卡](testing/live-acceptance.md)。

每模块在 `docs/testing/` 写 RED/GREEN 精确 SHA、run URL、命令、失败原因、验证环境和限制；设计放 `docs/decisions/`，过程放 `docs/development/`。README 保留真实入口，STATUS 区分历史受控通过、新环境通过与产品验收。尚未完成的远程 run 必须标为待验证。
