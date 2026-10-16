# 用 GitNexus 浏览与审阅代码

GitNexus 是独立开发工具，用于 Rust 调用关系、功能聚类与影响范围查询；不会成为邮件客户端的运行依赖。当前固定 **1.6.12**，许可证为 **PolyForm Noncommercial 1.0.0**；商业用途需另行核对上游许可。设计与限制见 [ADR 0008](../decisions/0008-gitnexus.md)。

## 安装与打开图谱

在当前 checkout 根目录运行，需要 Linux x86_64、Python 3、Git、Node.js（22.18+ 的 22.x 或 24.11+）与 npm：

```sh
python3 tools/gitnexus.py install
python3 tools/gitnexus.py index
python3 tools/gitnexus.py status
python3 tools/gitnexus.py serve
```

安装会下载固定 GitNexus 和 LadybugDB Linux x64 预编译包及传递依赖，关闭 npm lifecycle scripts、audit、fund 和 Scarf 安装遥测；不运行编译命令。工具、源码快照、索引与注册表位于被忽略的 `.local/gitnexus/`；LadybugDB 的官方 FTS 预编译扩展另使用 `~/.lbdb/extension/` 共享缓存，不存邮件或项目配置。不会写全局编辑器设置。首次安装需联网；传递依赖的实际解析记录保存在该目录的 `tool/package-lock.json`，这不是跨时间完整依赖树锁定承诺。升级必须复核预编译包、许可证与接口。

`serve` 在前台运行，浏览器打开 [本机 GitNexus](http://127.0.0.1:4747)，在仓库列表选择 `letterkeeper`；使用 Ctrl+C 停止。页面由安装包自带静态资源提供，无需上传 ZIP 或连接托管 UI。不配置聊天模型、远程 embeddings 或 LLM wiki。图页面是开发工具界面，不应用于打开或展示真实邮件。

## 索引范围与新鲜度

`index` 从当前 **Git HEAD** 读取不可变 blob，导出 Cargo 清单、`crates/` 中的 Rust、`tests/`/`tools/`/`.github/ci/` 下的 Python/shell；不复制 `.git` 历史、文档、邮件 fixtures、隐藏目录、target、vendor、未跟踪文件或未提交改动。选中路径如果是符号链接就拒绝；单文件 512 KiB、合计 32 MiB、最多 4096 个文件。

每个 checkout 独立保存快照和注册表；每次索引生成独立快照，不覆盖正在使用的旧快照。`source-revision.json` 记录原始提交与路径清单。图中路径相对于源 checkout，源码应按该提交打开；查询启动时如果 HEAD 已变化会在 stderr 提醒。提交变更后重新运行 `index`，再重启图服务或 MCP 连接以采用新快照。索引失败（包括全文搜索扩展不可用）不发布新的 current 指针。GitNexus 默认扫描规则可能进一步排除测试文件；默认执行流预算也可能截断结果，应阅读其警告。

这类导出不隔离恶意工具的 OS 文件权限，也不能发现误写在允许源码文件里的秘密。项目仍要求源码只包含合成数据；不要通过图工具处理私有运行目录。索引文本通过 MCP 被用户所选模型读取时，会进入该模型上下文。

## 用于人工审查

1. 先核对 `status` 的源码 SHA 与待审提交一致，在图页面看功能聚类与调用关系。
2. 请 Agent 从 CLI 或 MCP 查询具体入口，例如 `send_submission`、`parse_message`；先确认符号路径，再查询其上游影响。
3. 将影响范围与 Git diff、需求编号、相应 CI 证据一起审查。Graph 空结果不能证明没有调用者；宏、trait 动态分派和跨 crate 解析需要人工核对。

CLI 也能查询（具体参数以固定版本 `--help` 为准）：

```sh
python3 tools/gitnexus.py query 'send approval' --repo letterkeeper
python3 tools/gitnexus.py context --help
python3 tools/gitnexus.py impact --help
python3 tools/gitnexus.py trace --help
```

## Codex MCP

按 [Codex 官方 MCP 文档](https://developers.openai.com/codex/mcp)，在该项目 `.codex/config.toml` 中合并以下配置。该本机配置已被 Git 忽略；`args` 中的路径替换为本 checkout 的绝对路径；已有配置保留。新建/重新加载聊天后查看 MCP 工具是否已连接：

```toml
[mcp_servers.gitnexus]
command = "python3"
args = ["/absolute/path/to/letterkeeper/tools/gitnexus.py", "mcp"]
startup_timeout_sec = 60
tool_timeout_sec = 60
enabled_tools = ["list_repos", "query", "context", "impact", "trace"]
```

launcher 固定 `GITNEXUS_MCP_READ_ONLY=1`，使用本快照专用注册表，过滤继承的模型密钥、Node 注入配置和 GitNexus 环境设置。Codex allowlist 进一步只开放上述五项。不开启 rename、raw Cypher、跨仓库 group 或工作目录 diff 工具；不运行 `gitnexus setup`，不生成或替换 AGENTS.md，不安装 hooks。

## 验证与清理

```sh
python3 tests/tools/test_gitnexus.py
```

边界测试仅创建临时合成仓库。完整产品门禁仍由远程 CI 执行，GitNexus 图不替代原隐私/协议/真实客户端验证。停掉本项目 GitNexus 进程后，可删除当前 checkout 的 `.local/gitnexus/` 清理工具、所有源码快照与索引；不要删除 `.local/` 中其他账户或运行数据。移除 Codex 配置中的 `mcp_servers.gitnexus` 即可解除连接。
