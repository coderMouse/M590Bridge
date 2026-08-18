# task-057 · Windows Explorer 多文件剪贴板粘贴

## 状态

`in_progress`

## 背景

task-043/044 已验证 Windows 单文件 OLE 虚拟剪贴板与按需网络读取。此任务将同一能力
扩展为 Explorer 可识别的多文件 `IDataObject`，目录树语义以 task-055 清单为准。

## 目标

- Windows 发布批次清单对应的虚拟文件集合，Explorer `Ctrl+V` 时按需读取每个文件。
- 文件名、相对目录、大小和取消/替换生命周期与网络批次状态一致。
- 保留系统原生复制进度，不提前把全部文件落盘到接收目录。

## 允许修改

- `crates/m590-clipboard/src/virtual_file.rs`、`src/windows_virtual_file.rs`、`src/lib.rs`：
  虚拟文件集合描述、OLE `IDataObject` 与公开 API
- `crates/m590-clipboard/src/file_paths.rs`、`src/linux.rs`：Linux 发送端多路径文件列表解析与
  Wayland MIME 回退
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`、`src/virtual_file_bridge.rs`：
  Windows STA 发布与逐文件惰性流桥
- `crates/m590-daemon/src/hub.rs` 的 Windows 批次接线
- 必要的 Windows 构建/测试辅助代码
- 本 task 及必要文档

## 禁止修改

- Linux FUSE 虚拟目录树（task-058）。
- 现有单文件 OLE 行为、安装器、自启和断点续传。

## 验证命令

```bash
cargo test -p m590-daemon virtual_file_bridge
cargo test -p m590-clipboard
cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --lib --examples
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples
```

Windows 10 真机：Explorer 多文件、嵌套文件夹、取消、替换、断线，以及重新发送批次后
再次粘贴。

## 完成标准

- [ ] Explorer 能粘贴多文件及嵌套文件夹，内容和相对路径一致。
- [ ] 系统进度、取消、替换和断线均无死锁或残留状态。
- [ ] 单文件回归保持通过；Windows 真机结果已记录。

## 下一步

- 两端拉取当前提交并运行 `desktop:standalone`；从 Linux 文件管理器复制包含多个顶层
  文件和目录的选择，在 Windows Explorer 粘贴。确认发送端路径不再带 `\r` 并出现
  `clipboard_batch_queued`，接收端出现 `batch_received entries>1` 和多个 `GetData.lindex`；
  再覆盖嵌套/空目录、取消、替换、断线及重新发送后的再次粘贴。

## 实施记录

- 2026-08-17：task-056 验收通过后开始开发。复核仓库发现 OLE 数据对象实际位于
  `m590-clipboard`，daemon 负责 STA manager 与网络桥；据此修正允许范围，不修改
  task-058 Linux FUSE、协议字段、安装器、自启或断点续传。
- 2026-08-17：新增虚拟文件集合模型；一个 `FILEGROUPDESCRIPTORW` 可描述文件与目录，
  `FILECONTENTS.lindex` 映射到对应文件描述项，目录只携带目录属性且不暴露内容流。
  Windows 文件名、相对路径、UTF-16 长度、大小写冲突和“文件作为父目录”在发布前拒绝。
- 2026-08-17：Windows STA manager 支持发布/条件替换整个集合；原单文件 API 保留并
  包装为单元素集合，避免改变 task-043/044 已验收调用面。
- 2026-08-17：Hub 在 Windows 收到 `BatchOffered` 后发布 OLE 集合，不进入 `.partial`
  自动落盘。Explorer 打开某个文件流后才请求对应 entry，多个已打开流按现有单连接能力
  串行调度；排队流在其 `FileRequest` 真正发出后才开始读超时计时。
- 2026-08-17：批次取消、远端失败、OLE 发布失败和运行态析构会唤醒所有未完成流并清理
  剩余 entry offer；新 offer、延迟替换和本机剪贴板替换沿用“已开始流先完成”的规则。
- 2026-08-18：按 Win32 Shell 规则让 `EnumFormatEtc` 为每个文件 descriptor 枚举独立的
  `CFSTR_FILECONTENTS` 索引格式；目录项不枚举内容格式，集合条目数限制到可表达的
  `FORMATETC.lindex` 范围。
- 2026-08-18：Windows 10 首轮真机验收不通过：多文件只能粘贴其中一个，嵌套目录和
  空目录无法粘贴。task-057 保持 `in_progress`。对照 Explorer 使用的成熟 `IDataObject`
  实现后确认，`EnumFormatEtc` 应只公开一个 `lindex=-1` 的 `CFSTR_FILECONTENTS` 通配格式，
  具体 descriptor 索引由后续 `GetData` 请求携带；目录集合也必须保留 descriptor/contents
  格式对，即使 Explorer 不会为目录请求内容流。
- 2026-08-18：已将 `EnumFormatEtc` 改为始终枚举一个通配 `CFSTR_FILECONTENTS`；
  `QueryGetData` 接受该能力查询，而 `GetData` 仍只接受 Explorer 提供的具体文件 descriptor
  索引并拒绝目录内容流。新增无网络 Windows 集合探针，固定覆盖两个顶层文件、嵌套文件、
  嵌套空目录和顶层空目录，以便先区分 OLE/Shell 问题与跨机网络调度问题。
- 2026-08-18：用户确认无网络 OLE 集合探针完整通过，但重新运行
  `desktop:standalone` 后跨机行为仍无变化。代码核对确认 Windows 关闭主窗口只会最小化
  到托盘，且桌面端没有单实例保护；旧 `m590-ui.exe` 可继续占用 5910、持有旧 Hub/剪贴板，
  让新启动进程无法运行其内嵌 Hub。为避免后续误判，在 standalone 的 npm 前置脚本增加
  Hub 端口预检：发现旧实例占用时明确报错并停止启动，不自动终止用户进程。
- 2026-08-18：用户在带 5910 预检的 standalone 再次复测，行为仍未改善，并反馈传输体感
  比以前慢；旧实例不是完整解释。检查 `0ee6e65..40504d4` 确认 OLE 枚举和端口预检没有
  修改文件分块、TCP 缓冲、Session 泵或网络流算法；多文件仍按 task-056 的同连接串行策略。
- 2026-08-18：为下一次真机复测加入 feature-gated 诊断：打印收到/发布的完整批次清单与
  descriptor 索引、Explorer `QueryGetData` / `GetData.lindex`、每个 `BridgeEvent::Request`、
  `FileRequest` 发出、首块延迟、完成耗时与 MiB/s；单文件路径也输出同口径速度，便于与
  用户之前的大 MP4 对照。`desktop:standalone` 在 Windows 临时保留控制台；普通打包与
  NSIS 不启用诊断 feature，行为不变。
- 2026-08-18：真机日志确认失败样本并未进入 Windows 批次路径：只有
  `publish_collection entries=1` / `single_ole_stream_request`，transfer id 也是
  `ui-file-*`，没有 `batch_received`。因此 Explorer 只显示一个文件符合其收到的数据，OLE
  集合不是本次丢项位置。对应发送端代码仍明确调用 `first_regular_file(&paths)`，会丢弃
  file_list 的其余顶层路径并跳过单个目录。
- 2026-08-18：修复发送入口：一个普通文件保持已验收的单文件 offer，一个图片保持位图
  同步；多个根路径或任一目录使用 task-056 的安全扫描与 `FileBatchOffer`，保留嵌套/空
  目录。Windows 接收虚拟批次期间也计入 `virtual_clipboard_active`，避免轮询自己发布的
  OLE 集合形成回传。诊断扩展到发送端的 file_list 数量、批次排队和 offer 发出。
- 2026-08-18：用户的大文件日志得到 1,387,616,317B / 302.516s，端到端
  `effective_mib_s=4.37`、首块后 `data_mib_s=4.38`，请求调度仅 2ms、首块 258ms；没有
  发现应用层请求间停顿。本轮未改分块、TCP 缓冲或 Session 泵，需用同方向 `iperf3`
  对照后判断是网络/磁盘环境还是传输管线吞吐。
- 2026-08-18：针对发送端仍可能走文本回退的情况，新增 GNOME/Nautilus 多行
  `text/uri-list` / `x-special/gnome-copied-files` 解析：保留所有现存文件与目录，多个根路径
  或目录统一进入既有 `FileBatchOffer`，单个普通文件仍走原单文件 offer。Wayland 环境在
  `arboard.file_list()` 返回至多一项时，以 500ms 节流直接读取两个原始 MIME，并选择条目数
  更多的完整列表；无 data-control 时仍安全回退到 arboard 与文本路径分支。
- 2026-08-18：用户提供的 Linux/Windows 同次复测日志确认四根路径被检测到，但
  `arboard.file_list()` 的各路径末尾残留 `\r`，导致文件/目录计数均为 0、批次首个 `stat`
  失败；Hub 随后又错误降级为第一个可解析普通文件，所以 Windows 只发布并粘贴了一个
  空文本文件。Linux 现会在初始化与每次轮询时保守清洗文件列表：仅当原路径不存在、清洗
  后路径确实存在时替换，避免改写合法路径；需要批次的选择即使扫描失败也不再降级为部分
  单文件 offer。

## 修改文件

- `crates/m590-clipboard/Cargo.toml`、`src/lib.rs`、`src/virtual_file.rs`、
  `src/windows_virtual_file.rs`：集合模型、Windows 路径校验、多项 OLE descriptor 与按索引
  `IStream`，并保留单文件兼容入口。
- `crates/m590-clipboard/examples/windows_virtual_file_collection.rs`：本机 OLE 多文件、
  嵌套文件、空目录探针。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`：STA 线程发布及条件替换集合。
