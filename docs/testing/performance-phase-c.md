# 阶段 C 性能测量

日期：2026-10-13。`scripts/benchmark_phase_c.py` 是 release PTY 基准入口，输入为 release 构建的 `terminal_workflow` 集成测试可执行文件。工作负载为合成 1,000 与 10,000 封元数据、32 KiB 常用原文及 2 MiB 原文边界。脚本分别运行注入 Host 的 UI 路径、稀疏合成 TLS IMAP + RemoteHost + 受限正文 worker，以及该受控 Host 上的官方 SDK MCP stdio 交互；按页提供 50 条元数据，测量温启动、帮助键与 32 KiB/1 MiB 编辑器正文输入的重绘 p95、阅读渲染、MCP 列表/搜索/阅读、进程树采样 RSS/PSS 与两秒空闲 CPU。门槛为两类输入重绘 p95 ≤100 ms、单核空闲 CPU ≤1%。

```sh
cargo test --release -p letterkeeper --test terminal_workflow --no-run --message-format=json
python3 scripts/benchmark_phase_c.py /path/to/release/terminal_workflow-TEST-HASH
```

TLS 服务只在测试进程中按需生成有限邮件；不预先分配 10,000 × 32 KiB 的正文。2 MiB 是包含头部的完整 EML 上限，安全文档输出另有 64 Ki 字符预算。它不覆盖附件、公开 MCP 客户端、真实账户或冷缓存；`/proc` 轮询也可能漏掉短暂内存峰值。测试框架在 MCP 服务前输出诊断行，基准脚本跳过这些行；这不是公开客户端兼容性测试。脚本没有使用 `drop_caches`，不把温启动称为冷启动。**本文件没有性能结果；必须在集成后的 release 构建实际运行并记录机器、CPU、内核及输出后才能判断门槛。**
