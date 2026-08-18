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

- 在 Linux GNOME Wayland + Nautilus 与同一局域网 Windows 端执行多文件/目录真机验收，
  根据日志修复实际 Shell 行为后再决定是否完成本 task。

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

## 修改文件

- `crates/m590-daemon/src/linux_virtual_file.rs`：只读 FUSE tree、路径安全、逐文件惰性读取、
  零字节处理及纯内存/显式 FUSE smoke 测试；单文件实现保持原 API。
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`：单文件/tree 统一挂载所有权、顶层
  路径列表发布、条件替换与卸载清理。
- `crates/m590-daemon/src/hub.rs`：Linux 虚拟批次发布、串行请求、进度、取消、替换、失败
  和延迟 offer 生命周期；Windows OLE 分支未改变行为。
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
- Linux GNOME Wayland + Nautilus 跨机：尚未执行，不能以本地 FUSE smoke 代替。

## 文档影响检查

- 已更新：本 task、当前计划、项目结构图、常用命令、Agent 当前阶段与项目说明。
- 无需更新：协议 wire 字段、Hub HTTP API、UI 交互、Windows OLE、安装器与自启均未改变。

## 风险 / blocker

- 仍需 Linux GNOME Wayland + Nautilus 真机确认：粘贴多个顶层文件、嵌套/空目录、空文件、
  大文件、系统进度、取消、替换、断线及重新发送同批输入后再次粘贴。
- 沿用 task-044 的一次性网络 offer/reader：同一个 clipboard offer 完成后直接再次
  `Ctrl+V` 不保证能重开已消费的网络流。若产品要求同一 offer 任意次粘贴，需要另建协议
  生命周期任务，不能在 task-058 内暗改 task-055 wire 字段。
- tree 使用两个 FUSE worker 配合串行网络请求；Nautilus 若在真机产生超出该模型的并发
  打开顺序，需要以实际日志为准调整，但本 task 不扩展为并行网络传输。