- `crates/m590-daemon/src/virtual_file_bridge.rs`：网络请求开始前不计算排队流读取超时。
- `crates/m590-daemon/Cargo.toml`、`src/hub.rs`：Windows 批次发布、逐文件惰性请求、串行
  调度、状态和清理；多路径/目录 file_list 自动生成批次并避免接收虚拟批次回传；向
  standalone 透传 task-057 诊断 feature。
- `crates/m590-clipboard/src/file_paths.rs`、`src/linux.rs`：多行 GNOME 文件选择解析、
  目录保留、Wayland 原始 MIME 补读与节流缓存；清洗 `arboard` 文件列表末尾 CR 残留。
- `ui/scripts/prepare-standalone.mjs`：standalone 启动前检查固定 Hub 端口，拒绝与旧实例并行。
- `ui/package.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/src/main.rs`：仅 standalone
  启用 task-057 诊断并在 Windows 显示控制台；正式构建继续使用 windowed subsystem。
- `ui/README.md`：补充 Windows 托盘退出和旧 Hub 端口占用说明。
- `docs/domain/protocol-draft.md`、`docs/discovery/project-map.md`、
  `docs/discovery/commands.md`：同步 Windows 批次运行时、模块职责和真机验收步骤。
- `AGENTS.md`、`docs/plans/current.md`、`项目说明.md`、本 task：同步当前阶段与验收边界。

