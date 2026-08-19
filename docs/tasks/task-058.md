# task-058 · Linux FUSE 虚拟目录树

## 状态

`in_progress`

## 背景

task-052 已验证 Linux FUSE 单文件按需读取和 Nautilus 系统进度。此任务在不改变单文件
行为的前提下，把批次清单投影为只读虚拟目录树。

## 目标

- 收到批次清单后发布一个可浏览的只读 FUSE 目录树，不在首次发布时下载文件内容。
- Nautilus 读取具体文件时才为对应 entry 发起单文件请求，目录结构和路径严格来自已
  校验清单。
- 支持取消、替换、断线和挂载清理；保留 Nautilus 原生复制进度。

## 允许修改

- `crates/m590-daemon/src/linux_virtual_file.rs`
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`
- `crates/m590-daemon/src/hub.rs` 的 Linux 批次接线
- `crates/m590-daemon/src/virtual_file_bridge.rs` 的 Linux 非阻塞背压入口与回归测试
- 必要的剪贴板 URI 发布辅助代码
- 本 task 及必要文档

## 禁止修改

- Windows OLE 多文件对象（task-057）。
- task-055 已定的 wire 字段和路径规则。
- 断点续传、并行读取和 Android/macOS 支持。

## 验证命令

```bash
cargo test -p m590-daemon virtual_file
cargo test -p m590-daemon linux_virtual
cargo test -p m590-daemon mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure -- --ignored --nocapture
cargo check --workspace
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
```

Linux GNOME/Wayland + Nautilus 真机：浏览嵌套目录、粘贴多个文件、取消、替换、断线和
重复粘贴，并校验路径与内容。

## 完成标准

- [ ] 目录树可浏览，文件按需读取，内容和相对路径一致。
- [ ] 生命周期清理和单文件回归通过；Linux 真机结果已记录。
- [ ] 不因恶意清单创建绝对路径或路径穿越。

## 下一步

- 两端更新到本次活跃流剪贴板轮询修复后，先复测 Windows→Linux 的几十 MiB 单文件完整粘贴与
  传输中断开，再执行多文件/目录、取消、替换和断线验收；通过前不完成本 task。

## 实施记录

- 2026-08-18：在保留 task-052 单文件实现的前提下新增只读 FUSE tree。清单中的文件、
  显式目录和缺失的安全父目录分别投影为 inode；挂载根只向剪贴板发布全部顶层路径，
  不把随机临时挂载目录当成待粘贴目录。
- 2026-08-18：tree 独立校验相对路径，拒绝空路径、绝对路径、反斜杠、NUL、`.`、`..`、
  空组件、重复路径及文件作为父目录；文件名必须与相对路径 basename 一致。
- 2026-08-18：目录枚举和元数据查询不打开内容。每个普通文件拥有独立惰性内容工厂、
  reader 和释放回调；零字节文件在 FUSE `open` 时触发按需请求并等待空流完成，避免内核
  因 size=0 不发送 `read` 而遗漏网络生命周期。
- 2026-08-18：Linux FUSE manager 同时管理单文件或 tree 挂载。tree 发布完整顶层路径
  列表；条件替换要求剪贴板当前路径列表与所发布列表逐项一致；清理时卸载并删除临时目录。
- 2026-08-18：Hub 在 Linux 收到 `BatchOffered` 后直接发布虚拟 tree，不进入原有 `.partial`
  自动落盘。每个文件首次被 FUSE 访问后才产生 `FileRequest`，多个请求沿用现有单连接能力
  串行派发；`Chunk` / `StreamCompleted` / `Failed` 按 entry id 路由。
- 2026-08-18：批次跟踪网络完成、FUSE 消费和句柄释放。新单文件/批次 offer 在已开始读取
  尚未完整收尾时延迟；本机剪贴板替换保留已开始流，完成后取消未请求条目并卸载；用户取消、
  reader 取消、远端失败、断线和运行态析构都会唤醒 producer 并清理挂载。
- 2026-08-18：清理了两个没有对应进程且返回 `ENOTCONN` 的旧 m590bridge 临时 FUSE
  挂载；随后用新挂载执行本地 tree smoke，成功浏览顶层/嵌套目录、读取内容并正常卸载。
- 2026-08-18：首轮跨机反馈 Linux 接收几十 MiB 单文件也会在数百 KiB 后卡住，Hub 此时
  无法及时断开，多文件更严重。检查确认单文件 FUSE 实现未随 tree 改动，但 Hub 仍在网络
  session loop 内同步调用有界管道 `push`；消费者停读或变慢、管道写满时该调用最长阻塞
  30 秒，使取消、断开和心跳无法被同一循环处理。
- 2026-08-18：为管道增加整块 `try_push`。Linux Hub 在容量不足时最多保留一个网络块并
  暂停继续读取 socket，每轮继续处理控制命令、FUSE release/cancel 和生命周期事件；容量
  恢复后按原顺序写入，不部分入队。持续背压仍累计原有 30 秒超时并发出 cancel，但每次
  Hub 调用都会立即返回；不改变 Windows OLE 路径和 wire 协议。
- 2026-08-18：新增单文件/tree 大流量真实挂载 smoke，分别把 24 MiB+123 B 模式内容按
  256 KiB 网络块送入有界管道，两个入口均完成逐字节一致性校验并正常卸载。
- 2026-08-19：补齐 Linux 单文件与 FUSE tree 的 task-058 诊断链路，记录 offer 发布、FUSE
  `Request`、网络请求、首块、背压、完成、Consumed/Released/Cancel，便于把“0 KB”定位到
  剪贴板发布、FUSE 访问或网络流阶段。
- 2026-08-19：修正未收到任何 FUSE `Request` 时的剪贴板所有权检查。Wayland 读取当前
  URI 列表可能短暂返回不一致，单文件/批次现在会保留尚未开始读取的挂载；一旦有文件
  真正请求，仍按原逻辑处理替换、取消和完成后的清理。Windows OLE 分支未改动。
- 2026-08-19：第二轮跨机日志确认单文件已完成 offer 发布、FUSE 请求、网络请求，首个
  256 KiB 块也已成功写入管道，但此后没有下一块或完成事件。对照 task-052 与 task-057
  的变更，主循环仍会在每轮调用 `is_current()`；其 Linux 文件列表读取会进入 task-057
  新增的 Wayland MIME 同步回退，Nautilus 正在等待 FUSE 数据时可能互相等待。
- 2026-08-19：单文件或批次只要已有 FUSE 请求尚未完成网络、消费和句柄释放，就不再从
  session 热循环同步读取剪贴板所有权。活跃传输原本就会在剪贴板替换后继续完成，因此
  取消语义不变；首次请求前和完整释放后仍检查所有权，保留替换与挂载清理。

## 修改文件

- `crates/m590-daemon/src/linux_virtual_file.rs`：只读 FUSE tree、路径安全、逐文件惰性读取、
  零字节处理及纯内存/显式 FUSE smoke 测试；单文件实现保持原 API。
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`：单文件/tree 统一挂载所有权、顶层
  路径列表发布、条件替换与卸载清理。
