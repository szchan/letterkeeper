# 托管 CI 验证

本轮按照用户要求不在本机执行 Cargo 构建、测试或 Clippy。轻量 Python 编排测试允许本地运行。

## RED 记录

`python3 scripts/test_ci.py`：编排接口 stub 抛出 `NotImplementedError`，7 个测试、17 个测试/子用例错误；这是实现前真实失败，不是 Rust 业务断言失败。

完整托管验证尚未运行；后续追加确切 commit SHA、Actions run URL 与结果。

## GREEN 与运行环境

`python3 scripts/test_ci.py`：9 项通过（子用例覆盖非法 SHA、crate、test、phase）；调用 Cargo 的接口在本地使用替身，仅验证参数数组、准确选择、拒绝零/多个匹配、SHA 不符拒绝执行、编译 RED 与原始失败不转绿。脚本语法用 `bash -n scripts/ci/*.sh` 验证；本机没有执行 Cargo。

`.github/workflows/ci.yml` 在完整 `ubuntu-24.04` VM 上运行，无 slim 容器。`workspace` 执行 fmt、Clippy、完整 workspace 测试、构建、真实终端、编辑/审阅/发送工作区及可达 HTTP/DNS 隐私脚本；独立 `codex-client` 安装公开 Codex CLI 并运行现有实际 app-server 脚本，无 auth、无模型回合。`ci-gate` 仅在两个 job 都为 success 时通过；失败、取消或跳过均不通过。权限仅 `contents: read`，checkout 不保留凭据，不使用个人 secret、不上传测试目录。只缓存 Cargo registry/git，不缓存 target。

首次 subagent 分支 push 后立即创建 draft PR；后续 PR 更新自动 CI。main push 自动 CI，其他尚无 PR 的分支用 `gh workflow run ci.yml --ref 分支`。这样正常 PR 分支只跑一次，无 push/PR 重复。普通 CI 取消同一 PR 的过时运行；TDD 专用 workflow 无 concurrency 取消。

`setup-linux.sh` 仅允许 GitHub hosted VM，安装 bubblewrap 等依赖；先实际测试两层 user/network namespace、无用户目录挂载及隔离网络。预检失败即失败，不跳过沙箱，不卸载或改写 AppArmor profile、不修改全局 sysctl。若未来 runner 镜像改变并导致拒绝，先记录真实失败与明确原因，再独立审查最小修复；应用 worker 另由真实 Rust/隐私测试覆盖。机制依据 [Ubuntu 24.04 release notes](https://documentation.ubuntu.com/release-notes/24.04/) 的 user namespace 约束。Ubuntu worker 路径兼容性在远程完整运行之前不作通过声明。

## 固定版本来源

2026-10-14 查询官方端点确认：

- Rust `1.99.0`：[版本 manifest](https://static.rust-lang.org/dist/channel-rust-1.99.0.toml) 的发布日期 2026-10-01，Linux x86_64 available=true；不是根据本机版本猜测。
- [actions/checkout](https://github.com/actions/checkout/commit/11d5960a326750d5838078e36cf38b85af677262)：v4 resolved SHA `11d5960a326750d5838078e36cf38b85af677262`。
- [actions/cache](https://github.com/actions/cache/commit/0057852bfaa89a56745cba8c7296529d2fc39830)：v4.3.0 resolved SHA `0057852bfaa89a56745cba8c7296529d2fc39830`。
- [Codex rust-v0.162.1](https://github.com/openai/codex/releases/tag/rust-v0.162.1)：`codex-x86_64-unknown-linux-musl.tar.gz`，GitHub release asset digest `sha256:86f268d81b898f3e144c5ecff2ad2fda2c5802e042d6195b72538fe0fa125057`，下载后必须校验，失败拒绝执行。

action/tag/asset 元数据使用 `gh api` 从官方仓库读取；更新版本需重新查证并审查源码变化，不使用浮动 SHA。

## 远程 TDD 原始证据

`.github/workflows/tdd.yml` 的 `workflow_dispatch` 输入为 `commit`（40 位小写 SHA）、`crate`（六个工作区 crate 之一）、`test`（准确 Rust test 名称）、`phase`（RED/GREEN）。先从 workflow ref 读取验证脚本，目标 SHA 独立 checkout 至 subject，比较实际 HEAD，列举测试确认恰好一个匹配，再 `cargo test --locked -p CRATE TEST -- --exact --nocapture`。用户输入仅放 env、受限 checkout ref 和 Python 参数数组，不插入 shell 代码。

```sh
gh workflow run tdd.yml --ref main -f commit=完整SHA -f crate=letterkeeper-core -f test=module::test_name -f phase=RED
gh run list --workflow tdd.yml
gh run view RUN_ID --log
```

phase 仅为记录标签，RED 断言/编译失败保留 Actions failure；没有 continue-on-error、`|| true` 或“预期失败即通过”。原始日志、step summary 的 `TDD_EVIDENCE` 包含目标 SHA、crate/test/phase、匹配数、raw_exit_code 和状态。编译 RED 标记 `test_inventory_failed`；测试 RED 标记 `test_finished` 并保留 Cargo exit。RED 意外通过仍记录 exit=0，审查者必须核对断言，不能仅凭标签声称取得失败证据。等待 run 完成、记录 URL 后才开始 GREEN，不在正在跑的 RED 分支上覆盖证据。

## 手动 release 与性能样本

`.github/workflows/release.yml` 仅 `workflow_dispatch`，不作为每次提交的性能门禁、不发布 GitHub Release。运行完整 release workspace 测试/构建，再执行 `benchmark.py` 与 `benchmark_phase_c.py` 的合成邮件、1k/10k 邮箱、正文/编辑/TLS/MCP 样本；共享 VM 结果不是稳定专机基准。失败保留失败，不降低既有脚本断言。

```sh
gh workflow run release.yml --ref main
gh run list --workflow release.yml
```

成功后仅上传 `target/ci-release/` 中明确复制的二进制、对应 SHA 的 `git archive` 源码、原始 LICENSE、合成性能 JSON 和 manifest；不 glob 私有目录、credential store 或完整 workspace。Artifact 保留 7 天，无秘密权限。固定 [actions/upload-artifact v4.6.2](https://github.com/actions/upload-artifact/commit/ea165f8d65b6e75b540449e92b4886f43607fa02) SHA `ea165f8d65b6e75b540449e92b4886f43607fa02` 由官方 GitHub API 查询确认。当前只是工作流入口；完整 release/performance run 未完成之前不称为已经验证。
