# task-043 · Windows 单文件 OLE 虚拟剪贴板原型

## 状态

`completed`（2026-08-12：Windows 10 Explorer 真机验收通过）

## 目标

在 Windows 10 上提供一个最小、可独立运行的虚拟文件剪贴板原型：把单个虚拟文件通过 OLE `IDataObject` 发布到系统剪贴板，Explorer 读取 `CFSTR_FILECONTENTS` 时才打开内容源并通过 `IStream` 读取，不先写入用户保存目录。

本 task 只验证 Windows Shell/OLE 能力，为后续接入现有 `FileOffer` / `FileRequest` 文件通道降低风险；不把原型误报为端到端网络功能。

## 实现选择

- `CFSTR_FILEDESCRIPTORW`：发布安全文件名和准确大小。
- `CFSTR_FILECONTENTS`：仅支持索引 0，通过 `TYMED_ISTREAM` 延迟提供内容。
- `CFSTR_PREFERREDDROPEFFECT`：明确声明复制语义。
- 内容源使用可重复打开的 `Read + Seek + Send` 工厂；`GetData(FILECONTENTS)` 前不得调用工厂。
- 独立 Windows example 使用可调速的生成流，便于观察 Explorer 原生复制进度；不创建永久中间文件。

## 允许修改

- `crates/m590-clipboard/Cargo.toml`、`Cargo.lock`：Windows-only 官方 `windows` crate 依赖。
- `crates/m590-clipboard/src/windows_virtual_file.rs`、`src/lib.rs`：Windows OLE 数据对象和公开入口。
- `crates/m590-clipboard/examples/windows_virtual_file.rs`：Windows 真机原型入口。
- 本 task、task-042 状态、当前计划、命令与项目结构文档。

## 禁止修改

- `m590-core` 协议、`m590-net`、Hub/Session 文件状态机与自动 request 行为。
- `FileCancel`、端到端网络流桥接、多个文件、文件夹、断点续传。
- Linux FUSE、GNOME/Nautilus 扩展、macOS/Android。
- task-042 已实现的安装包和登录自启代码。

## 验证命令

Linux 当前环境：

```bash
cargo test -p m590-clipboard
cargo check -p m590-clipboard
cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings
cargo check -p m590-clipboard --target x86_64-pc-windows-gnu
```

Windows 10 真机：

```powershell
cargo run -p m590-clipboard --example windows_virtual_file -- 268435456 8
```

启动 example 后，在 Explorer 当前目录按 `Ctrl+V`；确认粘贴前日志没有 `content_opened`，粘贴时才出现该日志，并生成指定大小的文件。

## 完成标准

- [x] Windows target 类型检查通过，Linux 构建不引入 Windows 链接依赖。
- [x] `IDataObject::QueryGetData/GetData` 只接受本 task 声明的三种格式和合法 `lindex` / `tymed`。
- [x] 文件描述符拒绝空名、路径分隔符、`.` / `..` 和超长 UTF-16 名称。
- [x] 内容工厂只在 `CFSTR_FILECONTENTS` 请求时调用；流支持 Explorer 所需的 `Read` / `Seek` / `Stat`。
- [x] Windows 10 Explorer 可粘贴出内容正确的单文件，并观察到系统复制进度。

## 实施记录

- 新增平台无关的 `VirtualFile` 描述：持有安全 Windows 文件名、准确大小，以及可重复打开的惰性 `Read + Seek + Send` 内容工厂。
- Windows 使用 OLE STA 发布自定义 `IDataObject`，提供 `CFSTR_FILEDESCRIPTORW`、`CFSTR_FILECONTENTS` 和 `CFSTR_PREFERREDDROPEFFECT`；只有请求索引 0 的 `FILECONTENTS` 时才调用内容工厂。
- `FILECONTENTS` 返回只读 `IStream`；`Read` 会处理 Rust 短读和 `Interrupted`，只在真正 EOF 时返回 `S_FALSE`，并实现 `Seek` / `Stat`。
- HGLOBAL 与 `IStream` 通过 `STGMEDIUM` 把所有权交给 OLE 调用方；发布失败及 guard 释放时保证 COM 接口先于 `OleUninitialize` 释放。
- guard 只能在创建它的线程泵消息和释放；退出时比较当前剪贴板对象的 COM 身份，只在内容仍属于本原型时清空，避免覆盖用户后来复制的新内容。
- 新增 Windows example，按参数生成可调大小/延迟的模式数据流，不创建永久中间文件；`content_opened` 用于观察何时开始读取内容。
- 暂不实现 `IDataObjectAsyncCapability`：基础虚拟文件格式不以它为前置条件，是否需要异步协作留给 Explorer 真机结果决定。

## 修改文件

- `crates/m590-clipboard/Cargo.toml`、`Cargo.lock`：增加 Windows-only 官方 `windows` / `windows-core` 依赖与所需 Win32 feature。
- `crates/m590-clipboard/src/virtual_file.rs`：虚拟文件描述、内容工厂与安全文件名校验。
- `crates/m590-clipboard/src/windows_virtual_file.rs`：OLE `IDataObject`、延迟 `IStream`、STA 生命周期与消息泵。
- `crates/m590-clipboard/src/lib.rs`：导出平台无关描述和 Windows 发布 API。
- `crates/m590-clipboard/examples/windows_virtual_file.rs`：Windows Explorer 真机原型入口。
- `AGENTS.md`、`docs/plans/current.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`docs/tasks/task-042.md`、`项目说明.md`、本 task：同步当前边界、状态、命令与结构。

## 验证结果

- `cargo test -p m590-clipboard`：通过，21 passed、0 failed；包含 3 项新增虚拟文件描述/惰性工厂测试。
- `cargo check -p m590-clipboard`：通过，确认 Linux 构建不链接 Windows OLE 依赖。
- `cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples`：通过，Windows 库和真机 example 均完成类型检查。
- `cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings`：未通过；被 task-043 范围外既有 `src/image_file.rs:16` 的 `clippy::doc_lazy_continuation` 拦截。
- `cargo clippy -p m590-clipboard --target x86_64-pc-windows-gnu --lib --examples --no-deps -- -D warnings -A clippy::doc_lazy_continuation`：通过；只豁免上述既有文档注释告警后，task-043 Windows 库/example 无新增 clippy 告警。
- `rustfmt --edition 2021 --check crates/m590-clipboard/src/virtual_file.rs crates/m590-clipboard/src/windows_virtual_file.rs crates/m590-clipboard/examples/windows_virtual_file.rs`：通过。
- Windows 10 Explorer 运行：用户于 2026-08-12 确认真机测试通过；粘贴前未打开内容，按 `Ctrl+V` 后开始取流，文件生成和系统复制进度符合预期。

## 文档影响检查

- 已更新：当前计划、常用命令、项目结构、项目说明与 Agent 当前阶段说明。
- 无需更新：没有修改协议、Hub API、网络状态机、UI 或配置字段，因此 `docs/domain/protocol-draft.md` 与 `docs/ui-spec.md` 无需更新。

## 风险 / blocker

- Explorer 或安全软件可能在实际粘贴前请求文件内容；本原型只能保证按 `FILECONTENTS` 消费请求取流，不能识别请求一定来自键盘 `Ctrl+V`。
- 当前网络协议没有取消消息；因此本 task 明确不接网络，避免用户取消 Explorer 复制后发送端仍持续占用连接。

## 下一步

- 另建 task-044，设计并实现虚拟文件 `IStream` 与现有 `FileRequest` 的端到端桥接及取消语义。
