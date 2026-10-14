# 只读账户环境诊断

需求：AUTH-03、DATA-01、DATA-02、PRIV-03。诊断不读取配置正文或凭据条目，
不创建配置、数据或 runtime 目录，输出只包含固定状态和建议。

## 接口与边界

`letterkeeper::diagnostics::diagnose(config_dir, data_dir)` 返回固定字段
`DoctorReport`：三个目录状态、worker 状态、凭据服务状态、
`authentication=not_checked`、`environment_ready`、建议代码和固定提示。
不读取账户配置正文，不判断账户已登录，不输出路径、邮箱地址、DBus 地址或原始异常。

目录以 descriptor 从 `/` 逐级 `openat(NO FOLLOW)` 检查，最终目录要求当前 UID、
0700；父目录不要求 0700（例如 `/tmp`），但不允许符号链接、相对路径、`..`。
缺失目录报告 `not_initialized`；权限、类型或 owner 错误报告 `unsafe`。
runtime 固定检查 `/run/user/UID`，不信任 XDG runtime 覆盖，不创建任何目录。
检查不枚举目录、不读取用户内容；沙箱自身仍使用现有临时 marker。

worker 状态必须通过实际 `Sandbox::parse` 合成邮件。`ready` 只表示隔离
worker 可运行和返回预期安全文本，并非 doctor 单独证明全部 HTTP/DNS/宿主文件隔离；
这些仍由完整 CI 的沙箱和正文隐私测试证明。

凭据服务诊断使用固定 `/usr/bin/busctl`、参数数组、`env_clear`，只保留固定语言、
固定 UID runtime 和已验证的本地 `unix:path`/`unix:abstract` DBus 地址。
不允许 TCP、exec、autolaunch、多地址、任意地址参数；不支持的地址编码报告未知。
两次调用共享 3 秒总 deadline（不是每次各 3 秒）；stdout 上限 4096 字节，
非阻塞读取、失败/超限/超时终止并回收子进程，不等待无界 reader 线程。
没有继承 HOME、PATH、预加载库变量、token 或其他用户环境。

调用仅为 `ReadAlias(default)` 和固定 `Properties.Get(Collection.Locked)`，
两次都禁用自动启动和交互授权。Collection 返回路径只允许固定命名空间下单个
DBus 路径组件；`/` 报告 `not_initialized`。不打开 Secret Service session，
不搜索、枚举或读取任何 item，不解锁、创建、删除、修改或弹出 prompt。
服务 locked、unlocked、missing collection、unavailable、unknown 和 timed_out 分开。
只有所有环境项成功时 `environment_ready=true`，仍为 `authentication=not_checked`。

根据 [Secret Service 规范](https://specifications.freedesktop.org/secret-service/latest-single/)，
ReadAlias 无结果返回 `/`，Locked 是无需会话的集合元数据。
根据 [systemd busctl call 实现](https://github.com/systemd/systemd/blob/main/src/busctl/busctl.c)，
`call` 会设置 no-auto-start/no-interactive 标志；直接 `get-property` 使用另一条
调用路径，因此这里使用显式 `call Properties.Get`。
[systemd JSON 实现](https://github.com/systemd/systemd/blob/main/src/libsystemd/sd-bus/bus-dump-json.c)
确认 variant 的 bool 是标量，外层 message data 是数组；解析严格检查 signature、
字段与基数，不接受任意字符串 bool 或额外字段。

## 远程 TDD

RED commit `5d9be0f957c7dda1b25d07a14e0a634185a8a620`：
[真实失败运行](https://github.com/szchan/letterkeeper/actions/runs/38579536219)。
`letterkeeper-accounts` 精确测试
`diagnostics::tests::directory_diagnostic_is_read_only_and_distinguishes_missing_unsafe_ready`，
匹配 1 项、raw exit 101，编译成功；缺失目录断言实际 `Unsafe`、预期 `NotInitialized`。

GREEN 和完整 CI 待运行。本机只执行 rustfmt 和静态检查，没有 Rust build/check/test/Clippy。
测试覆盖只读目录、owner/权限/类型、符号链接父目录、missing 与 lexical 边界、
固定安全输出、实际 sandbox 正常/失败，以及受控 executable shim 的调用 argv、
清空环境、locked/unlocked/missing/error/malformed/超量输出/统一 deadline。
shim 验证受控进程及响应边界，不代表真实凭据库兼容性。
真实 Gmail 和系统凭据库独立验收未执行，不在公共 CI 中测试真实服务。

## 合成进程 fixture 启动回归

CLI 集成 commit `ef181112fcf1a84b5175fac7121fda356fdfe138` 的
[真实 CI 失败](https://github.com/szchan/letterkeeper/actions/runs/38581758074)
显示 metadata 正向 fixture 没有生成调用记录，约 80ms 后失败；其他 metadata
拒绝/超量输出/deadline 测试通过。原日志没有 spawn errno，不能把起因确定为超时
或 ETXTBSY。

新增确定性 RED commit `8a416dfe5152e3586d9efc9de961ff67155a6ef8`：
[RED 运行](https://github.com/szchan/letterkeeper/actions/runs/38582820177)，
精确测试 `diagnostics::tests::metadata_fixture_can_be_read_while_a_script_writer_is_open`，
匹配 1 项、raw exit 101、`test_finished`。保持合成 shim 的写 descriptor 打开时，
直接执行该临时文本报告 `ExecutableFileBusy`、errno 26（ETXTBSY），
返回 Unavailable 且无调用记录。这证明 fixture 的启动方式存在该失败类别，
但不冒充原始间歇失败的完整 errno 证据。

修复只修改 test 构建中的 shim launcher：固定 `/usr/bin/python3` 读取合成脚本，
不直接 exec 临时文本；仍使用同一生产 argv/environment/deadline/output runner。
生产程序保持直接执行固定 `/usr/bin/busctl`，3 秒总预算与所有边界不变。
fixture 预创建调用日志并严格断言至少有一条记录，没有吞掉缺失日志或放宽正向状态。
新增 RED/GREEN 和完整 CI 的后续结果待精确 SHA 验证。
