# 阶段 C 实际 Codex MCP 客户端验证

2026-10-13。已安装 codex-cli 0.162.1，基线服务仍为阶段 B 的14工具；此验证是现有能力的实际客户端正向控制，不冒充新业务的RED。

使用 [OpenAI Docs 技能](/home/szchan/.codex/skills/.system/openai-docs/SKILL.md) 核对[官方 app-server 文档](https://learn.chatgpt.com/docs/app-server)的 `mcpServerStatus/list` / `mcpServer/tool/call`，再用本机 `generate-json-schema` 的临时输出确认实际版本字段。临时参数遵循[官方配置说明](https://learn.chatgpt.com/docs/config-file/config-reference)。没有修改个人配置、打开真实邮件或启动模型回合。

首次直接 app-server 启动因只读 Codex runtime SQLite 失败。解决方法是在 bubblewrap 中挂载新临时 `.codex` 目录，不重设 HOME/CODEX_HOME，不挂载真实 auth/config/plugins。客户端禁网，只有合成fixture、临时应用数据和必要执行文件；解析另走原 worker。先 config/read 核对只有两个指定测试服务器，然后创建 ephemeral thread，不发 turn/start。

`python3 scripts/verify_codex.py /绝对路径/letterkeeper` 通过：两个已连接 MCP 服务各14工具；实际工具调用能够读取中文合成HTML的保守文本、创建/读取草稿；只读写入、未批准发送、模型confirm参数和账户覆盖都拒绝。输出无活动URL/HTML/终端escape，正文标注untrusted。身份均为example.test，smtp_test使用不可批准的草稿，因此没有SMTP连接。

脚本输出版本与断言结果，不输出个人配置/凭据或邮件正文。临时目录和客户端进程清理后退出。运行需要namespace/bubblewrap权限及已安装Codex，不进入普通cargo单元测试（外部客户端为独立验收命令）。源码采用GPL-3.0-only。

边界：证明的是实际公开Codex app-server MCP协议与文本数据路径，不是所有GUI前端的渲染证明，也不宣称模型能永久抵抗提示注入。外层禁网确保此特定消费路径无外联，应用侧正文无请求仍由独立HTTP/DNS正向控制证明。图形前端及云端模型行为继续列独立缺口；没有通过受控模拟来冒充真实Gmail或Secret Service。
