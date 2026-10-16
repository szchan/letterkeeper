# 2026-10-16 适配、正文与应用模块拆分

本次为纯结构重构，沿用 `3cf6b01697a3dd31adbeaccb2a9cfdae27e5d279` 的行为与依赖。基线完整 CI：[38736285052](https://github.com/szchan/letterkeeper/actions/runs/38736285052)。不新增文件位置断言或人为制造行为 RED；现有成功、拒绝、边界、状态转换和安全集成测试作为回归依据。

- `letterkeeper-adapters`：crate 根保留公开重导出；Host 合同、MCP 派发/stdio 帧限制、安全文本、基本 TUI 与浏览状态分开。人工工作区分别放在 editor、review、state、input、background、drawing 和 session 中，审批能力仍与 MCP Host 分离。
- `letterkeeper-content`：公开值类型、MIME 解析/正文选择、HTML 无资源遍历、既有 composition 分开。共享字符预算仅在 crate 内可见；原 `parse_message`、`sanitize_text` 和类型路径保持。
- 应用：二进制入口只负责 worker 入口识别及参数解析，CLI 参数和命令派发分别放入 `cli_args`、`commands`。`remote` 保留完整 HumanWorkspace/Host trait 实现，转交 authentication、mail、drafts、send、sync 中的内部方法；账户配置、凭据库和 OAuth 既有边界保持。

没有改变依赖、数据库 schema、线上协议、授权、超时或正文处理策略。原公开路径经重导出保留；跨子模块必要的内部可见性只扩至父模块。未执行本机 Rust build/check/test/Clippy；本机仅执行 rustfmt、差异检查与静态搬移核对。新 revision 的完整远程 CI 结果在完成后追加；本记录不代表新增真实 Gmail、凭据服务或全部 P0 验收。
