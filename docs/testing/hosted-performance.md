# GitHub 托管性能样本

2026-10-14 手动 release/performance workflow 在 GitHub-hosted Ubuntu runner 上完成：[Actions run 38537562629](https://github.com/szchan/letterkeeper/actions/runs/38537562629)。测量对象是源码提交 [`94ee542dba450083bc0c59cce50ab690dac9aaf3`](https://github.com/szchan/letterkeeper/tree/94ee542dba450083bc0c59cce50ab690dac9aaf3) 构建的 release 工作负载；没有使用真实邮箱或凭据。

Runner 为 Ubuntu 24.04.5、Linux `6.17.0-1022-azure`、Rust 1.99.0、bubblewrap 0.9。未在测量记录中找到 CPU 型号，因此不推断硬件型号或与本机 Arch 样本的速度差异。artifact ID 为 `11880208853`，压缩后 `64,558,996` 字节，Actions 保留 7 天。它是可下载的工作流验证产物，不是 GitHub Release，也没有自动发布发行版。

下载后对 `SHA256SUMS` 执行校验，清单中的 7 个 payload 与 manifest 共 8 个文件均通过。release 二进制 SHA-256 为 `81ad740b1271f37f759c19b20b59feb511a75d688e5eed8dac3cbbd72f27888a`；manifest SHA-256 为 `e59036b4e85f1906b57f70621d423c7a560466803b1351e72ecdfedc5a18d5af`。两份下方 JSON 是从 artifact 原样复制的合成性能报告，其 SHA-256 分别记录在表格中。

## 阶段 A 基线

阶段 A 使用 1,089 字节合成邮件、温缓存、20 次读取。release workflow 明确调用 `target/release/letterkeeper`。完整数据见[原始 JSON](hosted-performance-phase-a.json)，SHA-256：`2fcd69e5dadda447b2f24d956acffdcf47b236fafb2ae5ce5f19e1a769e8996e`。

| 路径 | 结果 |
| --- | ---: |
| 安全读取 median / p95 | 13.31 / 13.60 ms |
| 采样进程树峰值 RSS / PSS | 14,504 / 6,265 KiB |
| 子进程累计 CPU | 0.085 s |
| 已预载、等待握手的 MCP RSS / PSS | 8,420 / 6,278 KiB |

这项基线没有搜索、真实账户、已认证 MCP 工作负载或冷缓存结果；1 ms 采样可能漏掉瞬时峰值。

## 阶段 C PTY 与活跃 MCP

阶段 C 采用合成的 1,000 / 10,000 封元数据、32 KiB 常见负载及 2 MiB 边界负载，测量 TUI 和受控 TLS IMAP/MCP。完整数据见[原始 JSON](hosted-performance.json)，SHA-256：`df894ff5be7e7ea41dc198036979c713b745b666bd6a970098df76e8728e605c`。

下表列出全部 8 组 TUI 工作负载。RSS/PSS 是启动到退出期间以 5 ms 间隔轮询进程树得到的采样峰值；idle 列报告工作流的单核 CPU ≤1% 门槛是否通过。

注入 Host 的输入大小表示注入的 SafeDocument 文本；TLS IMAP 的输入大小表示完整原始 EML。

| 路径 | 邮箱数 | 输入大小 | 帮助键 p95 ms | 编辑 p95 ms | 阅读 ms | 峰值 RSS / PSS KiB | idle ≤1% |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 注入 Host | 1,000 | 32 KiB | 0.46 | 0.31 | 21.98 | 8,820 / 6,673 | 通过 |
| 注入 Host | 1,000 | 2 MiB | 0.46 | 2.66 | 25.29 | 18,176 / 16,054 | 通过 |
| 注入 Host | 10,000 | 32 KiB | 0.47 | 0.42 | 22.17 | 10,252 / 8,145 | 通过 |
| 注入 Host | 10,000 | 2 MiB | 0.35 | 2.58 | 24.83 | 19,608 / 17,473 | 通过 |
| TLS IMAP | 1,000 | 32 KiB | 0.47 | 0.32 | 183.98 | 19,068 / 13,357 | 通过 |
| TLS IMAP | 1,000 | 2 MiB | 0.44 | 2.68 | 205.33 | 34,192 / 26,874 | 通过 |
| TLS IMAP | 10,000 | 32 KiB | 0.41 | 0.38 | 184.33 | 19,360 / 12,296 | 通过 |
| TLS IMAP | 10,000 | 2 MiB | 0.43 | 2.19 | 205.08 | 33,172 / 25,922 | 通过 |

8 组的输入门槛均为 p95 ≤100 ms 且通过；最大编辑 p95 为 2.68 ms，最大采样 RSS 为 34,192 KiB。JSON 中两秒 idle CPU 样本均显示 `0.0%`，但受内核计时粒度量化影响；这不表示 CPU 消耗为零，也不把短样本外推为长期空闲保证。

| 活跃 MCP stdio + TLS IMAP | 邮箱数 | 原文大小 | 初始化 ms | 列表 50 项 ms | 搜索 50 项 ms | 阅读 ms | 峰值 RSS / PSS KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 合成服务 | 1,000 | 32 KiB | 3.83 | 755.52 | 754.81 | 181.53 | 20,968 / 13,541 |
| 合成服务 | 1,000 | 2 MiB | 4.01 | 753.80 | 789.09 | 200.21 | 31,708 / 24,378 |
| 合成服务 | 10,000 | 32 KiB | 3.71 | 757.17 | 757.07 | 181.40 | 21,440 / 14,176 |
| 合成服务 | 10,000 | 2 MiB | 3.83 | 758.43 | 757.43 | 199.53 | 33,304 / 26,012 |

MCP 时延来自受控本地协议服务与 SDK stdio 路径，不代表真实服务商或公开 Codex 客户端延迟。2 MiB 在 TLS 路径表示完整原始 EML；安全文档显示另受 64 Ki 字符预算限制。所有内存数都是采样值，短于轮询间隔的峰值可能遗漏；共享 runner、温缓存和测试采样开销也会影响时延。

此次 release/performance workflow 仅证明上述受控构建与负载。它不构成真实 Gmail、系统凭据库、冷缓存或长期性能验收；也不证明主分支 `ci-gate` 已完成 Codex readiness 验收。
