# 阶段 C 人工 TUI

日期 2026-10-13；合成数据与受控服务。真实账户尚未验收。

TDD RED：`cargo test -p letterkeeper-adapters --test phase_c --offline`，新增公共状态机 API 尚不存在，预期为编译 RED（不是断言失败）。验证编辑 q、不执行粘贴、危险字符原子拒绝、完整审阅尾部与线程头。实际错误和 GREEN 结果将在实现后记录。

终端恢复依靠 Crossterm 保存进入 raw mode 前的 termios，并由 RAII 在所有返回路径调用 disable_raw_mode；alternate screen 与 bracketed paste 同一 guard 管理。退出不等待网络线程；已接收的副作用由后台执行并持久化，退出不表示取消发送。
