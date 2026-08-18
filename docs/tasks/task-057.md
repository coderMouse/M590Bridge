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

- 在 Windows 10 构建当前代码，完成 Explorer 多文件/嵌套目录、取消、替换、断线与
  单文件回归真机验收并记录结果。

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

## 修改文件

- `crates/m590-clipboard/Cargo.toml`、`src/lib.rs`、`src/virtual_file.rs`、
  `src/windows_virtual_file.rs`：集合模型、Windows 路径校验、多项 OLE descriptor 与按索引
  `IStream`，并保留单文件兼容入口。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`：STA 线程发布及条件替换集合。
- `crates/m590-daemon/src/virtual_file_bridge.rs`：网络请求开始前不计算排队流读取超时。
- `crates/m590-daemon/src/hub.rs`：Windows 批次发布、逐文件惰性请求、串行调度、状态和清理。
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
- Windows 10 Explorer 真机：待验收；当前 Linux 环境只能交叉编译，不能代替 Shell/OLE
  行为验证。

## 文档影响检查

- 已更新：本 task、当前计划、项目入口、协议草案、项目结构图和 Windows 验收命令。
- 无需更新：协议字段、Hub HTTP API、UI 交互、安装器和 Linux FUSE 均未改变。

## 风险 / blocker

- task-057 仍需 Windows 10 Explorer 真机验收后才能标记完成。
- 沿用 task-044 的一次性网络 offer：同一剪贴板 offer 完成后再次 `Ctrl+V` 仍不保证可重开
  `FILECONTENTS`；本 task 验证“重新发送同批输入后再次粘贴”。若产品要求同一 offer 任意
  次粘贴，需要另建任务扩展发送源保留与重复请求协议生命周期。
