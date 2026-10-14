# Hosted Codex MCP readiness

2026-10-14。范围：MCP-01、MCP-08 的受控 Codex app-server 验证入口；只使用合成数据，无模型 turn、真实认证或邮箱操作。

## 失败证据与修复边界

父分支运行 [38538057660](https://github.com/szchan/letterkeeper/actions/runs/38538057660) 的 Codex job 通过，而相同环境相关子分支运行 [38538084342](https://github.com/szchan/letterkeeper/actions/runs/38538084342) 在 `thread/start` 后首次状态读取失败于 `server_not_connected`。旧日志没有记录 `runtimeStatus` 值；异步启动是待验证的原因，不能据此断言服务器当时是 `starting`。

已安装 Codex 0.162.1 生成的官方 `v2/ListMcpServerStatusResponse.json` 声明 `notStarted`、`starting`、`connected`、`authenticationRequired`、`failed`、`cancelled`、`disabled`；`runtimeStatus=null` 表示状态不可用或配置变化。验证器在 `thread/start` 后仅通过只读 `mcpServerStatus/list` 等待已配置的两个服务器，每次响应检查数量、名称集合、重复项和无后续分页。

只有 `notStarted`、`starting` 可继续等待；两个服务器均 `connected` 才执行既有工具目录和合成正文/草稿/权限验证。其他状态、未知/缺失状态和 RPC 失败立即终止，不重启服务器、不重试工具、不产生模型 turn。30 秒总预算使用 monotonic 时间，单次 RPC 使用剩余预算，间隔最多 1 秒。错误只包含固定诊断和已知状态枚举，不打印服务端错误详情、任意状态值或凭据。

原 `--clearenv` 实际 sandbox 环境探针、每服务器精确 14 工具、禁用批准/文件/权限工具及只读、未批准发送、模型 confirm、账户覆盖拒绝检查全部保留。就绪等待不放宽任何隐私或授权边界。

## RED / GREEN

- RED：`eb4a4c36df9d553099c48e112ffc5cd1cdaa1a41`，[Actions 38539425199](https://github.com/szchan/letterkeeper/actions/runs/38539425199)。`codex-client` 在编译前执行 `python3 scripts/test_codex_readiness.py`，8 项测试运行，16 个断言/子测试失败，退出码 1；包含 pending→connected 的 `server_not_connected` 行为失败。整个 run conclusion 为 failure，未吞失败或跳过安全项。workspace 还包含已知终端 fixture 基线失败，另由父分支修复。
- 本机只执行轻量 Python stub 测试与 `git diff --check`；未执行 Cargo、实际 Codex app-server 或模型/真实认证网络调用。GREEN 的 8 项 readiness 测试及既有 11 项 CI 编排测试通过。
- GREEN 精确 SHA 与远程结果：待推送后记录；任务分支通过也不能替代主代理集成 revision 的完整门禁。

测试覆盖即刻就绪、两个异步状态及混合就绪、所有终止状态/null/未知状态、库存缺失/额外/重复/分页、后续 poll 库存变化、超时、RPC 时间计入预算和 RPC 错误不重试。stub 测试不证明实际 Codex startup 行为；实际受控客户端仍须远程完整验证。真实 Gmail、Secret Service 和图形前端不在此验收范围。
