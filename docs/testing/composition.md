<!-- SPDX-License-Identifier: GPL-3.0-only -->
# 安全回复与附件验证

2026-10-13；合成邮件，未使用真实账户或网络。

RED：`cargo test --offline --locked -p letterkeeper-content --test composition`，六个行为测试均失败；可编译 stub 返回 InvalidMessage，成功路径和附件上限断言失败。覆盖 Reply-To、全部回复去重/去自身/Bcc、中文、安全引用、回复线程、附件显式 ID、二进制和 quoted-printable 解码、文件名和大小限制。

GREEN：`cargo test --offline --locked -p letterkeeper-content`，12 个新增测试和原有 24 个测试通过；`cargo clippy --offline --locked -p letterkeeper-content --all-targets -- -D warnings` 通过。追加测试曾发现未知 Content-Transfer-Encoding（gzip）被误当作原始字节；先执行失败断言，再拒绝未知/重复编码头，重新通过。

接口：`inspect_message` 返回独立安全正文和 ReplyMetadata；`compose` 返回 Reply/ReplyAll/Forward 草稿，不产生批准或发送；`extract_attachment` 仅解码明确指定的、已列出的 MIME part ID。保留 SafeDocument 字段及已有 parse_message 行为。无新增依赖。

Reply-To 优先于 From；全部回复以 ASCII 不区分大小写去重并去除身份地址，保留 To/Cc 分组，不复制 Bcc。地址限每字段 64 个、输出总收件人最多 100。Unicode 主题、正文保留，移除既有危险控制字符与 bidi；主题单行、最多 990 字节，草稿正文最多 1 MiB；引用标记为不可信，截断正文显示显式说明。转发不带线程头或自动复制附件，收件人留空供明确编辑。

线程字段采用保守 ASCII dot-atom mailbox 形式的 `<local@domain>`，每 ID 最多 256 字节；拒绝混入的文字、控制字符、重复顶层头、非法折行。合法折行仅可出现在 ID 之间。最多 32 个 References，超出保留根及最新尾部；新回复附加当前 Message-ID。带引号本地部分、域字面值、SMTPUTF8 和复杂注释暂不支持，拒绝而非猜测。

附件原文沿既有默认 2 MiB、part/depth/字符预算；解码字节最多 512 KiB，使最坏 Vec<u8> JSON 字节输出小于应用 4 MiB worker 上限。使用 mail-parser transport 解码，不做文本字符集转换，保存原始字节。文件名移除控制字符后再次取 basename，`.`/`..`/空白变为 unnamed，UTF-8 边界最多 255 字节；MIME token 最多 127 字节，非法类型回退 application/octet-stream。未知/重复传输编码或解码异常拒绝。嵌套 multipart 附件保持已有 part ID；message/rfc822 附件目前拒绝提取，不遍历其内部附件。压缩包只是字节，不解压、不预览、不执行、不写盘。

这些是离线库测试，并不能独立证明进程网络/宿主文件隔离。应用必须仅在既有 bubblewrap worker 内调用原文 inspect/extract；集成代理另行验证 worker HTTP/DNS、输出传输、MCP/CLI 输出与保存权限。引用文本仍含正文 URL，消费端只能当纯文本，不能启用资源预览；本次没有进行真实邮箱、OAuth、远程收发或下游渲染验收。
