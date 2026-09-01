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
用户已确认 task-042 的登录自启、卸载清理与跨机回归通过；task-043 至 task-054 已完成。task-052 已验证 Linux FUSE 单文件在 GNOME Wayland Nautilus 粘贴时通过网络惰性读取、显示系统进度且内容一致；task-053 已修复 Linux 托盘菜单文字回归；task-054 的 Linux `.deb`、Windows NSIS 一键打包与 Windows 成功/异常按键暂停均已验收。task-055 已完成多文件批次清单与路径安全协议基础；task-056 的手动多选/文件夹/拖放、Hub 串行批次、双层进度及 Linux/Windows 跨机交互均已验收。task-057 的无网络 Windows OLE 集合探针及 Windows 10 真机测试均已通过；发送端多路径/目录批次、GNOME 多行文本与 Wayland 原始 MIME 回退，以及 `arboard` 多路径末尾 `\r` 修复均已完成。task-058 的 Linux FUSE tree、逐文件网络惰性读取、非阻塞背压和生命周期已完成；GNOME Wayland + Nautilus 的大文件批次、目录树、取消、替换、断线及重新发送后再次粘贴均已真机验收通过。task-060 已实现同一 clipboard offer 串行重开（发送源保留、stream offer 保留、bridge/FUSE 重开、批次 entry 状态重置）；Q1/Q2 真机通过，Q3 权限修复（FUSE `0o664`/`0o775`）与 Q4 角标已真机通过；**mp4/pdf 粘贴替换修复：六轮（旧流在途先取消再重开 + 陈旧 chunk 丢弃 + 管道重开门控）真机 mp4/单个 pdf 通过；七轮（忽略旧轮在途 `FileComplete(ok)`，见 task-060）用户已确认通过；八轮（目录+mp4 批次替换：entry 级失败隔离）复测仍有问题，已回滚至七轮版本，日常规避为文件夹与其他文件分开复制（见 task-060）**。task-061 已实现初版（版本 0.1.2）：Windows 收到位图 → OLE 双表示（Word 贴位图 + Explorer 粘贴成 `.png` 图片文件，serve `CF_DIBV5` 与已注册 `PNG`）；Linux 收到单图片文件 offer（可解码扩展名、≤32MiB）→ 自动下载解码写剪贴板位图（gif/bmp 已开解码，tif/tiff 保持 文件粘贴）。代码级验证通过，**待 Windows 10 / GNOME Wayland 真机验收**（见 task-061）。prtsc 后「不能粘贴成文件 + 后续文件复制阻塞」已修：Windows 位图接收改单一 OLE 发布（不再先裸写 `EmptyClipboard` 覆盖 OLE owner），OLE 发布对 `CLIPBRD_E_CANT_OPEN`短暂重试。**「prtsc 位图 → Windows 可粘贴成图片文件、但无法粘贴到 Word」已于 2026-09-01 关闭**：正式构建（不带 `task-057-diagnostics`）真机跑场景 A 通过，诊断非 load-bearing；三轮诊断确认 Word 的位图取自系统剪贴板上的 `CF_DIB`/`CF_DIBV5`/合成 `CF_BITMAP`，不经 `IDataObject::GetData`。**2026-09-01 真机暴露两个问题、根因同一个**：图片双表示发布是全仓库唯一「有 OLE 对象存活、却无 `virtual_receive` 门控轮询、也无人消费 `ManagerEvent`」的状态。① Word 有时要粘两次 —— 本地轮询经 arboard 每 50ms 反复 `OpenClipboard`，抢掉 Word 的 `OpenClipboard`（也解释了此前 trace 里「Word 从未接触数据对象」）；② Linux 目录批次在 Windows 粘贴卡死且下一个也粘不了 —— 图片时代残留的 `ClipboardReplaced` 被下一个批次 offer 吃掉并将其取消（**②为 task-061 引入的回归**）。已修：`publish_collection_synced` 同步确认发布、长期闸门 `image_clipboard_owned`（替换先前瞄错窗口的 500ms 静默期）、图片时代自行 drain 事件、`discard_stale_ole_events` 发布前清队列。**用户确认五项真机复测全部通过**（收图后本机复制能同步到 Linux —— 闸门解除路径、目录批次不再卡死、Word 一次贴成、Explorer 贴 `.png`、文本/批次/替换/断线回归），**task-061 的 Windows 侧验收清单至此全部通过**。剩余两项：Windows→Linux 图片方向两场景（`LinuxAutoImageReceive` 路径）未单独验收；`task-057-diagnostics` 是否从 `desktop:standalone` 默认移除待定（见 task-061）。原「无法粘贴
到 Word」的 `pending` 状态已在 task-061 中标记 `resolved`。不复活 019A。

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
