# task-061 · 图片与图片文件的「双表示」粘贴（位图 + 虚拟图片文件）

## 状态

`in_progress`（版本 0.1.2。先实施方案 A：接收端物化。Windows 双表示 + Linux 自动收图
初版实现完成，代码级验证通过；待 Windows 10 / GNOME Wayland 真机验收，见文末）

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

## 实现记录（2026-08-28，初版完成）

### Windows：收到位图 → 双表示（OLE 对象同时 serve 三种用途）

- `m590-clipboard` 新增 `ImageClipboard::to_dibv5_bytes()`：按 arboard 写 `CF_DIBV5`
  的字节布局生成 `BITMAPV5HEADER`（124B、`BI_BITFIELDS`、RGBA 掩码、`LCS_sRGB`、
  正高度 bottom-up、像素 BGRA `[b,g,r,a]`），保证 Word/WordPad 可贴且回读指纹一致。
- `VirtualFileCollection::single_image` 挂载 DIBv5 + PNG 双净荷；`windows_virtual_file.rs`
  的 OLE `IDataObject` 新增 `CF_DIBV5` 与已注册 `PNG` 两个 `FORMATETC`，并新增
  `hglobal_medium_from_bytes` 提供 HGLOBAL 净荷。
- `hub.rs` AppliedImage（Windows 段）：`clip.write_image` 写位图 + 发布
  `image_file_collection`（虚拟 `image-<content_id>.png` + DIBv5 + PNG）。Explorer
  `Ctrl+V` 可粘贴成图片文件，Word 可贴位图；轮询经已注册 `PNG` 读取，无回环。

### Linux：收到单图片文件 offer → 自动解码写剪贴板位图

- `hub.rs` 新增 `LinuxAutoImageReceive` 与 `AUTO_IMAGE_DECODE_MAX_BYTES = 32MiB`：
  收到单文件 offer（可解码图片扩展名 + ≤32MiB + `auto_sync` + 无在途虚拟流）时自动
  `request_file` 下载，`Applied` 后 `load_image_file` 解码 → `write_image` → 删临时文件。
- `gif`/`bmp` 解码 feature 已开启（GIF 取首帧）；`tif/tiff` 不在解码集，保持原文件
  粘贴语义不回归。
- `m590-clipboard` 新增 `is_decodable_image_path` 供门控判断。

### 代码级验证

- `cargo test -p m590-clipboard --lib` 26 通过（新增
  `dibv5_layout_matches_windows_bitmap_consumer`，按标准 Windows 消费方语义断言头部
  字段与像素字节布局，不用 `BmpDecoder` 往返——见下方说明）。
- `cargo test -p m590-daemon --lib` 74 通过；`cargo test -p m590-core --lib` 41 通过。
- `cargo clippy -p m590-core -p m590-daemon -p m590-clipboard --lib --no-deps -- -D warnings`、
  `cargo check --workspace`、Windows 交叉 `cargo check -p m590-daemon
  --target x86_64-pc-windows-gnu --lib`、`cargo fmt --check`、`git diff --check` 均通过。

### 说明与待真机验证

- **为什么单测不用 `BmpDecoder` 往返**：image 0.25.7 起（PR #2552）为兼容真实 Windows
  DIBv5 dump（头后残留 12 字节掩码段）在 V4/V5 `BI_BITFIELDS` 时固定 `+12` seek；
  workspace 锁定 image 0.25.10，会跳过标准布局 DIB 的前 12 字节像素而报
  `UnexpectedEof`。我们 serve 给 Word 的必须保持标准布局（像素紧跟 124B 头），故测试
  改为按标准消费方语义解析。`arboard` 读回优先走已注册 `PNG`，不受影响。
- **待 Windows 10 真机验收**：① 收到位图后 Word/WordPad 可贴、Explorer `Ctrl+V` 可
  粘贴成 `.png` 图片文件；② 剪贴板轮询无回环；③ 四个方向回归（文本/普通文件/批次/
  替换/断线）。
- **待 Linux（GNOME Wayland）真机验收**：Windows 复制图片文件 → Linux 粘贴到 Word
  （LibreOffice）与文件管理器；依赖 GNOME 把位图物化成文件以保留“粘贴成文件”。
  若真机不满足，再补双表示（FUSE 文件 + 位图并存）。
- 已知上游：image ≥0.25.7 对「仅 DIBv5（无 PNG）」的外部剪贴板读取存在 +12 偏移
  误读风险（与本改动无关，属既有行为）；真机观察 Word/浏览器复制图片是否正常。

## 修复记录（2026-08-29）：prtsc 位图后 OLE 发布失败 / 之后文件复制阻塞

### 用户真机问题

Linux prtsc 截图 → Windows 能贴 Word、无法粘贴成图片文件；随后 Linux 向
Windows 复制任何文件都报 `OLE publish failed: ... OpenClipboard 失败
(0x800401D0)`，直到 Windows 本地随便复制一个文件才恢复。

### 根因

