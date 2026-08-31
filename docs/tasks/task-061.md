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

## 真机 trace 结论与修复（2026-08-31）：无人请求位图格式，补 `CF_DIB`

### trace 读到的事实（逐条核对 `win-trace.txt`，非推测）

HRESULT 对照取自 `windows-0.61.3` 的 `Win32/Foundation/mod.rs` 常量定义：
`0x8004006A`=`DV_E_CLIPFORMAT`、`0x80040069`=`DV_E_TYMED`、
`0x8004006B`=`DV_E_DVASPECT`。

1. **`Ctrl+V` 后确实有大量 `get_data_request`**（约 25 次/场景），消费者把
   `CF_UNICODETEXT`(13)、`CF_HDROP`(15) 和一批注册格式挨个问了一遍，其中 49 次
   得到 `DV_E_CLIPFORMAT`（我们没提供该格式，属正常）。
2. **零次 `cf=2`(`CF_BITMAP`) / `cf=8`(`CF_DIB`) / `cf=17`(`CF_DIBV5`) 请求。**
   `grep -cE "cf=(2|8|17) "` 结果为 0。我们枚举了 `dibv5=17`，但没有任何消费者
   来取它。
3. **我们自己的 `PNG`(49330) 被以 `tymed=0x4`(`TYMED_ISTREAM`) 请求过 3 次，
   均回 `DV_E_TYMED`**（我们只提供 `TYMED_HGLOBAL`）。但在
   `imgfile-35227-6` 那次，调用方随后用 `tymed=0x1` 重试并成功
   （`get_data kind=png bytes=87432`）—— 这是探测回退模式，**不是致命错误**，
   因此本次不动 tymed 处理。
4. Explorer 侧 `descriptor` + `contents lindex=0` 取数正常，与「Explorer 粘贴得到
   `.png` 成功」一致。

### 本次 trace 的局限（必须记下，避免下次误判）

场景 B（复制图片文件）里**同样没有任何位图格式请求**，trace 里也没有任何
能证明 Word 成功插入图片的痕迹。也就是说 **这份 trace 无法证实「场景 B 在
Word 里成功」**；此前那条观察可能早于 OLE 化改动。下次真机验证需要同时回报
Word 里的肉眼结果，不能只看日志。

### 根因判断

数据对象只提供 `CF_DIBV5`(17) 与注册格式 `PNG`。`CF_DIBV5` 是较晚引入的扩展
格式，Word 的图片粘贴路径不来取它（事实 2）；注册的 `PNG` 虽然被取走过，但那
是文件/通用路径，不是 Word 的「插入图片」格式。缺的是**最经典的
`CF_DIB`(8)** —— 位图消费者普遍认它。

这个判断的强度：事实 2 是硬证据（没人要 17），补 `CF_DIB` 是最小且方向明确的
一步；但**它是否足以让 Word 可用，只有真机能确认**。

### 改动

- `crates/m590-clipboard/src/lib.rs`：新增 `ImageClipboard::to_dib_bytes()`，
  产出 40 字节 `BITMAPINFOHEADER` + bottom-up BGRA（`BI_RGB`、32bpp、正高度）。
  与 `to_dibv5_bytes()` 共用新抽出的 `append_bottom_up_bgra()`，两种格式的像素
  块保证逐字节一致。
- `crates/m590-clipboard/src/virtual_file.rs`：`VirtualFileCollection` 增
  `dib` 字段与 `dib_bytes()`；`single_image()` 签名变为
  `(file, dib, dib_v5, png)`。
- `crates/m590-clipboard/src/windows_virtual_file.rs`：`ClipboardFormats` 增
  `dib: CF_DIB.0`；`as_format_etc()` 由 5 项增至 6 项；
  `RequestedFormat::Dib` 分支接入 `requested_format` / `GetData` /
  `QueryGetData`；诊断的 `format_ids`、`enum_format_etc`、
  `publish_collection_image` 三行都带上 `dib`，便于下轮 trace 判读。
- `crates/m590-daemon/src/hub.rs`：`image_file_collection()` 额外算 `dib` 并传入
  `single_image()`。

`CF_DIBV5` 与 `PNG` 都保留：DIBv5 带 alpha 信息，认它的消费者能拿到更好结果。
枚举顺序把 `CF_DIB` 放在 `CF_DIBV5` 前面，但**这一点没有 trace 证据支持**
（没人请求过 17，顺序很可能无关），只是倾向广泛兼容的那个格式。

