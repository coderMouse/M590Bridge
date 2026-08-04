# 文档规则 · M590Bridge

## Source of truth

| 类型 | 位置 |
|------|------|
| Agent 规则 | `AGENTS.md`, `CLAUDE.md`, `CODEX.md` |
| 工作流与策略 | `docs/agent/` |
| 当前计划 | `docs/plans/current.md` |
| 任务 | `docs/tasks/task-XXX.md` |
| UI | `docs/ui-spec.md` |
| 产品说明 | `项目说明.md` |
| 摸底 | `docs/discovery/` |
| 本机私有 | `.agent/local-environment.md`（不提交） |

## 何时更新文档

| 变更 | 更新 |
|------|------|
| 完成/阻塞 task | 该 task + 必要时 `plans/current.md` |
| 产品边界变化 | `项目说明.md` / `AGENTS.md` |
| UI 约定变化 | `docs/ui-spec.md` |
| 新模块/命令 | `docs/discovery/project-map.md` / `commands.md` |
| 仅内部重构且行为不变 | 可只写 task 记录 |

## 写法要求

- 用中文为主；代码标识符保持原文
- 不确定写 `Unknown` 或列入 open questions，不编造
- 需要举例连接信息时脱敏：`[REDACTED]`
- 不建重复总结文件（`final-report.md`、`codex-notes.md` 等），除非用户明确要求
- 任务完成后做「文档影响检查」：已更新 / 无需更新 / 待补

## 文档影响检查模板

```text
文档影响：
- 已更新：...
- 无需更新：...
- 待补：...
```
