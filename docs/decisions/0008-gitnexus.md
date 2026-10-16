# ADR 0008：GitNexus 开发辅助图谱

日期：2026-10-16。状态：采纳；实现与验证结果另行登记。

用户希望通过代码关系图更快理解、审核和指导 Agent 开发。采用 GitNexus 1.6.12 作为可选、独立的开发工具，不加入 Cargo 依赖或 LetterKeeper 发行物，不改变邮件产品的联网、正文隔离和发送批准边界。

## 范围与边界

- 从当前 Git HEAD 导出有限的已提交 Rust/Python/shell 源码与 Cargo 清单，创建独立快照；不扫描工作目录、邮件样本、文档、账户配置或历史提交。私有目录即使误被 Git 跟踪也不进入该快照。未提交改动不在索引中。
- 工具、快照、索引和专用注册表均保存在当前 checkout 被忽略的 `.local/gitnexus/`。不使用用户全局 GitNexus 注册表，不运行自动 editor setup，不修改已有 Agent 指引或安装 hooks。
- 使用官方 npm 固定版本和 Linux x64 预编译依赖，安装时禁用 lifecycle scripts，不在本机编译。索引不启用 embeddings 或 LLM wiki，不继承模型密钥/端点环境配置。只读 MCP 提供关系查询；图谱 HTTP 服务仅绑定 127.0.0.1。
- 导出是输入范围控制，不是针对任意恶意开发工具的 OS 沙箱。邮件正文仍只能走现有受限 worker；开发工具不得取得真实邮箱操作权限。

## 许可证与局限

[上游许可证](https://github.com/abhigyanpatwari/GitNexus/blob/main/LICENSE)为 PolyForm Noncommercial 1.0.0，不能将其称为 GPL 兼容或允许任意商业使用。工具独立安装，保留上游许可，不复制其实现到项目，也不打包进 GPL 应用。商业使用者需另行核对许可；本决定不是法律兼容性认证。

Tree-sitter 的 Rust 关系推断不等于 rustc 语义分析，宏、trait 动态分派、跨 crate 解析和执行流可能缺失或误判。图用于导航和影响分析，不能证明权限、安全性质或测试覆盖。快照的路径相对源 checkout 对应；图中显示的 revision 必须与待审提交核对。默认不暴露基于工作目录 diff 的 detect_changes，以免把快照误当成当前未提交工作区。

参考：[GitNexus](https://github.com/abhigyanpatwari/GitNexus)、[Codex MCP 配置](https://developers.openai.com/codex/mcp)。当前 npm 发布包与上游 main 同版本号下也可能存在实现差异，以固定安装包的实际帮助与行为为准。