### 代码级验证（2026-08-31 实际运行）

- `cargo test -p m590-clipboard --lib` 27 通过（新增
  `dib_layout_matches_windows_bitmap_consumer`：断言 40 字节头各字段，并断言像素
  块与 `to_dibv5_bytes()[124..]` 逐字节相同）。
- `cargo test -p m590-daemon --lib` 74 通过（2 ignored）；
  `cargo test -p m590-core --lib` 41 通过。
- `cargo clippy -p m590-core -p m590-daemon -p m590-clipboard --lib --no-deps
  -- -D warnings` 通过；Windows 交叉
  `cargo clippy -p m590-clipboard --lib --no-deps --features task-057-diagnostics
  --target x86_64-pc-windows-gnu -- -D warnings` 通过；
  `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib --features
  task-057-diagnostics` 通过；`cargo check --workspace`、`cargo fmt --check`、
  `git diff --check` 通过。
- **未做真机验证（本机 Ubuntu 无 Windows 运行条件）** —— 这是 blocker：
  `CF_DIB` 是否真的让 Word 的粘贴可用，只能在 Windows 上确认。

### 待用户真机确认

按前一节同样方式启动（`desktop:standalone` 已默认带诊断），跑场景 A：
Linux `prtsc` → Windows 在 Word `Ctrl+V`。**请同时回报 Word 里的肉眼结果**
（见上文「局限」）。判读：

- 预期出现 `get_data_request cf=8 ...` 紧随 `get_data kind=dib bytes=..`，
  且 Word 里出现图片 → 修复成立。
- 若出现 `cf=8` 请求但 Word 报错/图像错乱 → 转为像素布局问题（高度符号、
  `BI_RGB` 下 alpha 字节的处理）。
- 若**仍无任何 `cf=8`/`cf=2` 请求** → 说明问题不在格式列表，下一步转向
  `CF_BITMAP`(2, 需 `TYMED_GDI` + HBITMAP) 或 `OleSetClipboard` 之后的
  剪贴板所有权/延迟渲染时序；可用
  `publish_collection_image dib_bytes=..` 行先确认净荷确实挂上了。

## 文档影响检查（2026-08-31，第二次）

- 已更新：本 task（trace 逐条核对结论、`CF_DIB` 修复、trace 局限、待确认判读）。
- 待更新：`docs/plans/current.md` —— pending 问题状态。
- 无需更新：协议 wire、Hub HTTP API、UI 交互、安装器 —— 均未触及；
  `docs/discovery/commands.md` 未改命令或 feature 接线。

## 第二轮真机 trace（2026-08-31）：`CF_DIB` 无效，且此前对「谁在读」的归因是错的

### 事实（逐条核对新 `win-trace.txt`，166 行）

1. **`CF_DIB` 确实挂上了**：`format_ids ... dib=8 dibv5=17 png=49330`，
   `publish_collection_image dib_bytes=16367404 dibv5_bytes=16367488
   png_bytes=3696139`。净荷与格式表都没问题。
2. **仍然零次 `cf=8` / `cf=2` / `cf=17` 请求**（`grep -cE "cf=(2|8|17) "` = 0）。
   补 `CF_DIB` **对 Word 的粘贴没有任何作用**。
3. **`QueryGetData` 被调用 0 次**（`grep -c query_get_data` = 0）。Word 判断
   「粘贴是否可用」必然先经 `QueryGetData`，一次都没有。
4. **场景 A 与场景 B 的 `GetData` 请求序列几乎逐字节相同**：各 23 / 24 条，
   仅差一条 `cf=13`。但用户报告 B 在 Word 里能贴、A 不能 —— 相同的请求序列
   不可能导出相反结果。
5. 序列内容为 `cf=49397`(PreferredDropEffect)×14、`descriptor`、
   `contents lindex=0`，以及一批 shell 专有格式（49333/49341/49414/49416…）。

### 归因修正（重要）

**此前把这些 `GetData` 当作「Word 的读取」是错的。** 事实 3+4+5 指向同一结论：
这串请求是 **Explorer / 剪贴板监视器**的指纹，与 Word 无关。Word 很可能
**从未接触过我们的数据对象** —— 它在更早的环节就没看到可贴的位图内容，
所以数据对象的格式表里放什么都不会被读到。这解释了为什么两轮「补格式」
（DIBv5 → 再补 CF_DIB）都完全无效：**方向本身错了**，不是格式选错了。

### 本轮改动：只补一条能证伪的诊断（不改运行行为）

