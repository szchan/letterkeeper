# ADR 0005：账户接入引导与环境诊断

日期：2026-10-14。状态：已实施，代码完整远程集成通过；最终 revision 仍遵守 PR 门禁。基线 `d951c846212c12ff95354931dee61b69c879fdce`。对应 AUTH-02、DATA-01、DATA-02、MCP-04 与现有正文隔离要求；不改变真实账户的独立授权关卡。

本批增加人类 CLI 的 `doctor` 和 `account add`。继续复用 accounts 的配置校验、私有存储与既有登录入口；不增加 MCP 配置、凭据或批准工具，不增加自动发送能力。

`account add` 在检查交互终端后收集有界文本，展示配置和权限，要求明确确认后才初始化配置存储。取消、EOF 和非终端调用不写配置。新账户默认只读，草稿或发送范围必须显式选择；允许发送仍需既有的内容绑定人工批准。创建使用原子的不可覆盖操作，同名账户和并发创建不能覆盖已有配置。密码账户只支持现有隐式 TLS，Gmail 固定端点并使用用户自己的 Desktop OAuth client ID；引导不收集密码或 token，不启动登录或连接邮箱。保存结果明确为配置已保存、认证未验证。

`doctor` 不调用会创建目录的 AccountRepository 构造函数。逐级以不跟随符号链接的描述符检查配置、数据与固定用户运行目录，区分未初始化、不安全和可用。沙箱运行合成邮件的实际 worker，失败时报告阻塞；这项检查验证启动和解析，完整 HTTP/DNS/宿主文件隔离仍由远程隐私集成测试证明。

凭据诊断只查询 Secret Service 的默认 collection 及 Locked 元数据，不搜索条目、不读取凭据、不解锁、不创建 collection，也不认证账户。使用固定 `/usr/bin/busctl`、参数数组和禁服务自动启动的调用；只允许本地 Unix D-Bus 地址，清理环境、限制输出并设置总时限。缺少 busctl 或无法查询时报告不可用/未知，不声称已登录。该可选系统工具不引入 Cargo 依赖；本批不为实现诊断安装或修改用户系统服务。

ReadAlias 返回路径按有界的 D-Bus ObjectPath 语法校验，允许合法服务商自定义层级；目标服务、接口及 Locked 属性固定，路径不能改变调用权限。初版固定 collection 前缀的限制在集成审查中由单独 RED/GREEN 回归修正。参考 [Secret Service 规范](https://specifications.freedesktop.org/secret-service/latest-single/)及 [systemd busctl 手册源码](https://github.com/systemd/systemd/blob/main/man/busctl.xml)。

诊断只输出固定状态和操作建议，不包含真实路径、邮箱、D-Bus 地址或底层原始错误。交互配置审阅属于本机人类界面，输入不能被当作终端控制指令；公开测试全部使用合成身份和临时目录。

主代理负责接口、集成与隐私审查；诊断及配置存储用 GPT-6.1-Sol high，CLI/PTY 用 GPT-6.1-Sol medium。三个任务使用各自 worktree 和分支，按 ADR 0004 在 GitHub hosted runner 执行 RED/GREEN、完整 Rust/PTY/隐私/MCP 门禁。本机不编译。过程和精确验收证据分别记录在 docs/development 与 docs/testing。
