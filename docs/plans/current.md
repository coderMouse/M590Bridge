# 当前计划 · M590Bridge

> 更新：2026-09-01
> 阶段：task-061 Windows 侧验收清单全部真机通过（长期闸门 + OLE 事件归属修复生效）；剩 Windows→Linux 图片方向验收与诊断收敛

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
- [x] **task-057** Windows Explorer 多文件剪贴板粘贴（用户确认 Windows 真机测试通过）
- [x] **task-058** Linux FUSE 虚拟目录树（大文件批次、目录树及生命周期真机验收通过）
- [x] **task-059** 统一应用版本来源（根 workspace 为唯一来源；Tauri/npm 不再重复维护）

## 进行中 / 暂停

- **task-061**：图片与图片文件的「双表示」粘贴（位图 + 虚拟图片文件）。方案 A
  （接收端物化）初版已实现：Windows 收到位图 → OLE 双表示（Word 贴位图 +
  Explorer 粘贴成 `.png` 文件，含 `CF_DIBV5`/已注册 `PNG`）；Linux 收到单图片
  文件 offer（可解码扩展名、≤32MiB）→ 自动下载解码写剪贴板位图。代码级验证
  通过；**待 Windows 10 / GNOME Wayland 真机验收**（见 task-061）。prtsc 后
  “不能粘贴成文件 + 后续文件复制阻塞”已修复：Windows 位图接收改为单一 OLE
  发布（不再先裸写 `EmptyClipboard` 覆盖 OLE owner），OLE 发布对
  `CLIPBRD_E_CANT_OPEN` 短暂重试。剩余问题：prtsc 位图 → Windows 可粘贴
  成图片文件、但无法粘贴到 Word。2026-08-31 已为该问题补齐 Windows OLE 诊断
  日志（`GetData` 拒绝 HRESULT、格式 id 映射表、双表示净荷大小；均在
  `task-057-diagnostics` 下，正式构建不变），并顺带修复验证命令
  `cargo test -p m590-clipboard --lib` 的并行临时目录竞态（30 次连跑全通过）。
  真机 trace 已回传并逐条核对：`Ctrl+V` 后确有约 25 次 `GetData`，但
  **`CF_BITMAP`(2) / `CF_DIB`(8) / `CF_DIBV5`(17) 一次都没被请求**。据此补了
  `CF_DIB`，**第二轮真机验证证明无效**：格式表与净荷都正常（`dib=8`、
  `dib_bytes=16367404`），但仍零次 `cf=8` 请求，且 `QueryGetData` 调用 0 次、
  两个场景的读取序列几乎逐字节相同 —— 说明那串请求是 Explorer/剪贴板监视器
  的指纹，**Word 很可能从未接触我们的数据对象**，两轮「补格式」方向本身有误。
  2026-08-31 第三轮改为只补一条可证伪诊断：发布后枚举系统剪贴板真实格式列表
  （`system_clipboard_formats`，诊断 feature 内，正式构建空 stub）。
  **第三轮真机：场景一、场景二均通过。** trace 证明发布环节是对的 ——
  系统剪贴板确实暴露 `8`(CF_DIB)、`17`(CF_DIBV5) 及系统合成的 `2`(CF_BITMAP)，
  `OleFlushClipboard` 分支证伪。按 publish 切段统计还得到「Word 是否真的读了」
  的指纹（`TYMED_ISTORAGE` 探测 OLE 内嵌格式 + `cf=13`），它与用户报告的
  成功/失败在 7 个观测点上完全一致：**当初的失败是 Word 根本没读，不是读了拒绝**。
  **2026-09-01 最小实验已跑：Windows 端用 `desktop:standalone:nodiag`（不带
  `task-057-diagnostics`）跑场景 A，用户确认通过** → 诊断非 load-bearing，
  「正式构建可能仍坏」的风险证伪，2026-08-29 起 `pending` 的「prtsc 位图 →
  Windows 无法贴 Word」关闭。**仍未解决**：第二轮失败的构建已含 `CF_DIB`，与本轮
  通过的构建产品代码等价，fail→pass 无代码解释（测试侧偶发 / 竞争未触发，单次
  通过无法区分）；`hub.rs:4770-4781` 图片双表示发布不抑制本地轮询这一缺口未修。
  正式构建上仍待真机：Explorer 粘贴成 `.png`、Windows→Linux 两场景、文本/批次/
  替换/断线回归。

## 产品分期对照

| 原分期 | 内容 | 状态 |
|--------|------|------|
| MVP | 配对 + 文本 | **已完成** |
| V2 · 图片 | 图片剪贴板双向 | **已完成** |
| V2 · 文件 | 元数据 + 按需 + 进度 + 流式 | **Linux/Windows 单文件及多文件/目录 OS 粘贴均已完成真机验收**；断点续传未实现 |
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
| 文件夹 / OS 文件剪贴板 | 手动文件夹批次及 Windows Explorer、Linux Nautilus 多文件/目录均已真机验收通过 |
| 大文件流式（磁盘流+SHA-256，软上限 8GiB） | **有**（task-033；同连接串行；task-036 已移除固定批次节流和多帧累计误判） |
| file_list 触发原文件 offer（非图片，路径流式） | **有**；单文件走原 offer，多路径/目录走批次，Windows Explorer 验收通过 |
| 路径文本（非图片）→ file offer | **有**（task-025） |
| 发送方 FileComplete → UI done/满进度 | **有**（task-025） |
| GNOME Wayland 文件管理器复制自动同步 | **受限**（用原生选文件/窗口拖放，task-026/027） |
| GNOME Wayland 文件 URI 剪贴板发布 | **可行性通过**（task-050；`x11-fallback` 的 `text/uri-list` 可被 Nautilus 粘贴） |
| Linux FUSE 按需读取 | **task-058 已完成**；单文件/tree 本地真实挂载及 GNOME Wayland + Nautilus 大文件批次、目录树和生命周期跨机验收通过 |
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

