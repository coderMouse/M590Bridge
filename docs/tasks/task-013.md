# task-013 · Windows m590-ui 构建并联调确认

## 状态

`completed`

## 目标

确认 Windows 上可构建/运行 `m590-ui`（托盘 + 内嵌 hub），并与 Linux 端完成联调。

## 背景

- task-009 在 Linux 完成 Tauri 壳；文档曾标注 Windows 构建/托盘为待补
- 计划「下一步」原第 1 项为 Windows `m590-ui`
- 用户实机反馈：Windows 已构建并联调通过

## 允许修改

- 仅文档：`docs/plans/current.md`、本 task、相关 discovery / 历史 task 备注

## 禁止修改

- 业务代码（本任务为状态确认，非功能开发）

## 验证

- 用户实机：Windows 构建 `m590-ui` 成功
- 用户实机：与对端联调通过（文本同步 MVP 路径）

> 本环境未重复跑 Windows 构建命令；以用户实机结果为完成依据。

## 完成标准

- [x] 计划中「Windows m590-ui」从下一步移入已完成
- [x] 记录验证来源与后续推荐项

## 实施记录

### 修改文件

- `docs/plans/current.md`
- `docs/tasks/task-013.md`（本文件）
- `docs/tasks/task-009.md`（待补项 closure）
- `docs/discovery/open-questions.md`（Q2）
- `docs/discovery/project-map.md`
- `docs/discovery/commands.md`

### 验证结果

- Windows：`m590-ui` 已构建
- Windows：联调通过（用户确认）

### 文档影响

- 已更新：plan、本 task、task-009 备注、discovery
- 无需更新：协议草案、ui-spec 主体
- 待补：安装包 / 开机自启具体步骤（后续 task）

### 风险

- 未在本 Agent 环境复跑 Windows 构建日志；若需审计可补 CI 或保存构建命令输出

### 下一步

- 新建并执行 V2 相关 task（图片/文件通道），或 mDNS / 安装包
