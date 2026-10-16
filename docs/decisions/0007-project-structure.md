# ADR 0007：workspace、模块与验证目录结构

日期：2026-10-16。状态：采纳；结构变更以集成后的代码和 CI 为准。

## 决定

Rust workspace 将 `crates/*` 下的生产 crate 与一个仅供测试使用的 test-support crate 纳入 workspace；test-support 只作为相关 crate 的 dev-dependency，生产 crate 之间不得依赖它。现有 crate 的职责如下：

| Crate | 边界 |
| --- | --- |
| `letterkeeper-content` | MIME/HTML 安全解析、内容类型与回复/转发合成。 |
| `letterkeeper-core` | 持久状态、草稿/批准/提交/同步状态机、权限、验证和核心类型。 |
| `letterkeeper-protocol` | IMAP/SMTP、TLS、协议运行时、wire 与协议输入验证。 |
| `letterkeeper-accounts` | 账户配置、凭据存储协调、诊断、OAuth 与账户引导。 |
| `letterkeeper-adapters` | Host 接口、MCP、TUI、人工工作区、保守文本和 UI 浏览状态。 |
| `letterkeeper` | CLI 参数和命令、应用装配、远程 Host、沙箱与 transport。 |

职责按源文件拆分，公开 crate API 保持由 `lib.rs` 导出。当前拆分边界包括：

- `letterkeeper-content/src/lib.rs` 中的 HTML、MIME 与数据类型进入 `html.rs`、`mime.rs`、`types.rs`；正文合成继续在 `composition.rs`。
- `letterkeeper-core/src/lib.rs` 中的错误、权限、草稿、批准、迁移、路径、提交、同步、类型和验证进入 `error.rs`、`permissions.rs`、`store/{mod,approvals,drafts,migrations,paths,submissions,sync}.rs`、`types.rs`、`validation.rs`。
- `letterkeeper-protocol/src/lib.rs` 中的 IMAP、SMTP、TLS、运行时、类型和验证进入 `imap.rs`、`smtp.rs`、`tls.rs`、`runtime.rs`、`types.rs`、`validation.rs`；`wire.rs` 保留独立模块。
- `letterkeeper-adapters/src/lib.rs` 的 Host、MCP、TUI 与安全文本进入 `host.rs`、`mcp.rs`、`tui.rs`、`safe_text.rs`；`human_tui.rs` 拆为 `human_tui/{background,drawing,editor,input,review,session,state}.rs`。`browser.rs` 保留浏览状态适配。
- `letterkeeper/src/main.rs` 的 CLI 参数和命令进入 `cli_args.rs`、`commands.rs`；`remote.rs` 拆为 `remote/{authentication,drafts,mail,send,sync}.rs`。应用、诊断、文件、引导、沙箱、传输与 workflow 仍有各自模块。

验证入口按用途归档：Python CI 编排进入 `tests/ci/`，实际二进制/PTY/Codex/隐私验收脚本进入 `tests/e2e/`，CI shell helper 进入 `.github/ci/`。Benchmark 是 Cargo target，由 `cargo bench` 选择；其 Python 实现位于 crate 的 `benches/` 下。该布局是本项目约定，不表示 Cargo 要求所有 benchmark 的结果写到同一位置。需要持久保留的 benchmark JSON 与下载后解包的验证构建按本项目约定放入 `target/artifacts/`；CI workflow 自身暂存发布验证产物的 `target/ci-release/` 是独立路径；`cargo clean` 会同时清除 `target/artifacts/` 和 `target/ci-release/` 的内容，之后重新生成或下载即可。历史记录中的 `output/` 路径只描述当时文件位置，不改写为新的历史事实。任何私有账户、邮件、联系人、附件或凭据均不得放入 `target/`。

## 旧入口与新入口

| 旧路径或命令 | 新路径或命令 |
| --- | --- |
| `scripts/ci/*` | `.github/ci/*` |
| `scripts/test_ci.py` | `tests/ci/test_ci.py` |
| `scripts/test_codex_readiness.py` | `tests/ci/test_codex_readiness.py` |
| `scripts/verify_terminal.py` | `tests/e2e/verify_terminal.py` |
| `scripts/verify_workspace.py` | `tests/e2e/verify_workspace.py` |
| `scripts/verify_privacy_workspace.py` | `tests/e2e/verify_privacy_workspace.py` |
| `scripts/verify_codex.py` | `tests/e2e/verify_codex.py` |
| `python3 scripts/benchmark.py target/debug/letterkeeper` | `cargo bench --locked -p letterkeeper --bench read -- --binary target/release/letterkeeper --output target/artifacts/read-performance.json` |
| `python3 scripts/benchmark_phase_c.py PATH_TO_TERMINAL_WORKFLOW` | `cargo bench --locked -p letterkeeper --bench workspace -- --binary target/release/letterkeeper --output target/artifacts/workspace-performance.json` |
| 历史下载目录 `output/` | 新的下载/benchmark 文件按项目约定放到 `target/artifacts/`；历史证据中的原路径保持原样。 |

## 理由

Cargo 官方指南描述了常见项目布局、workspace 成员和 Cargo targets，支持按 crate 与 target 建立清晰入口：[Cargo project layout](https://doc.rust-lang.org/cargo/guide/project-layout.html)、[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)、[Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html)。CI shell 辅助文件放在 `.github/ci/`、Python 脚本按 `tests/ci/` 与 `tests/e2e/` 区分，以及输出放在 `target/artifacts/`，都是 LetterKeeper 自身的整理约定，不是 Cargo 官方规定。

按职责拆分大型 crate 根模块，便于审查解析、状态转换、网络协议与入口装配；不因此改变已有业务权限或产品验收结论。README 保持为项目入口，用户操作细节集中到 [使用指南](../user-guide/README.md)，记录分类索引见本目录及 [开发记录](../development/README.md)、[验证记录](../testing/README.md)。

## 后果

Cargo bench targets 仍通过 Cargo 入口运行，当前约定命令为：

```sh
cargo bench --locked -p letterkeeper --bench read -- --binary target/release/letterkeeper --output target/artifacts/read-performance.json
cargo bench --locked -p letterkeeper --bench workspace -- --binary target/release/letterkeeper --output target/artifacts/workspace-performance.json
```

结构整理只调整代码边界和文档入口，不更改稳定需求编号、安全边界或历史验收证据。重构后的集成 revision 仍须通过远程完整门禁；本 ADR 不声称该验证已完成。