- `crates/m590-daemon/src/hub.rs`：Linux 虚拟批次发布、串行请求、进度、取消、替换、失败
  和延迟 offer 生命周期；本轮新增单文件/批次的非阻塞暂存块与 socket 背压、task-058
  诊断事件、未请求 offer 的所有权检查保护，以及活跃流期间的同步所有权轮询暂停；
  Windows OLE 分支未改变行为。
- `crates/m590-daemon/src/virtual_file_bridge.rs`：新增整块非阻塞 `try_push`，覆盖容量不足、
  取消即时返回和单文件/tree 24 MiB 真实 FUSE 流测试；原同步 `push` 继续供 Windows 使用。
- `docs/tasks/task-058.md`、`docs/plans/current.md`、`docs/discovery/project-map.md`、
  `docs/discovery/commands.md`、`AGENTS.md`、`项目说明.md`：同步实现状态、验证命令和真机边界。

## 验证结果

- 原任务命令 `cargo test -p m590-daemon virtual_file_bridge linux_virtual_file
  linux_virtual_file_manager`：未运行测试，Cargo 返回 `unexpected argument
  'linux_virtual_file'`；Cargo 只接受一个位置过滤参数，已把任务命令修正为单过滤形式。
- `cargo test -p m590-daemon virtual_file -- --nocapture`：通过，17 passed、1 ignored；覆盖
  bridge、FUSE 单文件/tree 和 manager，0 failed。
- `cargo test -p m590-daemon linux_virtual -- --nocapture`：通过，15 passed、1 ignored；包含
  Hub 批次准备惰性、目录-only 批次及 Linux 生命周期测试，0 failed。
- `cargo test -p m590-daemon`：通过，daemon lib 63 passed、1 ignored，bin 1 passed，0 failed。
- `cargo test -p m590-daemon mounted_tree_smoke_browses_and_reads_nested_content -- --ignored
  --nocapture`：通过，真实 FUSE 挂载可列出 `folder/empty`、读取 `folder/note.txt` 后卸载。
- `cargo check --workspace`：通过。
- `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`：通过；Linux cfg 接线未
  破坏 Windows daemon 编译。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过，无 warning。
- `cargo fmt --all -- --check`、`git diff --check`：通过。
- 初次实现阶段尚未执行 Linux GNOME Wayland + Nautilus 跨机，且本地 FUSE smoke 不能
  代替跨机验收；随后首轮结果见下一项。
- 首轮 Linux GNOME Wayland + Nautilus 跨机：失败；几十 MiB 单文件在接收数百 KiB 后
  卡住且无法及时断开，多文件不能完成。本轮修复后仍待用户跨机复测。