## 验证结果

- `cargo test -p m590-clipboard`：通过，23 passed。
- `cargo test -p m590-daemon virtual_file_bridge`：通过，daemon lib 5 passed，bin 无失败。
- `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：通过。
- `cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --bins --no-deps -- -D warnings`：
  通过，无 warning。
- `cargo test --workspace`：通过；clipboard 23、core 37、daemon lib 54、daemon bin 1、
  net 21、Tauri lib 8，共 144 个单元测试通过，doc-tests 无失败。
- `cargo clippy -p m590-clipboard -p m590-daemon --lib --bins --no-deps -- -D warnings`：
  通过，无 warning。
- `cargo fmt --all -- --check`、`git diff --check`：通过。
- 首轮 Windows 10 Explorer 真机：不通过；多文件只粘贴一个，嵌套目录和空目录不能粘贴。
- 本轮返工 `cargo test -p m590-clipboard`：通过，23 passed。
- 本轮返工 `cargo test -p m590-daemon virtual_file_bridge`：通过，5 passed。
- 本轮返工 `cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --lib --examples`：
  通过，包含新增集合探针。
- 本轮返工 `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：通过。
- 本轮返工 `cargo clippy -p m590-clipboard -p m590-daemon --target x86_64-pc-windows-gnu
  --lib --bins --examples --no-deps -- -D warnings`：通过，无 warning。
- 本轮返工 `cargo test --workspace`：通过，共 144 个单元测试，doc-tests 无失败。
- 本轮返工 `cargo fmt --all -- --check`、`git diff --check`：通过。
- 修复后 Windows 10 Explorer：待复测；当前 Linux 环境只能交叉编译，不能代替 Shell/OLE
  行为验证。
- Windows 10 无网络集合探针：通过；两个顶层文件、嵌套文件和空目录均可粘贴。
- Windows 10 `desktop:standalone` 首次复测：仍不通过；需排除托盘旧实例/5910 占用后复测。
- `node --check scripts/prepare-standalone.mjs`、`npm run lint -- --deny-warnings`：通过。
- `npm run build`：通过；TypeScript 与 Vite production build 完成。
- standalone 端口占用探测：通过；测试进程占用 5910 时前置脚本非零退出并报告
  `already in use`。
- standalone 空闲端口/Linux 身份准备：通过；在临时数据目录生成 desktop entry 和图标后
  已清理该临时目录。
- 本轮诊断 `cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --lib --examples
  --features task-057-diagnostics`：通过。
- 本轮诊断 `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib --bins --examples
  --features task-057-diagnostics`：通过。
- 本轮诊断 `cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --bins --examples
  --features task-057-diagnostics --no-deps -- -D warnings`：通过，无 warning。
