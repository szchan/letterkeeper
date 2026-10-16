# 项目结构整理记录

基线：`3cf6b01697a3dd31adbeaccb2a9cfdae27e5d279`，[完整远程 CI](https://github.com/szchan/letterkeeper/actions/runs/38736285052) success。开始前工作目录干净，现有 worktree 保留。已登记 272 项 Rust 测试（包括 3 个基准辅助入口，不把 helper 数量当成产品覆盖）。

纯模块移动复用完整回归；共享测试设施及正式基准入口先提交调用方测试，预期 RED 为缺失模块和开发依赖导致的编译失败，不能称为业务断言失败。新增检查覆盖显式路径、拒绝遗漏/重复/未知参数、带空格路径、子进程失败传播，以及共享受控 TLS 场景的实际往返和无 STORE。全部 Rust 执行在 GitHub worker。
