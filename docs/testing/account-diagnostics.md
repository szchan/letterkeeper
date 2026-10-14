# 只读账户环境诊断

需求：AUTH-03、DATA-01、DATA-02、PRIV-03。诊断不读取配置正文或凭据条目，
不创建配置、数据或 runtime 目录，输出只包含固定状态和建议。

先提交可编译 stub 与目录状态回归，精确 SHA 的远程 RED 待运行。
本机不执行 Rust build/check/test/Clippy。真实 Gmail 和系统凭据库独立验收未执行。
