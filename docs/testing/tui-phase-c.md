# 阶段 C 人工 TUI

日期 2026-10-13；合成数据与受控服务。真实账户尚未验收。

TDD RED：`cargo test -p letterkeeper-adapters --test phase_c --offline`，新增公共状态机 API 尚不存在，预期为编译 RED（不是断言失败）。验证编辑 q、不执行粘贴、危险字符原子拒绝、完整审阅尾部与线程头。实际错误和 GREEN 结果将在实现后记录。

终端恢复依靠 Crossterm 保存进入 raw mode 前的 termios，并由 RAII 在所有返回路径调用 disable_raw_mode；alternate screen 与 bracketed paste 同一 guard 管理。退出不等待网络线程；已接收的副作用由后台执行并持久化，退出不表示取消发送。

实际 RED：编译报 E0432（`WorkspaceState`、`review_text` 不存在），提交 `5389663`。GREEN：`cargo test -p letterkeeper-adapters --test phase_c --offline`，12 项通过。包含成功/取消附件对话框、冲突另存新草稿、人工批准与发送分离、过期只读结果丢弃与 Uncertain 副作用保留、保存期间新编辑保留、网络阻塞时本地保存独立推进、120x36/80x24/40x12/12x5 四尺寸中文/组合字符和 18k 中文审阅尾部可达。初始5项API RED；后续7项针对已实现状态机增加回归覆盖，未声称全部先行为RED。

公开测试接口：`WorkspaceState::new`、`event`、`request`、`take_request`、`complete`、`render_workspace`；`WorkspaceWorker::poll` 驱动两个容量1后台通道。只读结果按会话 generation 过滤；修改操作的回执不因返回/退出而丢弃。保存期间的新输入保留，回执更新持久化版本并维持 dirty；附件返回更新版本/附件但保留后续输入。

编辑：To/Cc/Bcc/Subject/Body，Tab/Shift+Tab切换，小尺寸仅显示当前字段。q 是普通字符；Ctrl+S 保存；Esc/ Ctrl+Q 对未保存内容要求 s 保存/d 放弃/c 继续。冲突 r 重新加载/n另存/c继续。F2添加路径和类型，F3按编号删除，阅读 x按MIME part及目录导出；文件访问仅 HumanWorkspace。F4显式同步；提交列表 Enter状态/e证据/u恢复/v同步状态。F6完整审阅，PgUp/PgDn/Home/End可达尾部；F8输入整个摘要批准300秒，F9单独发送。无自动发送或不确定结果重试。

终端 guard 在 raw mode 成功后立即安装，alternate screen及 bracketed paste失败也由RAII恢复。Crossterm 0.28.1 Unix `TERMINAL_MODE_PRIOR_RAW_MODE` 保留原Termios，disable_raw_mode恢复整个状态。完整审阅以可逆 `\\` 和 `\u{xxxx}` 转义，不使用有损 inert_text，不截断正文；按显示宽度重排并缓存，仅尺寸变化重排。读取视图继续使用保守正文输出。