- `cargo test -p m590-daemon virtual_file -- --nocapture`：本轮通过，21 passed、2 ignored，
  0 failed；新增非阻塞整块入队、容量恢复、取消即时返回和非阻塞 30 秒累计超时覆盖。
- `cargo test -p m590-daemon linux_virtual -- --nocapture`：本轮通过，16 passed、1 ignored，
  0 failed；新增 Hub 管道满时立即返回背压的路由测试。
- `cargo test -p m590-daemon`：本轮通过，daemon lib 68 passed、2 ignored，bin 1 passed，
  0 failed。
- `cargo test -p m590-daemon mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure
  -- --ignored --nocapture`：通过；真实 FUSE 单文件和 tree 各读取 24 MiB+123 B，逐字节一致，
  0 failed。
- `cargo check --workspace`、`cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`：
  本轮通过。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`、`cargo fmt --all -- --check`、
  `git diff --check`：本轮通过。
- `cargo test -p m590-daemon`：本轮通过，daemon lib 68 passed、2 ignored，bin 1 passed，0 failed。
- `cargo test -p m590-daemon linux_virtual -- --nocapture`：本轮通过，16 passed、1 ignored，0 failed；
  覆盖未请求批次保持挂载的状态辅助及现有 Linux FUSE 生命周期。
- `cargo test -p m590-daemon --features task-057-diagnostics linux_virtual -- --nocapture`：通过，16 passed、1 ignored，0 failed。
- `cargo check --workspace`、`cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`、
  `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`、`cargo fmt --all -- --check`、
  `git diff --check`：本轮通过；Windows 交叉检查无 warning。
- `cargo test -p m590-daemon mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure -- --ignored --nocapture`：
  当前环境未通过，测试在创建 FUSE 挂载时返回 `ENOENT`；不是代码断言失败，需在具备可用
  `/dev/fuse` 的 Linux 桌面环境重跑。
- `cargo test -p m590-daemon linux_virtual -- --nocapture`：本轮通过，17 passed、1 ignored，
  0 failed；新增“活跃网络/FUSE 流不轮询剪贴板，完整释放后恢复轮询”的状态回归。
- `cargo test -p m590-daemon virtual_file -- --nocapture`：本轮通过，21 passed、2 ignored，
  0 failed。
- `cargo test -p m590-daemon`：本轮通过，daemon lib 69 passed、2 ignored，bin 1 passed，
  0 failed。
- `cargo test -p m590-daemon --features task-057-diagnostics linux_virtual -- --nocapture`：
  本轮通过，17 passed、1 ignored，0 failed。
- `cargo test -p m590-daemon mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure
  -- --ignored --nocapture`：本轮通过；真实 FUSE 单文件和 tree 各读取 24 MiB+123 B，逐字节
  一致，0 failed。此前同日的 `ENOENT` 环境 blocker 已解除。
- `cargo check --workspace`、`cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`、
  `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：本轮通过；Windows OLE/wire
  分支未产生交叉编译回归。

## 文档影响检查

- 已更新：本 task、当前计划、Agent 当前阶段与项目说明，补充首块后停住的真机证据、
  活跃流剪贴板所有权轮询暂停和本轮真实验证结果。
- 无需更新：项目结构图和常用命令；任务仍为 `in_progress`，没有新增模块、命令、wire
  字段或产品能力边界。
- 无需更新：协议 wire 字段、Hub HTTP API、UI 交互、Windows OLE、安装器与自启均未改变。

## 风险 / blocker

- 仍需 Linux GNOME Wayland + Nautilus 真机确认：粘贴多个顶层文件、嵌套/空目录、空文件、
  大文件、系统进度、取消、替换、断线及重新发送同批输入后再次粘贴；尤其要先确认本轮
  单文件卡住和无法断开的回归已经消失。
- 当前缺少活跃流剪贴板轮询修复后的跨机复测；启用 `task-057-diagnostics` 后重点确认
  `single_network_first_chunk` 后持续出现 `single_network_chunk_pushed`，最终出现
  `single_network_stream_completed`、`single_virtual_consumed` 和 `single_virtual_release`。
- 未请求的 offer 现在对一次所有权不一致采取保留策略，因此本地用户在首次粘贴前替换
  剪贴板时，旧挂载可能要等新的远端 offer 或显式取消才清理；跨机复测需覆盖该场景。
- 沿用 task-044 的一次性网络 offer/reader：同一个 clipboard offer 完成后直接再次
  `Ctrl+V` 不保证能重开已消费的网络流。若产品要求同一 offer 任意次粘贴，需要另建协议
  生命周期任务，不能在 task-058 内暗改 task-055 wire 字段。
- tree 使用两个 FUSE worker 配合串行网络请求；Nautilus 若在真机产生超出该模型的并发
  打开顺序，需要以实际日志为准调整，但本 task 不扩展为并行网络传输。
