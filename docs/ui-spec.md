# M590Bridge UI 规格

> 用途：Figma 设计与前端实现的统一对照文档。  
> 范围：Linux（Ubuntu）+ Windows 10 桌面端工具 UI。  
> 技术预期：Rust 核心服务 + 轻量桌面壳（建议 Tauri 2）+ 系统托盘。  
> 版本：v0.1（草稿，可随 Figma 定稿修订）

---

## 1. 产品与设计目标

### 1.1 一句话

两台电脑之间的剪贴板与文件桥：在 A 机复制，用罗技多设备鼠标切到 B 机后直接粘贴。

### 1.2 设计原则

| 原则 | 说明 |
|------|------|
| 托盘优先 | 默认常驻托盘；主面板是按需打开的工具窗 |
| 状态一眼可见 | 连接、同步中、暂停、断开必须在 1 秒内可辨认 |
| 少步骤 | 配对一次，之后尽量零交互 |
| 文件与文本分离表达 | 文本可即时同步；文件强调大小、方向、进度 |
| 安静克制 | 不社交、不游戏化、不重后台；像 LocalSend + 系统工具 |
| 双语文案空间 | 默认中文，布局预留英文变长 30% |

### 1.3 非目标（UI 不做）

- 多设备网格（首版仅 1 对 1）
- 云账号 / 聊天 / 历史云同步
- 远程键鼠控制（用户已有 M590 硬件切机）
- 复杂账号权限体系

### 1.4 目标平台视觉差异

| 平台 | 处理方式 |
|------|----------|
| Windows 10 | 托盘菜单偏原生；窗口可轻微圆角 |
| Ubuntu | 托盘/状态栏表现因桌面而异；窗口与 Win 共用同一套组件语言 |
| 设计稿 | 先出 **通用面板**；托盘菜单各出 Win / Linux 各一版即可 |

---

## 2. 信息架构

```text
系统托盘
├── 状态摘要
├── 最近一条同步
├── 暂停 / 恢复同步
├── 手动发送当前剪贴板
├── 打开主面板
├── 设置
└── 退出

主窗口
├── 首次配对（未配对时）
├── 主面板（已配对）
│   ├── 连接状态
│   ├── 本机 / 对端设备卡
│   ├── 当前剪贴板
│   ├── 最近同步
│   └── 快捷开关
├── 传输进度（叠加卡片 / 独立小窗）
└── 设置

系统通知
├── 同步成功
├── 收到文件待粘贴
├── 传输完成 / 失败
└── 连接断开与重试
```

---

## 3. 设计系统

### 3.1 画板与尺寸

| 画板 | 尺寸 | 备注 |
|------|------|------|
| 主面板 | 380 × 520 | 可高度自适应到 560 |
| 配对页 | 400 × 560 | 含插画时可更高 |
| 设置页 | 420 × 600 | 可滚动 |
| 传输卡片 | 360 × 280 | 也可嵌入主面板 |
| 托盘菜单 | 240 × auto | Win / Linux 各一 |
| 通知 | 360 × 88 | 多行可达 110 |
| UI Kit | 1440 × 900 | 组件与色板 |

密度：紧凑工具风。基础间距 **8px**；卡片内边距 **12–16px**；区块间距 **16–24px**。

### 3.2 圆角与阴影

| Token | 值 | 用途 |
|-------|-----|------|
| `radius.sm` | 6px | 按钮、输入、小标签 |
| `radius.md` | 10px | 卡片、列表行 |
| `radius.lg` | 14px | 主窗口外轮廓（若自定义） |
| `shadow.card` | `0 4px 16px rgba(15, 23, 42, 0.08)` | 卡片 |
| `shadow.float` | `0 8px 28px rgba(15, 23, 42, 0.12)` | 浮层/传输卡 |

避免过度毛玻璃；如需 backdrop，透明度不超过 8%。

### 3.3 颜色

#### Light

