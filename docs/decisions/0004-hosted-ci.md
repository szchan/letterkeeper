# ADR 0004：公开仓库与远程构建验证

日期：2026-10-14。状态：用户已授权公开 GitHub `szchan/letterkeeper` 与开发分支推送；CI 实施中，远程运行结果另记。需求编号、GPL-3.0-only、阶段 C 安全边界和真实服务独立关卡保持不变。

## 背景与决定

本机重型编译与测试影响日常使用。将 Cargo 构建、check/test/Clippy、release、安装编译、实际 PTY、隐私 HTTP/DNS 观察器、工作区授权闭环、Codex 数据路径及性能测量迁移到 GitHub Actions。本机仅编辑、Git/worktree、静态检查与远程结果审查；当前项目会话关闭编辑器自动 Cargo check/build，不修改无关系统配置。

每个子代理有独立分支/worktree、限定文件和可审查提交。主代理审查并集成，各任务测试提交先推送，用精确 SHA 的远程 TDD 留下真实 RED，再实现 GREEN。TDD evidence 使用独立 workflow_dispatch，指定完整 SHA、crate、精确测试名和 phase；不自动取消旧 run，不吞失败码。编译失败与断言失败区分记录，环境失败不冒充行为 RED。

CI 对 main push、所有 PR 与手动 dispatch 执行。任务分支首次推送立即创建 draft PR，后续 push 由 PR 更新自动验证；PR 建立前用手动 dispatch 验证该分支。`ci-gate` 汇总完整工作区与 Codex 数据路径，所有必需任务实际成功才可通过，失败/取消/跳过不能成为 GREEN。主代理集成后的新 SHA 必须重新验证；main 保护要求 PR 和最新 required check，禁止强推/删除。设置完成之前不声称保护生效。

## 隐私与运行边界

公开仓库和 Actions 日志/摘要/产物均可被他人读取，只能使用合成邮件、虚构身份和受控服务。禁止上传 `.local/`、真实正文/联系人/附件、个人配置、系统凭据库、OAuth token 或 client secret；不在公共 CI 运行真实 Gmail 或真实凭据库验收。用户已明确允许保留既有 Git 作者个人邮箱，主代理负责发布前历史与工作区审查；该决定不授权公开其他私有数据。

runner 必须具备 Linux bubblewrap/namespace、loopback、PTY、工具链和独立 Codex 客户端路径。原隔离与权限测试不删、不降级、不用 skip 让结果变绿；环境不足时修复远程环境或如实阻塞。依赖获取的合法网络与正文无外联仍分别控制；依赖下载成功不证明正文隔离。

远程使用独立 checkout/target，可缓存依赖，但缓存不是验收证据。日志与产物关联完整 commit SHA、run URL 和环境；测试结果只适用于被测版本。性能建立远程新基线，记录系统/工具链/负载/冷热缓存和完整进程树，不与 Arch 本机历史样本直接混比，也不降低安全与性能验收要求。

## 结果与限制

重型负载离开本机，失败证据和集成版本可公开复核；同时需要等待远程任务、维护 runner 环境并承担公开日志的隐私约束。当前阶段 C 历史结果仍仅代表原受控环境；新 CI 未完成时保持待验证。

公开仓库建立与本次开发推送已有授权；发行版、真实账户收发/删除/状态修改仍需单独授权。真实 Gmail、系统凭据库、图形前端及受限自动发送仍未验收，不能把远程 CI 通过写成全部 P0 或生产可用。

操作规范见 [协作流程](../WORKFLOW.md)，执行入口见 [CI 验证](../testing/ci.md)，实际结果见 [开发记录](../development/2026-10-14-hosted-ci.md)。
