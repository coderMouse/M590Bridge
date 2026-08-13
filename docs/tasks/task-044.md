# task-044 · Windows 按粘贴请求文件流与取消

## 状态

`in_progress`

## 目标

把 task-043 的 Windows 单文件 OLE `IStream` 原型接入现有 `FileOffer/FileRequest/FileChunk/FileComplete` 通道：发送端复制文件时只发布元数据，接收端在 Explorer 真正请求 `CFSTR_FILECONTENTS` 时才发送 `FileRequest`，网络分片直接进入有界内存管道，由 OLE 流读取并写入目标目录。补充文件取消与无进展超时语义。

## 允许修改

- `crates/m590-core`：文件取消消息、Session 流式接收目标与校验。
- `crates/m590-net`：取消消息帧编解码和协议版本同步。
- `crates/m590-daemon`：有界虚拟文件桥、Windows STA/OLE 生命周期、Hub 文件 offer/request 调度。
- `crates/m590-clipboard`：为网络流接入 OLE `IStream` 所需的小幅 API 扩展。
- 本 task、`docs/plans/current.md`、`docs/domain/protocol-draft.md`、必要的命令/结构文档。

## 禁止修改

- 多文件、文件夹、Linux FUSE、断点续传、多 peer 网格。
- task-042 安装包/登录自启功能；不复活 task-042 或已取消 task-019A。
- Tauri UI 大改；跨机文件数据不穿过前端 IPC。

## 完成标准

- 文件 offer 只发布安全元数据；Windows 接收端未粘贴前不发送 `FileRequest`，不创建 `.part` 或用户可见临时文件。
- Explorer `Ctrl+V` 触发请求后，网络 chunk 经有界管道进入 `IStream`，目标目录只出现最终文件，系统复制进度可见。
- SHA-256、大小、offset 校验继续由 `m590-core` 执行；错误或取消清理活动传输。
- 取消消息可停止发送方活动文件，管道读写端和等待请求在超时/断连时被唤醒。
- 不影响 Linux 现有 offer 后自动请求、`.part` 落盘行为。

## 验证命令

```bash
cargo test -p m590-core
cargo test -p m590-net
cargo test -p m590-daemon
cargo test -p m590-clipboard
cargo check --workspace
cargo clippy -p m590-core -p m590-net -p m590-daemon -p m590-clipboard --lib --no-deps -- -D warnings
cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples
cargo check -p m590-daemon --target x86_64-pc-windows-gnu
```

Windows 真机由用户完成：A/B 双机复制只发 offer、B 未粘贴不传内容、Explorer `Ctrl+V` 后按需传输、系统原生进度、取消/超时停止发送。

## 实施记录

- 在 `m590-core` 增加协议版本 3 的 `FileCancelPayload` / `Message::FileCancel`，Session 处理取消、清理 staged/active/inbound 状态，并为接收端增加磁盘与流式两种目标。
- 新增 `request_file_stream`：只登记流目标并发出 `FileRequest`；后续 `FileChunk` 仍由 core 校验 offset、大小、offer/complete SHA-256，再以 `Chunk` 事件交给 daemon。
- 在 `m590-daemon` 新增有界 `VirtualFileBridge`：首次打开 reader 才发 `Request`，网络 producer 背压写入，reader drop 或已开始传输后读写连续 30 秒无进展时发 `Cancel`；未粘贴的 offer 不受该超时影响。Windows-only STA manager 持有 OLE guard 并泵消息。
- Hub Windows 分支发布单文件虚拟剪贴板，暂停普通文件列表轮询，按 OLE 请求启动网络拉取；流完成后保留虚拟剪贴板直到用户替换，取消/超时清理并发 `FileCancel`。

## 修改文件

- `crates/m590-core/src/protocol.rs`、`src/session.rs`、`src/lib.rs`：FileCancel、流式接收目标、校验和测试。
- `crates/m590-net/src/frame.rs`、`src/lib.rs`：消息类型 15 编解码与全消息 roundtrip。
- `crates/m590-daemon/src/virtual_file_bridge.rs`：跨平台有界 pipe、lazy request、取消/超时、Linux 单测。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`、`src/hub.rs`、`src/lib.rs`：Windows STA/OLE 生命周期和 Hub 接线。
- `crates/m590-clipboard/src/windows_virtual_file.rs`：暴露剪贴板当前身份查询。
- `docs/domain/protocol-draft.md`、`docs/plans/current.md`、`docs/discovery/project-map.md`、`docs/discovery/commands.md`、`项目说明.md`：同步协议 v3、能力边界和真机步骤。

## 验证结果

- `cargo test -p m590-core -p m590-net -p m590-daemon`：通过；core 32、net 17、daemon lib 25 + bin 1。
- `cargo test -p m590-clipboard`：通过；21 passed。
- `cargo check --workspace`：通过。
- `cargo clippy -p m590-core -p m590-net -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings -A clippy::doc_lazy_continuation`：通过；不豁免时仍受 task-043 既有 `image_file.rs` 文档告警影响。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples`：通过；主 Cargo cache 只读后改用 `/tmp` 临时缓存完成验证。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu`：通过；修复 3 处 Windows-only `SessionError` 转换后无告警。
- `CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings`：通过。
- `cargo test -p m590-daemon virtual_file_bridge`：通过；3 个有界管道测试全部通过，包含 producer 在消费者停滞时超时并发出取消事件。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过；新增 producer 超时路径无告警。
- Windows↔Linux Explorer 真机：待用户测试，不能以 Linux/交叉检查替代。

## 文档影响检查

- 已更新：协议草案、当前计划、项目结构、常用命令、项目说明。
- 无需更新：UI 规格、Tauri IPC/API、task-042 安装/自启代码；跨机文件数据未穿过前端 IPC。

## 风险 / blocker

- OLE `IStream` 只允许所有 seek origin 计算后仍位于当前位置的 no-op；网络流不能任意回退，需在 Windows 真机确认 Explorer 行为。
- Explorer 可能重复请求 `FILECONTENTS`；本 task 先限制单个活动消费者并返回清晰错误，记录后续扩展点。

## 下一步

下一步由用户在 Windows↔Linux 真机验证 offer 不传内容、Explorer `Ctrl+V` 后传输、系统进度以及取消/超时；根据实际失败日志再开修复 task，task-042 继续暂停。