| Token | Hex | 用途 |
|-------|-----|------|
| `bg.app` | `#F4F6F9` | 窗口背景 |
| `bg.surface` | `#FFFFFF` | 卡片 |
| `bg.subtle` | `#EEF2F6` | 次级区块 |
| `text.primary` | `#0F172A` | 主文字 |
| `text.secondary` | `#64748B` | 次要说明 |
| `text.inverse` | `#FFFFFF` | 深色底文字 |
| `border.default` | `#E2E8F0` | 分割/描边 |
| `brand.primary` | `#2563EB` | 主按钮、链接 |
| `brand.primaryHover` | `#1D4ED8` | hover |
| `status.connected` | `#16A34A` | 已连接 |
| `status.syncing` | `#D97706` | 同步中 |
| `status.paused` | `#64748B` | 已暂停 |
| `status.disconnected` | `#DC2626` | 断开/错误 |
| `status.info` | `#0284C7` | 提示 |

#### Dark

| Token | Hex | 用途 |
|-------|-----|------|
| `bg.app` | `#0B1220` | 窗口背景 |
| `bg.surface` | `#121A2B` | 卡片 |
| `bg.subtle` | `#1A2438` | 次级区块 |
| `text.primary` | `#E5E7EB` | 主文字 |
| `text.secondary` | `#94A3B8` | 次要 |
| `border.default` | `#243044` | 描边 |
| `brand.primary` | `#3B82F6` | 主色 |
| 状态色 | 与 Light 同色相，略提亮一档 | 保持语义一致 |

### 3.4 字体

| 用途 | 规格 |
|------|------|
| 家族 | Inter / 系统 UI（Win: Segoe UI；Linux: Inter/Noto Sans） |
| 窗口标题 | 16px / Semibold / 24 line |
| 区块标题 | 13px / Semibold / 20 line |
| 正文 | 13px / Regular / 20 line |
| 辅助 | 12px / Regular / 16 line |
| 配对码 | 28–32px / Bold / tracking +4 |
| 数字/速度 | Tabular nums |

### 3.5 图标

- 风格：2px 描线图标，24 网格，圆角端点
- 必要图标：clipboard、image、file、files、link、laptop、arrow-left-right、check、x、pause、play、settings、refresh、copy、qr-optional、wifi、alert
- 应用图标概念：两台设备圆角矩形 + 桥接/剪贴板记号；**不要**画鼠标
- 托盘图标状态：
  - `tray.idle.connected`
  - `tray.syncing`（可微小动画或角标）
  - `tray.paused`
  - `tray.disconnected`

### 3.6 动效（设计标注即可）

| 场景 | 建议 |
|------|------|
| 面板打开 | 120–160ms fade + 轻微上移 |
| 状态胶囊变化 | 颜色 150ms crossfade |
| 列表新增 | 120ms 高度展开 |
| 进度条 | 平滑，不弹跳 |
| 成功 | 可选短 check，不放 confetti |

---

## 4. 组件库

### 4.1 按钮 Button

| 变体 | 使用 |
|------|------|
| Primary | 主操作：开始配对、重试、打开面板 |
| Secondary | 次操作：手动 IP、取消 |
| Ghost | 工具操作：刷新、复制 |
| Danger | 解除配对、清空历史 |
| IconButton | 复制、刷新、关闭 |

状态：default / hover / pressed / disabled / loading

### 4.2 状态胶囊 StatusPill

| 状态 | 文案 | 色 |
|------|------|----|
| connected | 已连接 | green |
| syncing | 同步中 | amber |
| paused | 已暂停 | gray |
| connecting | 连接中 | blue |
| disconnected | 未连接 | red |
| error | 出错 | red |

### 4.3 设备卡 DeviceCard

```text
[设备图标]
本机 · Ubuntu
小新 Pro
IP 可选显示（默认折叠）
```

对端卡增加次要操作：`重新配对`（放设置更合适，主面板仅展示）。

### 4.4 剪贴板预览卡 ClipboardPreviewCard

| 字段 | 说明 |
|------|------|
| 类型徽章 | 文本 / 图片 / 文件 / 混合 |
| 预览 | 文本前 2 行；图片缩略图；文件名列表最多 3 个 + N |
| 元信息 | 来源、相对时间、大小（文件） |
| 空态 | 见文案表 |

### 4.5 同步历史行 HistoryRow

```text
[类型图标]  摘要（单行省略）
           本机 → 对端 · 12 秒前 · 2.1MB
```

点击：文本可展开预览；文件可显示“若尚未传输，粘贴时开始”。

### 4.6 开关 ToggleRow

左侧标题 + 右侧说明（可选）+ Switch。  
用于：自动同步文本/图片、文件粘贴时再传输、通知开关。

### 4.7 进度 ProgressBlock

