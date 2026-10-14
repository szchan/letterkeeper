# Hosted PTY 缩放与帮助验证

日期：2026-10-14。对应 UX-01、UX-02；实际应用的远程 GREEN 与集成门禁待验证，不替代真实账户验收。

## 远程失败证据

[Actions run 38536633653](https://github.com/szchan/letterkeeper/actions/runs/38536633653)，被测 SHA `79b447b2cc5d72818cc75a8b14d533c36f26489b` 的 `workspace` 在 `python3 scripts/verify_terminal.py target/debug/letterkeeper` 失败。应用仍运行，但 80×24 缩放至 24×12 后立即输入 `?`，十秒内未观察到“帮助”。捕获的输出是一帧 24×12 的安全正文，而非帮助页；不把此前 Rust 测试、fmt、Clippy 的通过当作这项 PTY 验收通过。

脚本使用 `start_new_session=True`，却没有为新会话获取控制终端；此外，发送 SIGWINCH 后紧接按键，没有等到实际尺寸变化的帧。两项属于验证夹具缺口，尚不能仅据日志断言它们是远程失败的唯一原因。

## 修复与保持的断言

启动时由轻量 Python 子进程在 stdin 的 PTY 上执行 `TIOCSCTTY`，然后以参数数组 `execv` 启动应用。会话创建仍由 `Popen(start_new_session=True)` 完成，不使用 `preexec_fn`。设置 PTY 尺寸后由内核向前台进程组投递 SIGWINCH，不再额外手动重复发信号。

输入 `?` 前，脚本必须观察 Ratatui/Crossterm 输出中的清屏、新的第 12 行定位和帧末隐藏光标，且该帧包含 LetterKeeper 标记。这证明新尺寸已经渲染，不以固定等待时间或重发按键掩盖事件丢失。十秒超时不变；缺少缩放帧或帮助仍失败。中文与宽字符、j/Enter 导航、帮助、q 退出、终端设置恢复和完整摘要人工批准断言均保留。

## 验证边界

本机仅对脚本做 AST 语法检查、`git diff --check` 与轻量 Python 夹具检查：合成子进程成功打开 `/dev/tty`，确认自己是控制终端前台会话/进程组；PTY 缩放自动触发 SIGWINCH；完整的新尺寸帧被接受，缺清屏、错误末行和未完成帧被拒绝。夹具不是 LetterKeeper，不验证应用帮助或批准行为。本机没有执行 Cargo、LetterKeeper 或实际应用 PTY 验收。

GREEN 必须由远程 runner 执行原始 `python3 scripts/verify_terminal.py target/debug/letterkeeper`；主代理集成产生的新 SHA 仍须运行完整 CI。待完成后记录精确 SHA、run URL 和结论；失败时继续修复，不删除帮助、缩放、批准或隐私门禁。

## 四尺寸工作区夹具的后续 RED

[父分支 run 38538057660](https://github.com/szchan/letterkeeper/actions/runs/38538057660)（被测 SHA `a449a77270c2c0c7736fdf23995726eee028c602`）和 [子分支 run 38538084342](https://github.com/szchan/letterkeeper/actions/runs/38538084342) 未通过。父分支 Codex 项成功，但 `workspace_four_sizes_editing_q_and_terminal_restore` 在缩放后立即输入 Esc，未观察到“未保存”，捕获 1016 字节。这是另一个实际 PTY 失败，不把前一脚本修复写成完整终端 GREEN。

Rust 四尺寸测试已有控制终端；本次增加两处可观察同步：先看到编辑字段中的 `q`，确认编辑事件已处理，再缩放；缩放后看到清屏、新尺寸末行及帧末隐藏光标，再输入 Esc。移除重复的手工 SIGWINCH，保留原八秒输出超时、未保存提示、丢弃退出及 termios 恢复检查。`scripts/verify_workspace.py` 同样补上控制终端和上述同步，四个尺寸与全部断言保留。

只读检查本机 crossterm 0.28.1 的 Unix mio 事件源发现，它会在处理一个 readiness token 得到按键或 resize 时立即返回。这个实现说明同时输入/缩放值得单独验证，但不足以确认所有丢键原因；本次仅修复夹具调度，不声称应用的同时缩放与按键行为已修复。若同步后仍失败，必须继续调查应用事件处理。

本机新增验证仅为 Python 语法与四种目标高度（39/23/11/5）的完整帧接受、缺清屏/未完成帧拒绝夹具检查，以及独立 rustfmt 格式化；未编译或运行 Rust，未启动 LetterKeeper。实际 Rust 测试和两个 Python PTY 验证的 GREEN 仍等待对应新 SHA 的远程完整 CI。