不再猜格式。发布后立刻在本进程枚举**系统剪贴板**真实暴露的格式列表
（`OpenClipboard(None)` + `EnumClipboardFormats` + `GetClipboardFormatNameW`），
输出 `system_clipboard_formats count=.. ids=..`。

- `OpenClipboard(None)` 传 None，本线程不会成为 clipboard owner，不干扰刚建立
  的 OLE 所有权；任何路径都 `CloseClipboard`；打不开时输出
  `system_clipboard_formats open_failed err=..` 而不是静默。
- 整个函数在 `#[cfg(feature = "task-057-diagnostics")]` 下，正式构建是空 stub
  （已验证两种 feature 组合的 Windows 交叉 clippy 均 `-D warnings` 通过）。

判读逻辑（这是补它的唯一目的）：

- **列表里没有 8/17** → 格式根本没落到系统剪贴板上。问题在**发布环节**，
  最可能是延迟渲染的 OLE 对象未 `OleFlushClipboard`，跨进程枚举不到实体格式。
- **列表里有 8/17** → 格式系统级可见，问题在 Word 侧的读取条件（此时才值得
  查 `CF_BITMAP`/`TYMED_GDI`、DVASPECT、或 Word 对 OLE owner 的额外要求）。

### 代码级验证（2026-08-31 实际运行）

- `cargo test -p m590-clipboard --lib` 27 通过；`m590-daemon --lib` 74 通过
  （2 ignored）；`m590-core --lib` 41 通过。
- Windows 交叉 clippy **两种 feature 组合**均 `-D warnings` 通过：
  `--features task-057-diagnostics` 与不带该 feature（确认 stub 不触发
  unused 警告）。
- `cargo clippy -p m590-core -p m590-daemon -p m590-clipboard --lib --no-deps
  -- -D warnings`、`cargo check -p m590-daemon --target x86_64-pc-windows-gnu
  --lib --features task-057-diagnostics`、`cargo check --workspace`、
  `cargo fmt --check`、`git diff --check` 均通过。
- 未做真机验证（本机 Ubuntu 无 Windows 运行条件）。本轮**不含任何行为修复**，
  只为下一轮定位提供证据。

### 待用户真机确认（第三轮）

跑场景 A（Linux `prtsc` → Windows 在 Word `Ctrl+V`），回传 trace。只需看一行：
`system_clipboard_formats count=.. ids=..`。按上面两条分支判读即可确定下一步是
修发布（`OleFlushClipboard`）还是修读取条件。

**另需你确认一件仍未澄清的事**：场景 B（复制图片文件）在 Word 里现在到底能不能
贴？事实 4 显示两个场景的读取序列几乎相同，若「B 能贴」这条实际不成立，
则问题范围应改为「所有 OLE 图片路径对 Word 均不可用」，方向会跟着变。

## 文档影响检查（2026-08-31，第三次）

- 已更新：本 task（第二轮 trace 事实、归因修正、系统剪贴板枚举诊断）、
  `docs/plans/current.md`。
- 无需更新：协议 wire、Hub HTTP API、UI 交互、安装器 —— 未触及；
  `docs/discovery/commands.md` —— 未改命令或 feature 接线。

## 下一步

用户跑第三轮场景 A，回报 `system_clipboard_formats` 一行 + 场景 B 在 Word 的
实际结果。

## 第三轮真机 trace（2026-08-31）：两场景均通过；发布环节被证明是对的，但 fail→pass 尚无代码解释

用户回报：**场景一和场景二都测试通过了**（trace 275 行，三次 publish）。

### 事实 1 — 系统剪贴板确实暴露了位图格式（发布环节洗清）

三次发布输出完全相同的一行：

```
system_clipboard_formats count=9 ids=49161(DataObject) 49393(FileGroupDescriptorW)
  49342(FileContents) 49397(Preferred DropEffect) 8 17 49330(PNG)
  49171(Ole Private Data) 2
```

- `8`(CF_DIB)、`17`(CF_DIBV5) **在系统剪贴板上跨进程可见** → 第二轮设想的
  「延迟渲染未 `OleFlushClipboard`、格式没落地」这条分支**证伪**，发布路径正确，
  不需要 `OleFlushClipboard`。
- `2`(CF_BITMAP) 排在 `Ole Private Data` **之后**，即系统由 `CF_DIB`
  **合成**的格式（合成格式在 `EnumClipboardFormats` 中排在实际写入的格式之后）。
  Word 需要的位图形式是齐的。