1. **task-061（已完成）**：图片/图片文件「双表示」
   粘贴。2026-09-01 真机暴露的两个问题（① Word 有时要粘两次 —— 本地轮询经 arboard
   每 50ms 反复 `OpenClipboard` 抢掉 Word 的；② Linux 目录批次在 Windows 粘贴卡死
   且下一个也粘不了 —— 图片时代残留的 `ClipboardReplaced` 被下一个批次 offer 吃掉
   并将其取消，**②为 task-061 引入的回归**）**根因同一个**：图片发布是全仓库唯一
   「有 OLE 对象存活、却无 receive 门控轮询、也无人消费 `ManagerEvent`」的状态。
   已修：长期闸门 `image_clipboard_owned`（替换瞄错窗口的 500ms 静默期）、图片时代
   drain 自己的事件、`discard_stale_ole_events` 发布前清队列。
   **用户确认五项真机复测全部通过**：收图后本机复制能同步到 Linux（闸门解除路径，
   最关键）、目录批次不再卡死、Word 一次贴成、Explorer 贴 `.png`、文本/批次/替换/
   断线回归。**Windows→Linux 方向亦已通过**（`LinuxAutoImageReceive`）。
   **task-061 功能验收清单至此全部通过**；「多图批次贴不进 Word」是方案 A 的设计
   边界（`auto_image_eligible` 只对单文件 offer 生效），非缺陷。
   **收尾已完成**（2026-09-01）：`task-057-diagnostics` 已从 `desktop:standalone`
   默认移除，新增 `desktop:standalone:diag`（含 `pre` 钩子）保留带诊断构建；原
   `desktop:standalone:nodiag` 已删除 —— 它当初只为绕开写死的诊断，现与
   `desktop:standalone` 完全等同。日常 standalone 不再输出诊断日志，Windows 也不再
   保留控制台窗口。
2. **task-062（已完成）**：Windows 粘贴多文件时「取消整个批次」按钮无效。根因已
   定位：`hub.rs` 的 `cancel_batch` 块里取消虚拟批次那段只有
   `#[cfg(target_os = "linux")]` 分支，Windows 上按钮只走到 `cancel_runtime_batch`
   （仅管手动发送/接收批次），剪贴板虚拟批次从未接入取消路径。**已修复并真机通过**
   （2026-09-01）：新增结构镜像的 Windows 分支，取消按钮立即生效，后续批次不受
   影响，Linux 侧取消功能未回归。
3. **task-063（新，优化项）**：Windows 粘贴含文件夹的批次时不弹系统复制进度窗口。
   机制已定位：`windows_virtual_file.rs:335-338` 把 `FD_PROGRESSUI` 只加在非目录
   条目上。风险提示：task-058 目录树粘贴已验收，回归比缺进度窗口严重。
4. task-060（收尾）：Q1/Q2、Q3 权限、Q4 角标均真机通过；mp4/多个 pdf 替换
   七轮修复已由用户确认；「目录 + mp4」批次替换的八轮修复已回滚，日常以
   「文件夹与其他文件分开复制」规避（见 task-060）。
5. 为 Rust/前端构建与测试增加 CI
6. （可选）设置页发现开关 / 本机显示名
7. 如需发布到非开发用户，另建代码签名/升级机制 task，不改动现有传输协议

> task-042 至 task-054 已完成相应真机验收；task-056 的手动多文件/目录批次和 task-057
> 的 Windows Explorer 多文件剪贴板均已完成两端验收。task-052 已证明 Linux FUSE 单文件
> 可在 Nautilus 粘贴时通过网络惰性读取并显示系统进度；task-053 已修复 Linux 托盘文字
> 回归。task-058 已完成 Linux FUSE 虚拟目录树、路径安全、非阻塞背压和活跃流剪贴板
> 轮询保护；单文件/tree 本地真实挂载，以及 GNOME Wayland + Nautilus 的大文件批次、
> 嵌套/空目录、空文件、取消、替换、断线和重新发送后再次粘贴均已通过。断点续传和同一
> 已消费 clipboard offer 任意次重开不在 task-058 范围内。task-060 已实现同一 offer 串行
> 重开（发送源保留、stream offer 保留、bridge/FUSE 重开），Q1/Q2 已真机通过；Q3
> 权限修复与 Q4 角标已真机通过；**mp4/pdf 粘贴替换修复：六轮（旧流在途先取消
> 再重开 + 陈旧 chunk 丢弃 + 管道重开门控）真机 mp4/单个 pdf 通过；七轮（忽略旧轮
> 在途 `FileComplete(ok)`）用户已确认通过；八轮（条目级失败隔离，针对「目录 + mp4」
> 批次替换）复测仍有问题，已回滚至七轮版本，暂以「文件夹与其他文件分开复制」规避
> （见 task-060）**。并发重开、
> 断点续传与替换后重开仍不保证。**task-061 已立项：图片/图片文件「双表示」
> 粘贴（Word 贴位图 + 文件管理器贴图片文件），方案 A 接收端物化，版本升至 0.1.2
> （见 task-061）**。task-059 已将应用版本收敛到
> 根 workspace，并通过 Tauri Linux 实包确认 `.deb` 元数据随版本号更新。

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