- 总进度条
- 速度（MB/s）
- ETA
- 文件列表（可滚动，最多可视 3 行）
- 操作：暂停 / 继续 / 取消

### 4.8 输入 Input

- 默认、focus、error、disabled
- 配对码输入：6 位，可分组 `483 291`
- IP 输入：placeholder `192.168.x.x`

### 4.9 通知 Toast/Notification

系统通知样式 + 应用内轻提示（可选）。  
含图标、标题、一行正文、可选操作（重试 / 打开）。

---

## 5. 页面规格

### 5.1 首次配对 Onboarding / Pairing

**进入条件**：本地无有效配对会话。

**布局**

1. 顶部标题区
2. 双设备插画（两台笔记本 + 虚线桥）
3. 本机信息卡
4. 大号配对码 + 刷新 + 复制
5. 主按钮：`已在另一台设备确认`
6. 次级：`手动输入 IP / 配对码`
7. 底说明：同一局域网、防火墙提示链接 `如何使用`

**状态**

| 状态 | UI |
|------|----|
| 等待对端 | 文案：正在等待另一台电脑… |
| 发现设备 | 显示对端名称，可点连接 |
| 配对中 | 主按钮 loading |
| 成功 | 短成功页 1s 后进主面板 |
| 失败 | 行内错误 + 重试 |

**Figma 重点**：配对码是视觉焦点；不要把高级网络项塞进首屏。

### 5.2 主面板 Home

**进入条件**：已配对。

**结构（自上而下）**

1. 顶栏：`M590Bridge` + `StatusPill` + 可选窗口钉住
2. 设备行：`DeviceCard(本机)` — link 图标 — `DeviceCard(对端)`
3. `ClipboardPreviewCard`（当前剪贴板）
4. 区块标题：最近同步
5. `HistoryRow` 列表（默认 5 条，可“查看更多”后期再做）
6. 底栏：
   - Toggle：自动同步剪贴板
   - Toggle：文件粘贴时再传输
   - 文字按钮：设置
   - 文字按钮：暂停同步 / 恢复同步

**空历史**：插画或简单图标 + 空态文案。

**断连时**：顶栏红态；设备间 link 变灰；主内容可仍显示缓存的“最后剪贴板”，但加条横幅：`连接已断开，正在重试… [手动重连]`。

### 5.3 传输进度 Transfer

**触发**：文件开始实际传输（粘贴触发或用户手动发送）。

**形式**

- 优先：主面板内替换/覆盖中部卡片
- 次选：独立小浮卡（托盘也可点开）

**内容**

- 标题：`正在传输到 {对端名}` 或 `正在从 {对端名} 接收`
- 文件列表 + 单文件进度（可选）
- 总进度、速度、ETA
- 暂停 / 取消
- 完成态：成功摘要 + 完成路径原则（不暴露本机绝对隐私路径过长；可显示“已保存到下载/临时目录”）
- 失败态：原因简句 + 重试

### 5.4 设置 Settings

**分组**

1. **设备**
   - 本机显示名（可编辑）
   - 当前对端
   - 重新配对
   - 解除配对（Danger，二次确认）
2. **同步**
   - 自动同步文本
   - 自动同步图片
   - 文件仅在粘贴时传输（默认 ON）
   - 保留最近历史条数：10 / 20 / 50 / 关闭
3. **网络**
   - 发现方式：自动（mDNS）/ 手动
   - 端口（高级，默认折叠）
   - 仅允许已配对设备（默认 ON，不可作安全假象外的唯一手段）
4. **通知**
   - 同步成功通知
   - 传输完成通知
   - 断开连接通知
5. **关于**
   - 版本号
   - 开源许可入口
   - 日志目录（可选，开发期有用）

**二次确认弹窗**

- 标题：解除配对？
- 正文：解除后需重新输入配对码才能同步。
- 按钮：取消 / 解除配对

### 5.5 系统托盘菜单 Tray Menu

**顺序固定**

1. 状态（不可点或点开主面板）：`已连接到 {对端}` / `未连接` / `已暂停`
2. 最近：`已同步文本：会议纪要…`（单行）
3. 分隔线
4. 暂停同步 / 恢复同步
5. 手动发送当前剪贴板
6. 打开主面板
7. 设置
8. 分隔线
9. 退出

**平台变体**：Windows 经典菜单；Linux 在设计稿中做通用现代菜单即可，实现跟随托盘框架。

