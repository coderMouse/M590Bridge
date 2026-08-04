# CLAUDE.md · M590Bridge

本文件为 Claude Code 补充规则。通用规则见 `AGENTS.md` 与 `docs/agent/workflow.md`。

## 执行要求

- 开发前读取 `docs/plans/current.md` 与当前 `docs/tasks/task-*.md`
- **一次只做一个 task**，不提前实现后续版本功能
- 改动后更新该 task 的实施记录、验证结果、文档影响
- 中文回复；结论基于仓库文件与命令结果，不基于聊天记忆臆造状态

## 本项目特别注意

- MVP 先打通 **文本剪贴板** 双机同步，不做文件传输完整实现（除非 task 明确要求）
- 不实现 Android 客户端
- UI 以 `docs/ui-spec.md` 为设计对照；无 UI task 时不要擅自加 Tauri 大壳
- Linux 剪贴板：区分 X11 / Wayland，不确定就写进 open questions，不硬编码单一路径
- Windows 相关代码可先编写并 `cfg` 隔离；若当前环境无 Win 编译/运行条件，在 task 记 blocker

## 禁止

- 把 `.agent/local-environment.md` 内容抄进共享 docs
- 创建重复总结文件（如 `final-report.md`），除非用户明确要求
- 未授权的依赖大升级或目录大搬迁
