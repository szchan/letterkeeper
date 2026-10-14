# Account onboarding CLI / doctor 验证

2026-10-14。配置向导只创建配置；保存配置不代表凭据已保存、已认证或连接成功。真实 Gmail 与真实 Secret Service 往返仍未验收。需求对应 AUTH-01 至 AUTH-04、DATA-01/02、MCP-04/05、UX-01/03。

## RED

- 精确 SHA：`0454bec535114542c35276ce766aef3ce37caa50`。
- [TDD RED run](https://github.com/szchan/letterkeeper/actions/runs/38579620252)，Ubuntu 24.04 / Rust 1.99；真实 controlling PTY、虚构 `.test` 地址，24×12 终端。
- `cargo test --locked -p letterkeeper account_add_interactive_save_is_configuration_only -- --exact --nocapture`；inventory 匹配一项，checkout SHA 与请求一致。
- 可编译 stub 返回取消，行为断言 `confirmed interactive wizard did not save configuration` 失败；原始退出码 101，Actions conclusion failure。没有吞掉失败或把编译失败当行为 RED。

## 实现与测试边界

`account add` 在任何目录或数据库初始化前要求 stdin/stdout 均为终端，不解释全局 `--account`。每行最多 1024 字节；超限、控制字符、双向覆盖/隔离控制字符拒绝并结束向导，不能将剩余输入当确认。每步 `/cancel`、EOF、未完整结束的行取消；最终只接受精确 `yes`，缺省取消。完整配置先经共享 builder 校验，再以 JSON 展示，无秘密和用户文件路径。用户选择自己的 Desktop client ID，可选 client secret 引用；秘密另用隐藏输入命令保存。

默认只读，`draft` 和 `send` 是显式枚举选择；发送仍需已有独立人工审阅、批准和单独发送。新账户使用 create-only 存储；重复账户拒绝，不覆盖既有配置。保存结果仅报告 `configuration_saved=true`、`authentication=unverified`，提供 `account password` 或 `account client-secret` / `account login`，然后 `--account ID tui --read-only`。自定义目录参数需在后续命令重复使用。

`doctor` 在全局账户初始化前直接运行只读诊断。报告固定状态与建议，不读取账户、邮件或凭据条目、不创建配置/数据/runtime 目录。环境未就绪时输出 JSON 后返回非零；`authentication=not_checked` 不代表账户认证结果。

Rust 集成测试内启动 Python 的真实 controlling PTY：

- 24×12 与 80×12 的键盘逐行配置、中文宽字符文件夹、终端设置保持。
- 密码账户默认只读、Gmail 固定 TLS 端点和自己的合成 client ID、显式发送范围、secret 引用与后续命令。
- 每一个提示的 EOF 与 `/cancel`、默认否定和 `y`/`YES` 拒绝、不创建目录；保持普通测试 stdin 打开，避免 EOF 假通过。
- 重复账户不覆盖、无 SQLite 初始化；stdin/stdout 分别是管道时拒绝。
- doctor 带不存在的全局账户及合成路径 canary，取消 HOME/XDG 配置环境变量但传入绝对目录，仍输出安全 JSON、不回显路径或账户 ID、不创建目录；断言真实 worker 沙箱就绪。
- 显式双目录不依赖 HOME 的 paths helper 边界由静态审查发现后加入 GREEN 回归，未取得单独 RED；以上向导行为 RED 不被描述为已覆盖这一新增边界。
- 内存输入测试覆盖超长行、未结束行和终端控制字符，不能推进到确认。

## GREEN / 集成

待记录精确 GREEN SHA、TDD run 与完整 CI 门禁。仅运行 rustfmt 与静态审查；本机未执行 Cargo 编译、check、测试或 Clippy。任务分支通过不替代主代理集成后新 SHA 的完整门禁。