### 5.6 通知 Notifications

| ID | 标题 | 正文示例 |
|----|------|----------|
| N1 | 剪贴板已同步 | 文本已同步到 Windows 10 |
| N2 | 收到文件 | 来自 Ubuntu 的 3 个文件，粘贴后开始接收 |
| N3 | 传输完成 | 128MB · 用时 1.4s |
| N4 | 连接已断开 | 正在重试… |
| N5 | 传输失败 | 网络中断，点击重试 |

通知应可在设置中关闭；错误类建议默认保留。

---

## 6. 文案表（中文默认）

### 6.1 通用

| Key | 文案 |
|-----|------|
| app.name | M590Bridge |
| app.tagline | 双机剪贴板与文件桥 |
| action.copy | 复制 |
| action.refresh | 刷新 |
| action.retry | 重试 |
| action.cancel | 取消 |
| action.save | 保存 |
| action.open_settings | 设置 |
| action.open_panel | 打开主面板 |
| action.exit | 退出 |
| action.pause_sync | 暂停同步 |
| action.resume_sync | 恢复同步 |
| action.send_clipboard | 手动发送当前剪贴板 |

### 6.2 状态

| Key | 文案 |
|-----|------|
| status.connected | 已连接 |
| status.syncing | 同步中 |
| status.paused | 已暂停 |
| status.connecting | 连接中 |
| status.disconnected | 未连接 |
| status.error | 出错 |

### 6.3 配对

| Key | 文案 |
|-----|------|
| pair.title | 连接另一台电脑 |
| pair.subtitle | 两台电脑需在同一局域网。配对后即可跨设备复制粘贴。 |
| pair.this_device | 本机 |
| pair.code_label | 配对码 |
| pair.waiting | 正在等待另一台电脑… |
| pair.searching | 正在局域网中查找设备… |
| pair.confirm_cta | 已在另一台设备确认 |
| pair.manual | 手动输入 IP / 配对码 |
| pair.how_to | 如何使用 |
| pair.success | 已连接，可以开始复制了 |
| pair.fail_code | 配对码错误或已过期 |
| pair.fail_network | 找不到对端设备，请确认在同一网络 |
| pair.unlink | 解除配对 |
| pair.unlink_confirm_title | 解除配对？ |
| pair.unlink_confirm_body | 解除后需重新配对才能同步剪贴板和文件。 |

### 6.4 主面板

| Key | 文案 |
|-----|------|
| home.current_clipboard | 当前剪贴板 |
| home.recent | 最近同步 |
| home.empty_title | 还没有同步记录 |
| home.empty_body | 在一台电脑复制内容，切换鼠标后即可在另一台粘贴。 |
| home.auto_sync | 自动同步剪贴板 |
| home.file_on_paste | 文件粘贴时再传输 |
| home.direction.to_remote | 本机 → 对端 |
| home.direction.to_local | 对端 → 本机 |
| home.from | 来自 {device} |
| home.synced_ago | {time}前同步 |
| home.banner_disconnected | 连接已断开，正在重试… |
| home.reconnect | 手动重连 |

### 6.5 类型与历史

| Key | 文案 |
|-----|------|
| type.text | 文本 |
| type.image | 图片 |
| type.file | 文件 |
| type.files | 多个文件 |
| type.mixed | 混合 |
| history.files_more | 等 {n} 个文件 |
| history.text_ truncated_suffix | … |

### 6.6 传输

| Key | 文案 |
|-----|------|
| transfer.to | 正在传输到 {device} |
| transfer.from | 正在从 {device} 接收 |
| transfer.speed | {speed} MB/s |
| transfer.eta | 剩余 {time} |
| transfer.done | 传输完成 |
| transfer.failed | 传输失败 |
| transfer.pause | 暂停 |
| transfer.resume | 继续 |
| transfer.cancel | 取消 |
| transfer.retry | 重试 |

### 6.7 设置

