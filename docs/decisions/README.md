# 架构决定索引

ADR 文件记录决策形成时的范围与状态。较早 ADR 是历史决定，不表示每一项未完成验收后来自动通过；当前实现与产品状态应查 [STATUS](../STATUS.md)。编号重复的三个 0002 文件是历史地址，保持原名以兼容现有引用。

- [历史 ADR 0001：阶段 A](0001-phase-a.md)：首个可验证产品阶段。
- [历史 ADR 0002：账户](0002-accounts.md)：账户与凭据边界。
- [历史 ADR 0002：协议](0002-protocol.md)：邮件协议边界。
- [历史 ADR 0002：阶段 B](0002-phase-b.md)：阶段 B 纵向切片。
- [历史 ADR 0003：阶段 C TUI 与 Agent 审批](0003-phase-c.md)：人类审批边界和工作流。
- [历史 ADR 0004：托管 CI](0004-hosted-ci.md)：远程构建、TDD 与数据边界。
- [历史 ADR 0005：账户引导](0005-account-onboarding.md)：只读诊断与配置创建。
- [历史 ADR 0006：发送诊断](0006-send-diagnostics.md)：发送结果诊断持久化。
- [ADR 0007：workspace、模块与验证目录结构](0007-project-structure.md)：当前结构约定；集成后的代码和 CI 仍需确认。
- [ADR 0008：GitNexus 开发辅助图谱](0008-gitnexus.md)：独立工具、源码快照及只读查询边界。
