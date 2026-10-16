# ADR 0007：Rust workspace 与验证目录结构

日期：2026-10-16。状态：采纳。

## 决定

按 Cargo workspace 组织项目。根 workspace 使用 `crates/*` 成员布局，生产 crate 位于 `crates/`，仅供测试使用的共享支持代码放入 dev-only test-support crate。基准属于 Cargo target：读取基准放在 `crates/letterkeeper/benches/read/`，工作区基准放在 `benches/workspace/`。测试辅助脚本按用途放在 `tests/ci/` 和 `tests/e2e/`；CI shell 辅助文件放在 `.github/ci/`。Cargo bench 输出与下载的验证构建放在 `target/artifacts/`，`cargo clean` 会清除该目录内容，需重新下载或生成。不得将私有账户、邮件、联系人、附件或凭据写入 `target/`。

README 负责项目定位、当前状态、最短开始路径和文档导航；详细账户、CLI、TUI、MCP、离线演示及开发用法放在 `docs/user-guide.md`。`docs/README.md` 及分类索引提供可发现性。已有 `docs/REQUIREMENTS.md`、`docs/STATUS.md` 和 ADR 文件地址保持稳定。

## 理由

Cargo 官方指南将常见项目布局、workspace 成员和 Cargo targets 分别说明，支持按 crate、测试与 benchmark target 建立明确入口：[Cargo project layout](https://doc.rust-lang.org/cargo/guide/project-layout.html)、[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)、[Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html)。CI 辅助脚本放入 `.github/ci/` 是本项目约定，不是 Cargo 或 GitHub Actions 强制布局。短 README 让第一次接触项目的人先了解状态和入口，操作细节仍有单一长期指南。

## 后果

workspace 根目录以一致的 crate glob 维护成员；test-support 不能成为生产依赖。基准仍可通过 Cargo bench target 调用，例如：

```sh
cargo bench --locked -p letterkeeper --bench read -- --binary target/release/letterkeeper --output target/artifacts/read.json
cargo bench --locked -p letterkeeper --bench workspace -- --binary target/release/letterkeeper --output target/artifacts/workspace.json
```

结构迁移只调整组织与文档入口，不改变产品验收事实、需求编号或安全边界。具体实现和集成 revision 仍需按托管 CI 规则验证。