| Key | 文案 |
|-----|------|
| settings.device | 设备 |
| settings.sync | 同步 |
| settings.network | 网络 |
| settings.notifications | 通知 |
| settings.about | 关于 |
| settings.device_name | 设备名称 |
| settings.remote | 当前对端 |
| settings.repair | 重新配对 |
| settings.auto_text | 自动同步文本 |
| settings.auto_image | 自动同步图片 |
| settings.file_on_paste | 文件仅在粘贴时传输 |
| settings.history_limit | 保留最近历史 |
| settings.history_off | 关闭 |
| settings.discovery | 发现方式 |
| settings.discovery.auto | 自动 |
| settings.discovery.manual | 手动 |
| settings.port | 端口 |
| settings.paired_only | 仅允许已配对设备 |
| settings.notify_sync | 同步成功提示 |
| settings.notify_transfer | 传输完成提示 |
| settings.notify_disconnect | 断开连接提示 |

### 6.8 相对时间

| 条件 | 文案 |
|------|------|
| < 5s | 刚刚 |
| < 60s | {n} 秒前 |
| < 60m | {n} 分钟前 |
| < 24h | {n} 小时前 |
| 更早 | {date} {time} |

---

## 7. 状态机（UI 视角）

### 7.1 连接状态 ConnectionState

```text
                    ┌──────────────┐
                    │ Unpaired     │
                    └──────┬───────┘
                           │ 开始配对
                           v
                    ┌──────────────┐
            ┌───────│ Pairing      │──────┐
            │ 失败  └──────┬───────┘ 取消 │
            v              │ 成功         v
     ┌────────────┐        v       ┌──────────────┐
     │ PairError  │   ┌──────────── consed as     │
     └────────────┘   │ Connecting / Reconnecting │
                      └──────┬────────────────────┘
                             │ 心跳成功
                             v
                      ┌──────────────┐
              ┌───────│ Connected    │──────┐
              │       └──────┬───────┘      │
           暂停              │ 断线      解除配对
              v              v              v
       ┌──────────────┐ ┌──────────────┐ ┌──────────┐
       │ Paused       │ │ Disconnected │ │ Unpaired │
       └──────┬───────┘ └──────┬───────┘ └──────────┘
              │ 恢复            │ 自动/手动重连
              v                 v
           Connected        Connecting
```

> 实现可用枚举：`Unpaired | Pairing | Connected | Paused | Reconnecting | Disconnected | Error`。

### 7.2 剪贴板同步状态 ClipboardSyncState

```text
Idle
  → OutgoingSyncing（本机复制，推送到对端）
  → IncomingApplying（对端推来，写入本机剪贴板）
  → Idle
  → Error（可自动回 Idle，并通知）
```

规则：

- `Paused` 时不自动推送/拉取；手动发送可仍允许（产品可选，默认允许并标注）。
- 文本/图片默认自动；文件默认只同步元数据。

### 7.3 文件传输状态 FileTransferState

```text
None
  → PendingRemoteFiles（已收到文件清单，等待本地粘贴）
  → Transferring
  → Paused
  → Completed
  → Failed
  → None（历史仅保留摘要）
```

### 7.4 主面板展示优先级

当多种状态同时存在时，中部主卡片优先级：

1. `Transferring` / `Failed`（传输卡）
2. `PendingRemoteFiles`（待粘贴文件提示卡）
3. `ClipboardPreviewCard`（普通当前剪贴板）
4. 空态

顶栏 `StatusPill` 始终反映 ConnectionState；同步中可用次要指示（托盘图标/小点），避免和传输进度抢主色。

---

## 8. 关键交互流

### 8.1 文本复制粘贴

1. 用户在 A 复制文本  
2. A 托盘可短暂进入 syncing  
3. B 收到并写入剪贴板  
4. 可选通知：剪贴板已同步  
5. 用户按 M590 切到 B，Ctrl+V  

**UI 不依赖**鼠标切机事件。

### 8.2 文件复制粘贴

1. A 复制文件（资源管理器）  
2. 同步文件元数据到 B（名、大小、数量）  
3. B 显示当前剪贴板类型=文件；通知“粘贴后开始接收”  
4. 用户在 B 粘贴  
5. 进入 Transferring，完成后写入目标位置并就绪  

### 8.3 暂停同步

- 全局停止自动同步  
- 主面板与托盘文案改为“已暂停”  
- 已在进行的文件传输：暂停同步**不自动取消传输**（仅停止新的剪贴板自动同步）；若要停止传输用传输卡取消  

### 8.4 重新配对

设置 → 重新配对 → 进入配对流；成功后清空旧对端会话，历史可保留或清空（默认保留本机历史，标注来源失效）。

---

## 9. 内容预览规则

