# 原型性能样本

2026-10-12。本节是初步实测，不是最终性能预算或真实邮箱性能承诺。

环境：Arch x86_64，Linux 7.2.5-3-omarchy，Intel Core i7-10710U，约 15.3 GiB 内存；Rust/Cargo 1.99.0，debug 构建，bubblewrap 0.12.0。负载为 `tests/fixtures/welcome.eml`（1089 bytes，SHA-256 `e5bb7a69e48ce29aaa903dd25743a53467879d265a7bc7904008c4c753795c91`），先预热再运行 20 次。

重现：

```sh
cargo build --locked
python3 scripts/benchmark.py target/debug/letterkeeper
```

| 指标 | 本次样本 |
| --- | --- |
| 启动 CLI + 隔离解析 + 退出，median | 14.49 ms |
| 同一流程 p95 | 26.80 ms |
| 进程树采样峰值 RSS（含 worker/bwrap） | 22024 KiB |
| 进程树采样峰值 PSS | 11600 KiB |
| 20 次子进程累计 CPU 时间 | 0.113 s |
| MCP 已预载一封邮件、等待握手的 RSS / PSS | 13456 / 10864 KiB |

脚本通过 `/proc` 递归采样应用和子进程，读取阶段约 1 ms 一次，可能遗漏瞬时峰值；进程枚举存在退出竞争。RSS 会重复计算共享页，PSS 用于辅助判断。温缓存不等于冷启动，debug 不等于发行构建，等待握手不等于活跃 MCP 会话。尚未实现搜索，故没有搜索指标；未宣称 PERF-02/03 全面完成。

终端功能脚本验证真实 PTY 字符输出、键盘和尺寸变化，不测字体质量或输入延迟。下一阶段需固定大规模邮件语料和硬件，增加冷缓存、活跃 MCP、搜索、CPU 空闲与交互延迟预算。

## 阶段 B 同负载样本（2026-10-13）

同一机器、debug、同一 1089-byte fixture/SHA-256、温缓存、20 次，使用相同脚本：

| 指标 | 阶段 A | 阶段 B |
| --- | ---: | ---: |
| CLI + worker + 退出 median | 14.49 ms | 29.97 ms |
| p95 | 26.80 ms | 31.28 ms |
| 进程树采样峰值 RSS | 22024 KiB | 29684 KiB |
| 进程树采样峰值 PSS | 11600 KiB | 16233 KiB |
| 20 次子进程累计 CPU | 0.113 s | 0.233 s |
| MCP 预加载等待握手 RSS/PSS | 13456 / 10864 KiB | 20068 / 17476 KiB |

此样本显示原型变大，不以二进制体积或单次结果掩盖增加。没有测真实服务、搜索大邮箱、活跃 OAuth/MCP 或最小 Rust 版本；MCP 的上述数据仍是离线演示等待握手。采样、缓存和竞争限制与前节相同，不把增长因果单独归给某个依赖；后续需 release、固定邮箱规模/冷缓存及正式预算。
