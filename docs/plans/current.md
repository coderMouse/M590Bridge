# 当前计划 · M590Bridge

> 更新：2026-08-17
> 阶段：task-054 Linux 一键打包已生成实包；Windows 一键脚本待 Windows 构建机验证；task-042 继续暂停

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

## 进行中 / 暂停

- [x] Linux↔Windows 对 task-036 做同文件、同网络实机复测（用户确认：两边可复制文件）
- [x] **task-040** 修复 Linux 内嵌 Hub 持续离线提示
- [x] **task-041** 桌面壳经 IPC 访问内嵌 Hub（修复仍不可达）
- [ ] **task-042** Windows NSIS 安装包 / 用户登录自启（NSIS 已成功打包安装；登录自启、卸载清理与跨机回归待验收）
- [ ] **task-054** Linux / Windows 一键打包脚本（Linux `.deb` 已真实通过；Windows PowerShell 脚本待 Windows 构建机执行）

## 产品分期对照

| 原分期 | 内容 | 状态 |
|--------|------|------|
| MVP | 配对 + 文本 | **已完成** |
| V2 · 图片 | 图片剪贴板双向 | **已完成** |
| V2 · 文件 | 元数据 + 按需 + 进度 + 流式 | **已完成当前单文件范围**（task-033 流式+SHA-256；Linux→Windows Explorer 按需粘贴真机验收通过，无文件夹/断点续传） |
| V3 · mDNS | 局域网发现 | **第一刀完成**（task-029） |
| V3 · 安装 | 安装包/自启 | **Linux `.deb` + 用户登录自启已完成**；Windows NSIS 已成功打包安装，HKCU 自启待真机验收 |

### 明确取消

- **019A 收图落盘捷径**：**不做**

## 能力边界（当前）

| 能力 | 状态 |
|------|------|
| 文本/图片双向 | 有 |
| 文件 offer/request/chunk/complete | 有 |
| hub 自动落盘 + send_file(_bytes) | 有 |
| UI 选文件发送 + 进度 + 保存目录 | **有** |
| 文件夹 / OS 文件剪贴板 | Windows Explorer 与 Linux FUSE 单文件按需粘贴均已跨机真机通过；无文件夹 |
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
| Windows NSIS 安装包 | **已成功打包安装**（task-042；当前用户安装、未签名；功能回归待验收） |
| Windows 用户登录自启 | **待真机验收**（task-042；HKCU Run、开发壳拒绝开启、卸载清理） |
| 一键本地打包 | **Linux 已验证，Windows 脚本待执行**（task-054；脚本负责环境检查、`npm ci`、Tauri 打包和产物路径输出） |
| localhost Hub API 鉴权 | **有**（task-035；进程临时令牌 + 限定 CORS） |
| 设置「发现方式」开关 | 无（默认开启 browse） |

## 下一步（有序）

1. 在 Windows 构建机执行 task-054 一键脚本，确认 NSIS `.exe` 产物路径输出
2. 等待用户决定是否恢复 task-042 剩余的登录自启与跨机回归验收（当前暂停）
3. （可选）独立文件数据连接 / 更高吞吐调优
4. （可选）设置页发现开关 / 本机显示名
5. （可选）多文件并行 / 文件夹

> task-044 至 task-053 均已完成相应真机验收。task-052 已证明 Linux FUSE 单文件可在
> Nautilus 粘贴时通过网络惰性读取并显示系统进度；task-053 已修复 Linux 托盘文字回归。
> task-042 已确认 NSIS 可生成并安装，但剩余验收暂停。

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
- 主面板「文件传输」：原生选文件/拖放；设置里改接收目录  
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
