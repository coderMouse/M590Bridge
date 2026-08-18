# AGENTS.md · M590Bridge

## 项目是什么

M590Bridge 是局域网双机剪贴板与文件桥：在 A 电脑复制，用罗技多设备鼠标切到 B 电脑后直接粘贴。  
首期平台：**Linux（Ubuntu）+ Windows 10**。技术核心：**Rust**。  
**暂不考虑 Android** 与其它移动端。

## 任意 Agent 开工前必读（按顺序）

1. 本文件 `AGENTS.md`
2. `CLAUDE.md` / `CODEX.md`（若存在）
3. `docs/agent/workflow.md`
4. `docs/agent/documentation-rules.md`
5. `docs/agent/environment-policy.md`
6. `docs/plans/current.md`
7. 当前任务 `docs/tasks/task-XXX.md`
8. 相关规格：`docs/ui-spec.md`、`项目说明.md`

## 一句话规则

先读项目规则与当前 task → 一次只做一个 task → 真实验证 → 更新 task/计划/文档影响 → 不把本机路径和密钥写入共享文档。

## 当前阶段

文本 + 图片 + 单文件流式已在 Linux↔Windows 实机可用（见 `docs/plans/current.md`）。
**V2 图片/文件、mDNS、Linux `.deb`/登录自启已完成**；Windows NSIS、登录自启、卸载清理与跨机回归已在真机通过（task-042）；task-043 至 task-054 已完成真机验收（无文件夹/断点续传）。
原 task-019A（收图落盘捷径）已 **cancelled**。  
用户已确认 task-042 的登录自启、卸载清理与跨机回归通过；task-043 至 task-054 已完成。task-052 已验证 Linux FUSE 单文件在 GNOME Wayland Nautilus 粘贴时通过网络惰性读取、显示系统进度且内容一致；task-053 已修复 Linux 托盘菜单文字回归；task-054 的 Linux `.deb`、Windows NSIS 一键打包与 Windows 成功/异常按键暂停均已验收。task-055 已完成多文件批次清单与路径安全协议基础；task-056 的手动多选/文件夹/拖放、Hub 串行批次、双层进度及 Linux/Windows 跨机交互均已验收。task-057 首轮 Windows 10 真机发现只能粘贴一个文件且目录失败，OLE `CFSTR_FILECONTENTS` 通配枚举已返工并通过本地测试/Windows 交叉检查，等待复测。不要宣称 task-057 已完成，也不要宣称 Linux Nautilus 已可直接粘贴多文件/文件夹；不复活 019A。

## 产品边界（默认）

**做：**

- 双机 1 对 1 配对与会话
- 文本 + 图片剪贴板双向同步（位图；Word 等可粘贴）
- 后续：文件语义（桌面粘贴）/ 文件元数据 + 按需传输
- Linux + Windows：`m590-ui` 托盘 + 内嵌 hub（已有）

**不做（当前）：**

- Android / iOS
- 云账号、公网中继（默认仅局域网）
- 远程键鼠控制（用户已有 M590 硬件切机）
- 1 对多设备网格
- 依赖 Logitech 专有 SDK 读切机按键（无公开可用事件）

## 技术方向（已定倾向）

| 项 | 选择 |
|----|------|
| 语言 | Rust |
| 桌面 UI（后置） | 倾向 Tauri 2 + 托盘；MVP 可先 CLI/daemon |
| 传输 | 局域网 TCP，后续可评估 QUIC |
| 发现 | 先手动 IP + 配对码；后 mDNS |
| 剪贴板 | 分平台抽象；Linux 注意 Wayland |

协议与模块划分应**预留多设备字段**，但产品首期只实现 1 对 1；Android 暂不做实现。

## 仓库约定

- 文档 source of truth：`docs/`
- 本机环境：`.agent/local-environment.md`（gitignore）
- 不提交密钥、token、私钥、完整私有连接串
- 不顺手大重构；不扩 task 范围

## 验证原则

- 禁止用「应该可以」代替验证
- 有代码后：至少运行 task 写明的命令并记录真实输出摘要
- 跨平台功能：在任务允许范围内验证；无法在本机测 Windows 时，在 task 记录 blocker 与复现步骤
