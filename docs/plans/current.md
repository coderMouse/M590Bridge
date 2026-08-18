# 当前计划 · M590Bridge

> 更新：2026-08-18
> 阶段：task-057 无网络 OLE 探针通过，standalone 增加旧 Hub 端口预检后等待干净复测

## 目标（近期）

Linux + Windows 剪贴板与小文件桥；局域网发现；后续安装/自启。

## 已完成

- [x] task-001..018 文本/图片与硬化
- [x] **task-020** 文件协议 + Session loopback
- [x] **task-021** hub 落盘 / send_file / status
- [x] **task-022** UI 选文件发送、进度条、保存目录设置；`send_file_bytes`
- [x] **task-023** 协议不兼容提示（type 11）+ Win 选文件按钮
- [x] **task-024** file_list → 非图片原文件 offer
- [x] **task-025** 文本路径 offer + 发送方 FileComplete 进度 done
- [x] **task-026** GNOME Wayland 文件复制限制：拖放/选文件 + 提示
- [x] **task-027** 原生选文件/窗口拖放 + 托盘文案保活 + 关闭焦点
- [x] **task-028** 桌面裸文件名解析 + 托盘恢复关闭可点
- [x] **task-029** mDNS 广播 + `GET /api/discover` + UI joiner 点选
- [x] **task-030** 配对 reject/超时退出 + 错误提示；清理 smoke 配置污染
- [x] **task-031** 发现列表按 device_id/addr 去重 + 手动刷新
- [x] **task-032** Linux `.deb` 安装包基线
- [x] **task-033** 大文件流式传输（路径发送 / `.part` / SHA-256）
- [x] **task-034** 桌面 UI 自适应原生窗口（满窗口外壳 / 响应式导航 / 宽屏双列）
- [x] **task-035** Hub 与文件通道发布硬化（localhost API 鉴权 / 协议版本 / 上传与落盘正确性）
- [x] **task-036** 桌面文件传输吞吐调优（工作感知调度 / 顺序读取 / TCP 多帧缓冲修复）
- [x] **task-037** 文件通道安全边界
- [x] **task-038** Linux 用户级登录自启（XDG autostart + 设置页开关）
- [x] **task-039** Linux 自启开发壳保护 + 独立桌面运行（release 内嵌 UI/Hub）
- [x] **task-040** 修复 Linux 内嵌 Hub 持续离线提示（先 bind API / token 可重试 / 原因文案）
- [x] **task-041** 桌面壳经 IPC 访问内嵌 Hub（避免 https WebView fetch http 被拦）
- [x] **task-042** Windows NSIS 安装包 / 用户登录自启（安装、自启、卸载清理、跨机回归真机通过）
- [x] **task-043** Windows 单文件 OLE 虚拟剪贴板原型（Explorer 按粘贴取流 + 系统复制进度，真机通过）
- [x] **task-044** Windows 按粘贴请求 FileRequest、网络有界流与 FileCancel（Windows↔Linux 真机验收通过）
- [x] **task-045** 新文件 offer 替换旧 offer 时不误报失败（代码验证与 Windows↔Linux 真机复测通过）
- [x] **task-046** 文件 offer/按需传输生命周期修复（Windows↔Linux 真机验收通过）
- [x] **task-047** Linux 关闭到托盘与桌面图标统一（Linux/Windows 真机交互验收通过）
- [x] **task-048** Windows 本机剪贴板替换不中断已开始的 Explorer 粘贴（Windows↔Linux 真机验收通过）
- [x] **task-049** 配对总超时与断开后单次重连（Linux↔Windows 真机验收通过）
- [x] **task-050** GNOME Wayland 单文件 URI 剪贴板可行性（`x11-fallback` → Nautilus 真机粘贴与内容校验通过）
- [x] **task-051** Linux FUSE 单文件按需粘贴原型（惰性读取 + Nautilus 系统进度 + 内容校验，真机通过）
- [x] **task-052** Linux FUSE 单文件接入现有网络按需流（Linux↔Windows 真机验收通过）
- [x] **task-053** Linux 托盘菜单文字回归（GNOME/Wayland 真机复测通过）
- [x] **task-054** Linux / Windows 一键打包脚本（两端实包 + Windows 成功/异常按键暂停验收通过）
- [x] **task-055** 多文件批次清单与路径安全基础（type 16；仅协议模型与本地验证）
- [x] **task-056** 多文件选择、目录扫描与串行批次传输（Linux/Windows 桌面交互与跨机验收通过）

## 进行中 / 暂停

- [ ] **task-057** Windows Explorer 多文件剪贴板粘贴（OLE 集合探针通过；完整桌面端首次
  复测无变化，已增加 5910 旧实例预检，Windows 10 待干净复测）
- [x] Linux↔Windows 对 task-036 做同文件、同网络实机复测（用户确认：两边可复制文件）
- [x] **task-040** 修复 Linux 内嵌 Hub 持续离线提示
- [x] **task-041** 桌面壳经 IPC 访问内嵌 Hub（修复仍不可达）

## 已登记的后续任务

- [ ] **task-058** Linux FUSE 虚拟目录树（依赖 task-055、task-056）

## 产品分期对照

| 原分期 | 内容 | 状态 |
|--------|------|------|
| MVP | 配对 + 文本 | **已完成** |
| V2 · 图片 | 图片剪贴板双向 | **已完成** |
| V2 · 文件 | 元数据 + 按需 + 进度 + 流式 | **单文件 OS 粘贴与手动多文件/目录批次已完成**（无 OS 多文件/文件夹直接粘贴或断点续传） |
| V3 · mDNS | 局域网发现 | **第一刀完成**（task-029） |
| V3 · 安装 | 安装包/自启 | **Linux `.deb` + 用户登录自启、Windows NSIS + HKCU 自启均完成**；均为当前用户安装，未签名 |

