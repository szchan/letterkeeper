<!-- SPDX-License-Identifier: GPL-3.0-only -->
# 安全回复与附件验证

2026-10-13；合成邮件，未使用真实账户或网络。

RED：`cargo test --offline --locked -p letterkeeper-content --test composition`，六个行为测试均失败；可编译 stub 返回 InvalidMessage，成功路径和附件上限断言失败。覆盖 Reply-To、全部回复去重/去自身/Bcc、中文、安全引用、回复线程、附件显式 ID、二进制和 quoted-printable 解码、文件名和大小限制。
