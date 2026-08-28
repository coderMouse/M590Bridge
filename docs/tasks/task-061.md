# task-061 · 图片与图片文件的「双表示」粘贴（位图 + 虚拟图片文件）

## 状态

`in_progress`（已立项；版本 0.1.2。先实施方案 A：接收端物化。尚未开始开发）

## 背景

用户真机确认：图片内容（图片文件或剪贴板位图）跨机粘贴时，目标端只能使用一种形式，
且两个方向行为不对称。根因是现有两条通道互斥：

- **位图通道**：发送端把「单个图片文件」提升为位图（`hub.rs` `image_from_paths` /
  `image_from_clipboard_text`），接收端只 `write_image` 写剪贴板位图（`hub.rs`
  AppliedImage 分支）→ Word/画图可贴。
- **文件通道**：文件走 file offer，接收端只挂 OLE/FUSE 虚拟文件（Windows
  `windows_virtual_file_manager` / Linux FUSE tree）→ 文件管理器可贴。
- 两条通道不共存：单图片文件被提升为位图后丢失文件语义；收到位图不生成文件；
  收到文件不生成位图。

## 真机观察到的问题（用户确认）

| 方向 | 场景 | 现象 |
|------|------|------|
| Linux → Windows | 复制图片文件 | 只能粘贴到 Word，无法粘贴成图片文件 |
| Linux → Windows | 剪贴板为图片 | 只能粘贴到 Word，无法粘贴成图片文件 |
| Windows → Linux | 复制图片文件 | 只能粘贴成图片文件，无法粘贴到 Word |
| Windows → Linux | 剪贴板为图片 | 可粘贴成图片文件，也可粘贴到 Word（后者大概率是 Linux
  桌面把 image/png 剪贴板直接物化成文件，非 M590 生成） |

待确认：Windows→Linux 复制图片文件为何只走文件通道（按代码应被提升为位图）；
可用 `task-057-diagnostics` 抓发送端日志确认走的是哪条路径。

## 方案（推荐组合，不改协议 wire）

### 方案 A（本次实施）：接收端物化

1. **收到位图** → 除写剪贴板位图外，再发布一个虚拟图片文件 offer（命名如
   `image_<时间戳>.png`）：Windows 端用 OLE 虚拟文件（复用 task-043/044/057），
   Linux 端用 FUSE（复用 task-051/052/058）。解决 1、2，并让 4 在两端行为一致。
2. **收到单图片文件 offer** → 流转完成后解码成品位图写剪贴板（或边流边解码）：
   解决 3。GIF 取首帧；超大图沿用现有 `INLINE_IMAGE_MAX_BYTES` 上限跳过。

### 方案 B（后续可选）：发送端双发

复制单个图片文件时同时发送位图 + 文件 offer，两个表示绑定同一内容，避免重复复制、
回环与进度状态歧义。对发送端依赖 Wayland 文件复制的限制仍然存在。

## 涉及代码位置

- `crates/m590-daemon/src/hub.rs`：AppliedImage 分支（仅 `write_image`）、
  文件列表/文本路径发送提升逻辑、OLE/FUSE 发布调用点。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`（OLE 虚拟文件）。
- `crates/m590-daemon/src/linux_virtual_file*.rs`（FUSE 文件/tree）。
- `crates/m590-clipboard`：PNG 编码（wire 已有 encoding）、文件解码复用
  `image_from_paths`/`load_image_file`。

## 验证

- 代码级：`cargo test -p m590-core --lib`、`cargo test -p m590-daemon --lib`、
  Windows 交叉 `cargo check --target x86_64-pc-windows-gnu --lib`、clippy/fmt。
- 真机：四个方向 × 图片文件/位图，验证 Word 与文件管理器两种粘贴都可用；回归
  文本、普通文件批次、替换与断线。

## 文档影响

- 已更新：本 task、`docs/plans/current.md`、`AGENTS.md`、根 `Cargo.toml` 版本
  0.1.1 → 0.1.2。
- 不改协议 wire 字段；不改 Hub HTTP API、UI 交互与安装器。

## 风险

- 写完剪贴板后需更新基线，防止把刚生成的双表示内容又回环发给对端。
- 位图→PNG 编码成本与大小上限；动图只取首帧。
- 双表示同时存在时的进度/状态展示单一化。
- 需要 Windows 10 + GNOME Wayland 真机验收四个方向。