- `cargo check --manifest-path ui/src-tauri/Cargo.toml --features
  custom-protocol,task-057-diagnostics`：通过；本机尝试完整 Tauri Windows GNU 交叉检查时因
  缺少 `x86_64-w64-mingw32-windres` 停在资源构建，Windows 真机构建待本轮复测确认。
- 本轮诊断 `cargo test -p m590-daemon virtual_file_bridge`：通过，5 passed；
  `cargo test -p m590-clipboard`：通过，23 passed。
- 本轮诊断 `cargo test --workspace`：通过，共 144 个单元测试，doc-tests 无失败。
- 本轮诊断 `npm run lint -- --deny-warnings`、`npm run build`、
  `node --check scripts/prepare-standalone.mjs`：通过。
- 本轮 file_list 修复 `cargo test --workspace`：通过；clipboard 23、core 37、daemon lib 55、
  daemon bin 1、net 21、Tauri lib 8，共 145 个单元测试，doc-tests 无失败；新增测试确认
  单文件保留单路，多根路径/单目录选择批次。
- 本轮 file_list 修复 Windows GNU 诊断交叉检查：`m590-clipboard --lib --examples` 与
  `m590-daemon --lib --bins --examples` 均通过。
- 本轮 file_list 修复本机与 Windows GNU Clippy：通过，`-D warnings` 无 warning；
  `cargo fmt --all -- --check`、`git diff --check`：通过。
- 本轮 Linux 发送端兼容修复 `cargo test -p m590-clipboard`：通过，24 passed（新增多行
  文件/目录解析测试）。
- 本轮 Linux 发送端兼容修复 `cargo test -p m590-daemon virtual_file_bridge`：通过，5 passed。
- 本轮兼容修复 `cargo test --workspace`：通过；clipboard 24、core 37、daemon lib 55、
  daemon bin 1、net 21、Tauri lib 8，共 146 个单元测试通过，doc-tests 无失败。
- 本轮兼容修复 `cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --lib --examples`
  与 `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：通过，无 warning。
- 本轮兼容修复 `cargo clippy -p m590-clipboard -p m590-daemon --lib --bins --no-deps -- -D warnings`、
  `cargo fmt --all`：通过。
- 本轮 CR 路径修复 `cargo test -p m590-clipboard`：通过，25 passed；新增测试确认存在的
  文件/目录会清除末尾 `\r`，不存在的路径保持原样。
- 本轮 CR 路径修复 `cargo test -p m590-daemon virtual_file_bridge`：通过，5 passed。
- 本轮 CR 路径修复 `cargo test --workspace`：通过；共 147 个单元测试通过，doc-tests 无失败。
- 本轮 CR 路径修复 `cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --lib
  --examples` 与 `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：通过，
  无 warning。
- 本轮 CR 路径修复 `cargo clippy -p m590-clipboard -p m590-daemon --lib --bins --no-deps --
  -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：通过。

## 文档影响检查

- 已更新：本 task、当前计划、`AGENTS.md` 与项目说明，记录 CR 路径根因、禁止批次失败后
  部分降级及新的真机复测门槛。
- 历史已更新：协议草案、项目结构图、UI 规格、Windows 验收命令、standalone 旧实例预检
  与 feature-gated 诊断说明。
- 无需更新：本轮未改变协议字段、Hub HTTP API、UI 交互、安装器、模块职责或 Linux FUSE。

## 风险 / blocker

- Windows 10 首轮真机验收已确认多文件/目录行为失败；修复后仍需 Explorer 复测才能
  标记完成。
- 失败日志先后定位到发送端只发布第一项，以及多路径末尾 `\r` 使批次扫描失败后又降级为
  单文件；两处均已修复。Windows Explorer 端到端结果仍须真机复测，不能以本机 OLE 探针
  或交叉编译替代。
- Linux Wayland 原始 MIME 补读依赖 compositor 的 data-control；没有该协议时只能依赖
  arboard/X11 可见内容或多行文本回退。本轮已修复 arboard 多路径末尾 `\r`，但仍需真机
  确认发送端进入 `clipboard_batch_queued`，不能用单元测试替代桌面剪贴板行为。
- 大文件实测为 4.37 MiB/s，应用请求/首块等待只占约 0.26s；源码比较未发现本轮修改传输
  算法。需以同方向 `iperf3` 和本机磁盘读写对照后再决定是否需要吞吐修复。
- 沿用 task-044 的一次性网络 offer：同一剪贴板 offer 完成后再次 `Ctrl+V` 仍不保证可重开
  `FILECONTENTS`；本 task 验证“重新发送同批输入后再次粘贴”。若产品要求同一 offer 任意
  次粘贴，需要另建任务扩展发送源保留与重复请求协议生命周期。