位图分支原来先 `clip.write_image`（arboard 裸 `EmptyClipboard` +
`SetClipboardData`，hub 线程），再 `OleSetClipboard`（OLE STA 线程）。当剪贴板
正被我们自己的 OLE 对象持有（例如早前的文件 offer）时，用裸 `EmptyClipboard`
覆盖 OLE owner 会破坏 ole32 内部 owner 状态：之后每次 `OleSetClipboard` 都返回
`CLIPBRD_E_CANT_OPEN`（0x800401D0），直到外部程序重新复制（`EmptyClipboard`）
才重置。文件 offer 之间用 OLE 替换 OLE 不触发（task-045/046 已验证）。

### 修复

1. **Windows 位图接收改为单一 OLE 发布**：直接发布双表示对象（serve
   `CF_DIBV5` + 已注册 `PNG` + 虚拟 `.png`），不再先裸写；回读指纹由
   `ClipboardService::adopt_image_baseline` 登记（Windows/Linux 实现均登记
   `last_image_fp`），无回环。OLE 发布失败时才回退 `write_image`（保 Word 粘贴）。
2. **OLE 发布对 `CLIPBRD_E_CANT_OPEN` 短暂重试**（10 次 × 25ms，等效 arboard
   的 5×5ms 打开重试），吸收瞬时竞争；`task-057-diagnostics` 记录每次重试。

### 代码级验证

- `cargo test -p m590-clipboard --lib` 26 通过；`cargo test -p m590-daemon --lib`
  74 通过；Windows 交叉 `cargo check --target x86_64-pc-windows-gnu --lib`、
  clippy `-D warnings`、fmt、`git diff --check` 通过。
- 待真机复测：prtsc → Windows 贴 Word + 粘贴成图片文件；随后文件复制恢复正常。

## 剩余问题记录（2026-08-29）：prtsc 位图 → Windows 可粘贴成图片文件，无法粘贴到 Word

### 状态

`pending`（用户指示：先记录，等待下次再开发；本次不改代码）。

### 用户真机复测结果

上一轮修复（单一 OLE 发布 + `CLIPBRD_E_CANT_OPEN` 重试）后：

- Linux prtsc 截图 → Windows **能粘贴成图片文件**（Explorer 拿到 OLE 虚拟
  `.png`，说明 OLE 发布已成功）；
- 但 **无法粘贴到 Word**；
- 之前「复制图片文件 → Windows 可贴 Word」场景通过，说明 OLE serve 位图格式
  的路径在部分场景可用。

与上一轮的“能贴 Word、不能贴文件”正好对调：问题已从 OLE 发布环节转移到
「Word 从 OLE 数据对象读取位图格式（`CF_DIBV5` / 已注册 `PNG`）」环节。

### 下次开发的待查方向（未验证，仅供参考）

1. **抓取 Word 的读取请求**：用 `task-057-diagnostics` 确认 Word/WordPad 从
   OLE 对象请求的是哪个格式（`get_data kind=dibv5 / kind=png`）、tymed、lindex，
   以及 GetData 是否返回错误（如 `DV_E_TYMED`）。
2. **对比通过的场景**：“复制图片文件 → Word 可贴”与 prtsc 的差异（内容来源、
   尺寸、PNG/DIB 大小），确定是否与格式枚举顺序或内容大小有关。
3. **候选尝试**（真机验证为准）：
   - OLE 对象同时 serve `CF_DIB`（BITMAPINFOHEADER 风格）以兼容 Word 的枚举；
   - 保持标准 DIBv5 布局（不要套用 image 0.25.10 的 +12 假设，Word 按标准读）；
   - 若 OLE GetData 路径对 Word 不可靠，可评估「OLE 发布 + 有条件地补一次
     arboard 裸写」的组合，但需避免上一轮的 ole32 owner 破坏问题。

## 诊断准备（2026-08-31）：为真机抓取 Word 读取请求补日志

### 背景

上面「待查方向 1/2」需要真机日志。检查现有诊断输出后发现三个盲点，会让
Word 侧的失败无法区分，因此先只补日志（`task-057-diagnostics` feature 内，
不改任何运行行为），再请用户在 Windows 真机复现。

### 盲点与补丁

1. **`GetData` 拒绝路径无日志**：原先 `requested_format` 出错直接 `?` 返回，
   trace 里「Word 请求了但被拒」与「Word 根本没请求」完全同形。现补
   `get_data kind=rejected hr=0x........`（含 `DV_E_TYMED` / `DV_E_CLIPFORMAT`
   等具体 HRESULT）。
2. **`get_data_request cf=<数字>` 无法对应格式名**：已注册格式（`PNG`、
   FILEDESCRIPTOR 等）的 id 是运行时动态分配的。现在 `EnumFormatEtc` 时输出
   `format_ids descriptor=.. contents=.. preferred_drop_effect=.. dibv5=.. png=..`，
   可把后续 `cf=` 数字映射回格式。