### 明确取消

- **019A 收图落盘捷径**：**不做**

## 能力边界（当前）

| 能力 | 状态 |
|------|------|
| 文本/图片双向 | 有 |
| 文件 offer/request/chunk/complete | 有 |
| 多文件/目录手动批次 | **真机验收通过**（task-056；原生多选/目录/拖放、安全扫描、串行传输、整批暂存发布、双层进度/取消） |
| hub 自动落盘 + send_file(_bytes) | 有 |
| UI 选文件发送 + 进度 + 保存目录 | **有** |
| 文件夹 / OS 文件剪贴板 | 手动文件夹批次已通过；Windows Explorer 多文件/目录代码待真机验收，Linux FUSE 仍限单文件 |
| 大文件流式（磁盘流+SHA-256，软上限 8GiB） | **有**（task-033；同连接串行；task-036 已移除固定批次节流和多帧累计误判） |
| file_list 触发原文件 offer（非图片，路径流式） | **有** |
| 路径文本（非图片）→ file offer | **有**（task-025） |
| 发送方 FileComplete → UI done/满进度 | **有**（task-025） |
| GNOME Wayland 文件管理器复制自动同步 | **受限**（用原生选文件/窗口拖放，task-026/027） |
| GNOME Wayland 文件 URI 剪贴板发布 | **可行性通过**（task-050；`x11-fallback` 的 `text/uri-list` 可被 Nautilus 粘贴） |
| Linux FUSE 单文件按需读取 | **网络按需流真机通过**（task-052；`FileRequest` / 有界流 / `FileCancel`） |
| UI 拖入/原生选文件发送 | **有**（task-026/027） |
| UI 自适应桌面窗口 | **有**（task-034；窄屏底栏、宽屏侧栏/双列） |
| 托盘菜单文案保活 | **有**（task-053；AppIndicator 挂接后刷新标签，GNOME/Wayland 真机通过） |
| mDNS 发现（`_m590bridge._tcp`） | **有**（task-029；仍需配对码） |
| Linux `.deb` 安装包 | **有**（task-032；amd64、未签名） |
| Linux 用户登录自启 | **有**（task-038/039；正式/standalone 桌面端显式启停，开发壳拒绝开启） |
| Windows NSIS 安装包 | **真机验收通过**（task-042；当前用户安装、未签名） |
| Windows 用户登录自启 | **真机验收通过**（task-042；HKCU Run、开发壳拒绝开启、关闭/卸载清理） |
| 一键本地打包 | **Linux / Windows 均已验收**（task-054；环境检查、`npm ci`、Tauri 打包、产物路径输出，Windows 成功/异常按键暂停均通过） |
| localhost Hub API 鉴权 | **有**（task-035；进程临时令牌 + 限定 CORS） |
| 设置「发现方式」开关 | 无（默认开启 browse） |

## 下一步（有序）

1. Windows 10 从托盘退出全部旧实例后重新运行 standalone，复测 **task-057** 多文件/嵌套目录；通过后继续取消、替换、断线与单文件回归
2. task-057 完成后执行 **task-058**：Linux FUSE 虚拟目录树
3. 完成 OS 文件管理器多文件/文件夹直接粘贴端到端真机验收
4. （可选）设置页发现开关 / 本机显示名
5. 如需发布到非开发用户，另建代码签名/升级机制 task，不改动现有传输协议

> task-042 至 task-054 均已完成相应真机验收。task-052 已证明 Linux FUSE 单文件可在
> Nautilus 粘贴时通过网络惰性读取并显示系统进度；task-053 已修复 Linux 托盘文字回归。
> task-042 已完成 Windows 安装、自启、卸载清理与跨机回归验收；task-056 的手动
> 多文件/目录批次也已完成两端交互与跨机验收，但不包含 OS 文件剪贴板目录树或断点续传。

## 用户怎么用

```bash
cd ui
npm run desktop:standalone
```

该命令运行内嵌 UI/Hub 的 release 桌面端，不需要浏览器或 Vite。`desktop:dev` 与
`cargo run -p m590-ui` 仅用于开发，不能作为登录自启目标。

Linux 安装包：

```bash
./ui/scripts/package-linux.sh
sudo apt install ./target/release/bundle/deb/M590Bridge_*_amd64.deb
```

Windows 10（在 Windows 开发终端中）：

```powershell
.\ui\scripts\package-windows.ps1
```

- **创建配对**：本机生成配对码 → 开始等待（会 mDNS 广播）  
- **加入**：同一局域网列表点选对端，或手动填 `host:port`，输入同一配对码连接  
- 主面板「文件传输」：原生多选文件、选择文件夹或多路径拖放；设置里改接收目录
- Linux/Windows 设置页「启动」可显式开启/关闭当前用户登录自启；需从安装版或 standalone 桌面端设置

```bash
export M590_HUB_TOKEN='[REDACTED]' # 独立 Hub 的实际临时令牌；Tauri 内嵌模式无需手工设置
curl -s -H "X-M590-Token: $M590_HUB_TOKEN" http://127.0.0.1:5910/api/discover
curl -s -X POST http://127.0.0.1:5910/api/send_file_bytes \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"a.txt","data_base64":"aGVsbG8="}'
```

## 新会话入口

`AGENTS.md` → `docs/agent/*` → **本文件**
