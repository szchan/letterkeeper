# 阶段 C：展示工作流的 HTTP / DNS 验证

`python3 scripts/verify_privacy_workspace.py /绝对路径/letterkeeper` 已在合成数据上通过。临时 user/network namespace 内启动真实 UDP 53 DNS 服务及 TCP HTTP 观察器；给该测试挂载专用 hosts/nsswitch/resolv 文件，不改宿主配置。DNS 正向控制实际调用 gethostbyname；HTTP 正向控制通过域名建立连接。两者成功后清零观察记录。

随后在同一可联网测试网络中运行实际应用：MCP stdio 创建/读取含 img、dns-prefetch、script/fetch 字样的草稿；TUI 打开编辑器、完整审阅并 End 到超过16KiB正文后的 TAIL_PRIVACY。无新DNS查询或HTTP连接，终端属性恢复。客户端主进程能够连接观察器，不能将这项结果归因于切断了客户端全部网络。测试网络与宿主/外部网络分离；绑定53端口能力仅用于这个临时 namespace 内的DNS夹具。

初始夹具未保留低端口绑定能力，DNS正向控制未启动；补全测试namespace能力后成功。初次把原始EML解析也嵌套进这个受限测试环境时应用返回sandbox_or_parse_failed，故障关闭；本脚本明确只验收MCP草稿与编辑/审阅路径。原始解析、HTTP/DNS主动探针及宿主文件隔离继续由Rust工作区的独立worker测试覆盖，没有做不受限解析降级。

不同于向任意UDP端口人工发一个数据包，此脚本将消费者的实际resolver指向观察器，能够记录正文展示引发的DNS查询。测试仅使用虚构域名、example.test身份和临时草稿。stdout只有汇总；诊断文件来自合成应用并随临时目录清理，未读取真实邮箱、钥匙环或配置。

公开Codex数据路径另见 [实际客户端验证](codex-phase-c.md)。这不承诺所有第三方图形前端的行为，也不证明任何模型永远不受提示注入影响。