| 类型 | 预览规则 |
|------|----------|
| 文本 | 最多 2 行，约 80 字，超出省略 |
| 密码/敏感 | 不做特殊识别（首版）；历史可关 |
| 图片 | 64–72px 圆角缩略图；大图不进历史原图 |
| 单文件 | 图标 + 文件名 + 大小 |
| 多文件 | 前 2–3 个文件名 + `等 n 个` + 总大小 |
| 目录 | 首版可当“文件集合”或提示暂不支持（实现期定，UI 预留 `文件夹` 类型） |

隐私：

- 历史默认仅本机短缓存  
- 设置可关闭历史  
- 设计稿不要用真实隐私数据；用中性示例（会议纪要、截图.png、设计稿.fig）

---

## 10. 示例数据（设计稿填充）

```text
本机：Ubuntu · 小新 Pro
对端：Windows 10 · 书房电脑

当前剪贴板：
类型 文本
内容 下周一起对齐 M590Bridge MVP 范围…
元信息 来自 Windows · 12 秒前同步

最近同步：
1. 文本  本机 → 对端  刚刚
2. 图片  对端 → 本机  1 分钟前  截图.png 2.1MB
3. 文件  本机 → 对端  5 分钟前  设计稿.fig 等 2 个 · 242MB

传输中：
设计稿.fig  240MB
截图.png    2.1MB
总进度 67% · 92 MB/s · 剩余 00:08
```

---

## 11. Figma 交付清单

### 11.1 页面

- [ ] UI Kit（色板、字体、按钮、开关、胶囊、列表行）
- [ ] 配对：等待 / 成功 / 失败
- [ ] 主面板：已连接 + 有数据
- [ ] 主面板：空历史
- [ ] 主面板：断连横幅
- [ ] 主面板：待粘贴文件
- [ ] 传输中 / 成功 / 失败
- [ ] 设置全页
- [ ] 解除配对确认框
- [ ] 托盘菜单 Win / Linux
- [ ] 通知 4–5 种
- [ ] 暗色：至少主面板 + 托盘 + 传输

### 11.2 标注

- 颜色 token 名与 Hex
- 间距 8 基准
- 各状态组件
- 中文最终文案（与本表 key 对齐更佳）
- 导出图标：app 512、tray 16/20/32 各状态

### 11.3 可点原型（可选）

```text
配对成功 → 主面板
主面板 → 设置 → 解除配对 → 配对
主面板 → 传输中 → 完成
托盘 → 打开主面板
```

---

## 12. 与后端能力映射

| UI 元素 | 后端/核心能力 | MVP |
|---------|----------------|-----|
| 配对码 / 手动 IP | 局域网发现、配对握手、会话密钥 | 是 |
| StatusPill | 心跳、连接状态机 | 是 |
| 当前剪贴板 | 系统剪贴板监听与写入 | 是（先文本） |
| 自动同步开关 | 推送策略 | 是 |
| 文件粘贴时再传输 | 文件元数据 vs 内容通道 | V2 |
| 传输进度 | 多连接文件传输进度事件 | V2 |
| 最近同步 | 本地短历史存储 | 可后置 |
| 暂停同步 | 暂停自动同步标志 | 是 |
| 通知 | OS notification | 可后置 |
| 暗色 | 跟随系统 / 设置 | 可后置 |

---

## 13. 无障碍与可用性（基础）

- 主操作对比度达标（文字对背景 ≥ 4.5:1）
- 仅用颜色区分状态时，搭配文字/图标
- 焦点顺序：配对码 → 主 CTA → 次级操作
- 危险操作必须确认
- 进度与错误不只靠颜色闪烁

---

## 14. 开放问题（设计可先假定，实现前确认）

| # | 问题 | 临时默认 |
|---|------|----------|
| 1 | 应用显示名是否改中文名 | 先用 M590Bridge |
| 2 | 文件默认保存目录 | 系统下载目录或用户自选（设置后置） |
| 3 | 是否做剪贴板历史详情页 | MVP 不做，仅主面板 5 条 |
| 4 | 是否支持 1 对多 | 不做 |
| 5 | 文件夹复制 | MVP 可提示暂不支持 |
| 6 | 托盘在 GNOME 上的表现 | 实现期用兼容方案；设计仍给通用菜单 |

---

## 15. 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| v0.1 | 2026-08-04 | 首稿：页面、组件、文案、状态机、Figma 清单 |