### 事实 2 — 「Word 是否真的读了」有了可判别的指纹

第二轮把那串 `GetData` 整体归给「Explorer/监视器」是**过粗**的。把两轮 trace 按
publish 切段后统计（脚本按 `get_data_request cf=.. tymed=..` 聚合）：

| trace | 段 | 请求数 | `cf=13` | TYMED_ISTORAGE(0x4) | OLE 内嵌格式 |
|---|---|---|---|---|---|
| 第二轮 | 段1（prtsc，**Word 失败**） | 23 | 0 | 0 | 无 |
| 第二轮 | 段4（图片文件，**Word 成功**） | 38 | 3 | 6 | 49933/49935/49936/49938 |
| 第三轮 | 段1/2/3（**全部成功**） | 各 37 | 各 3 | 各 5 | 49933/49935/49936/49938 |

以 `TYMED_ISTORAGE` 探测 `49933/49935/49936/49938`（Embed Source / Object
Descriptor 一类 OLE 内嵌协商）+ `cf=13`(CF_UNICODETEXT) 作为「**OLE 容器
（Word）真的接触了数据对象**」的指纹，它与用户报告的成功/失败**在 7 个观测点上
完全一致**：失败的那段没有这个指纹，成功的每一段都有。

所以第二轮的核心判断成立且现在更精确：**当时的失败不是「Word 读了但拒绝」，
而是「Word 根本没读」**；本轮 Word 读了，于是能贴。

### 事实 3 — 即便成功，Word 也从不向我们的 `IDataObject` 要位图

**全部 7 段（含三段成功）中 `cf=2/8/17` 的请求数均为 0。** 结合事实 1：Word 的
位图是直接从**系统剪贴板**取的，不经过 `IDataObject::GetData`。这解释了为什么
前两轮盯着 `GetData` 日志找位图请求永远找不到 —— 那里本来就不会有。

### 事实 4（未解决）— 本轮没有任何行为改动，fail→pass 无代码解释

第三轮提交 `1e78f4f` **只加了诊断**（`log_system_clipboard_formats`），未改发布
逻辑。因此「上一轮失败、这一轮成功」目前**不能由我们的改动解释**。两种可能：

- **(a) 第二轮那次失败是测试侧偶发**（构建/焦点/时序），`CF_DIB`（`2947157`）
  实际已经修好或本就不需要；
- **(b) 诊断本身是 load-bearing**：它在 `OleSetClipboard` 之后立刻
  `OpenClipboard(None)` / `CloseClipboard`，占住了发布后的那个窗口。

(b) 有一条 trace 级佐证：第二轮**失败的那一段**里，本地轮询
`[task-057][hub] clipboard_file_list_detected roots=0 files=0 directories=0`
恰好落在 `format_ids` **紧后面**（即发布窗口内，且它读到的是「空」）；第三轮三次
发布的同一位置都换成了我们的 `system_clipboard_formats`，全程没有轮询插入。

代码侧确有对应缺口：`hub.rs:4770-4781` 只在 `virtual_receive` /
`virtual_batch_receive` 活跃时抑制轮询，而**图片双表示 OLE 发布这两者都不置位**，
因此发布后本地轮询不受抑制，可以立刻回读剪贴板并与发布窗口竞争。

### 由此产生的风险（尚未验证）

`log_system_clipboard_formats` 只在 `task-057-diagnostics` 下编译，**正式构建是
空 stub**。用户三轮真机跑的都是 `--features custom-protocol,task-057-diagnostics`。
若 (b) 成立，**正式构建仍然是坏的**，而我们会误以为已修好。

### 判定下一步的最小实验

用**不带诊断 feature** 的构建跑场景 A（Linux `prtsc` → Windows 在 Word `Ctrl+V`）。
注意 `ui/package.json` 的 `desktop:standalone` **把 `task-057-diagnostics` 写死在
脚本里**，加 `--` 传参不会去掉它，必须绕开该脚本直接跑：

```
cd ui
node scripts/prepare-standalone.mjs
npm run build
cargo run --manifest-path src-tauri/Cargo.toml --release --features custom-protocol
```

- **能贴** → (a) 成立，task-061 的 Windows 侧可判通过，诊断可择期收敛。
- **不能贴** → (b) 成立，问题是发布窗口竞争，按上面的代码缺口做确定性修复
  （发布期间抑制本地轮询 / 在正式构建里也加一次发布后同步屏障），而不是继续补格式。
