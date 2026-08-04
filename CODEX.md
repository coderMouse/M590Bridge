# CODEX.md · M590Bridge

本文件为 Codex 补充规则。通用规则见 `AGENTS.md` 与 `docs/agent/workflow.md`。

## 执行要求

- 先读计划与当前 task，再改代码
- 一次一个 task；用真实验证命令；更新 task 记录
- 默认中文简要汇报：结论 / 改动 / 验证 / 文档 / 风险 / 下一步

## 本项目特别注意

- 技术栈：Rust；目标 Linux + Windows 10
- 暂不实现 Android
- 设计对照：`docs/ui-spec.md`
- 无网络或无法装依赖时：记录 blocker，不伪造「编译通过」

## 与 Claude Code 协作

- 项目状态以 `docs/plans` 与 `docs/tasks` 为准，不以某次对话为准
- 并行时：可并行只读；同一工作区默认串行写