3. **无法从日志区分 prtsc 与图片文件两种来源**：现 `publish_collection` 追加
   `publish_collection_image dibv5_bytes=.. png_bytes=..`，用于比对两种场景的
   净荷大小（待查方向 2）。

### 修改文件

- `crates/m590-clipboard/src/windows_virtual_file.rs`：`GetData` 拒绝日志、
  `EnumFormatEtc` 输出格式 id 表、`publish_virtual_file_collection` 输出双表示
  净荷大小。三处均在 `task-057-diagnostics` 下，正式构建不受影响。
- `crates/m590-clipboard/src/file_paths.rs`：修复测试辅助 `temp_file` 的并行竞态
  （见下）。

### 顺带修复：验证命令本身 flaky

`cargo test -p m590-clipboard --lib` 是本 task 的验证命令，但它偶发失败
（12 次里 1 次）：`local_paths_from_text_keeps_multiline_files_and_directories`
断言只剩 `nested` 一项。根因是测试辅助 `temp_file` 只用纳秒命名临时目录，
并行线程取到同一纳秒时共用同一目录，另一个测试的 `remove_dir_all` 删掉了
本测试的文件。改为 `pid + 原子递增序号 + 纳秒`（新增 `unique_suffix()`，
`bare_desktop_name_resolves_under_home_desktop` 的 HOME 目录同样改用）。
修复后连跑 30 次全通过。仅测试代码，不影响产品行为。

### 代码级验证（2026-08-31 实际运行）

- `cargo test -p m590-clipboard --lib` 26 通过；连续 30 次全通过（0 失败）。
- `cargo test -p m590-daemon --lib` 74 通过（2 ignored）；
  `cargo test -p m590-core --lib` 41 通过。
- `cargo clippy -p m590-core -p m590-daemon -p m590-clipboard --lib --no-deps
  -- -D warnings` 通过；Windows 交叉
  `cargo clippy -p m590-clipboard --lib --no-deps --features task-057-diagnostics
  --target x86_64-pc-windows-gnu -- -D warnings` 通过；
  `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib --features
  task-057-diagnostics` 通过；`cargo check --workspace`、`cargo fmt --check`、
  `git diff --check` 通过。
- 未做真机验证（本机 Ubuntu 无 Windows 运行条件）。

### 用户真机复现步骤（Windows 侧抓日志）

`desktop:standalone` 已默认启用诊断：`ui/package.json` 的
`desktop:standalone` 带 `--features custom-protocol,task-057-diagnostics`，
`ui/src-tauri/src/main.rs` 在该 feature 下不隐藏 Windows 控制台。**无需改命令
参数**，只需把控制台输出重定向到文件：

Windows（PowerShell，在仓库 `ui` 目录）：

```powershell
npm run desktop:standalone 2>&1 | Tee-Object -FilePath ..\win-trace.txt
```

Linux（仓库 `ui` 目录）：

```bash
npm run desktop:standalone 2>&1 | tee ../linux-trace.txt
```

复现顺序（每步之间停 2 秒，便于时间线对齐）：

1. 两端启动并配对成功。
2. **场景 A（当前失败）**：Linux 按 `prtsc` 截图 → 切到 Windows，先在
   Word 里 `Ctrl+V`（预期失败），再在 Explorer 里 `Ctrl+V`（预期成功得到
   `.png`）。
3. **场景 B（此前通过，作对照）**：Linux 复制一个 `.png` 图片文件 → Windows
   在 Word 里 `Ctrl+V`（预期成功）。
4. 把 Windows 的 `win-trace.txt` 交回。

日志里需要关注（Windows 侧）：

- `format_ids ...`：把后面的 `cf=` 数字映射到格式名。
- `publish_collection_image dibv5_bytes=.. png_bytes=..`：场景 A 与 B 的净荷大小差异。
- `get_data_request cf=.. lindex=.. tymed=0x..` 与紧随的
  `get_data kind=dibv5|png|rejected`：Word 请求了什么、是否被拒、HRESULT 是多少。
- 若 Word 的 `Ctrl+V` 后**完全没有** `get_data_request`：说明 Word 在
  `EnumFormatEtc`/`QueryGetData` 阶段就没选中位图格式，方向转为格式枚举顺序
  或补 `CF_DIB`（待查方向 3）。

## 文档影响检查（2026-08-31）

- 已更新：本 task（诊断准备、flaky 修复、真机复现步骤）、`docs/plans/current.md`。
- 无需更新：`docs/discovery/commands.md` — `desktop:standalone` 已记录会启用
  `task-057-diagnostics` 并保留 Windows 控制台，本次未改命令或 feature 接线。
- 无需更新：`AGENTS.md` 阶段描述 — 功能状态未变，pending 问题仍未解决。
- 无需更新：协议 wire、Hub HTTP API、UI 交互、安装器 — 均未触及。

## 下一步

用户在 Windows 真机按上面步骤跑场景 A + B，回传 `win-trace.txt`；据此判断是
补 `CF_DIB`、调整格式枚举顺序，还是改 GetData 的 tymed 处理。
