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

文档与计划骨架已建立；业务代码尚未开始。  
唯一推荐入口：阅读 `docs/plans/current.md` 与 `docs/tasks/task-001.md`。用户说「开始开发」后再执行 task。

## 产品边界（默认）

**做：**

- 双机 1 对 1 配对与会话
- 文本剪贴板自动同步（MVP）
- 后续：图片、文件元数据 + 粘贴/发送时高速传输
- Linux + Windows 后台服务；可后续加托盘/Tauri UI

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
