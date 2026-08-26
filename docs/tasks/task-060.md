# task-060 · 重复粘贴与替换/图标回归

## 状态

`in_progress`（实现完成，真机待验收）

## 背景

真机跨机测试发现，当前「同一份已消费 clipboard offer 再次 `Ctrl+V`」在部分场景可用、
部分场景报错或只能部分重开。这与 task-058 验收后留在仓库的边界一致：协议仍为一次性
消费，但 OS 侧（Windows OLE / Linux FUSE）会在第二次 `Ctrl+V` 时再次打开虚拟对象，触发
发送端找不到已清理的 `transfer_id` 而失败。本 task 把真机观察到的四类问题集中记录，
等待统一排查与修复，不混入已完成的 task-058。

## 真机观察到的问题（原始记录）

1. **Linux → Windows，多文件只有第一个可第二次粘贴**
   - 单文件：完成后再次 `Ctrl+V` 可成功第二次粘贴。
   - 多文件：再次 `Ctrl+V` 时只能粘贴其中一个文件（疑似批次队列第一个），其余无反应
     或失败。
   - 疑似原因（待验证）：批次发送源在完成后未保留所有 entry 的可重开状态，只保留了
     队首 entry；Windows OLE 集合在第二次打开时只能重新请求到仍存活的那个 `transfer_id`。

2. **Windows → Linux，多文件第二次粘贴报「拼接文件时出错：输入/输出错误」**
   - 单文件：完成后再次 `Ctrl+V` 可成功第二次粘贴（前提：当前目录无同名文件）。
   - 多文件：再次 `Ctrl+V` 报错「拼接文件时出错：输入/输出错误」。
   - 疑似原因（待验证）：Linux FUSE tree 在第二轮重新打开时，部分 entry 的内容管道/
     reader 已被释放且未重建，Nautilus 读取到空或错误的流；或批次完成计数/entry 状态
     在第二轮没有统一重置。

3. **Windows → Linux，目标目录存在同名文件选「替换」报错**
   - 现象：当前目录已有同名文件，`Ctrl+V` 选「替换」时报错：
     「获取文件
     "/tmp/m590bridge-fuse-263923-13/DJI_20260412164952_0709_D.MP4" 的信息时出错：
     没有那个文件或目录」。
   - 疑似原因（待验证）：Nautilus「替换」流程先 `stat` 目标占��文件，但 FUSE tree 在
     上一轮完成后已清理/卸载对应 inode，`getattr` 返回 `ENOENT`；或第二轮重新挂载前
     旧路径已失效，Nautilus 没有重新走 `lookup`。

4. **Windows → Linux，粘贴的文件在 Nautilus 右上角显示不可点的「x」关闭图标**
   - 现象：粘贴落地后的真实文件（如 `/home/huang/视频/Node.txt`）在 Nautilus 中带一个
     「x」关闭角标，点击无反应，实际使用无影响；本机复制粘贴的文件无此图标。
   - 疑似原因（待验证）：FUSE 挂载文件缺少某些 xattr / 权限 / 类型标记，Nautilus/GVFS
     据此给文件附加「不可读/占位/链接失效」类 emblem；落地拷贝后 Nautilus 仍缓存了来源
     元数据。需对比本机文件与 FUSE 来源文件的 `stat`/`xattr`/MIME 差异。

## 目标

- 让「同一份已发布文件剪贴板在完成后再次 `Ctrl+V`」在单文件与多文件/文件夹批次下都
  一致可用，覆盖 Linux↔Windows 双向。
- 修复 Windows→Linux 多文件第二次粘贴的「拼接文件时出错：输入/输出错误」。
- 修复 Windows→Linux 同名文件「替换」时的 `ENOENT` 报错。
- 消除粘贴落地文件在 Nautilus 上的异常「x」图标（或确认无影响后记录为已知现象）。
- 不增加 wire 字段、不升级协议版本（继续 v3），同一 `transfer_id` 仅串行重开。
- 不实现断点续传、多文件并行、独立数据连接；不复活 task-019A。

## 允许修改

- `crates/m590-core/src/session.rs`（发送源保留/重开生命周期）
- `crates/m590-daemon/src/hub.rs`（单文件与批次重开接线、entry 状态重置）
- `crates/m590-daemon/src/virtual_file_bridge.rs`（reader 重开）
- `crates/m590-daemon/src/linux_virtual_file.rs` 与
  `crates/m590-daemon/src/linux_virtual_file_manager.rs`（FUSE 重开、`getattr`/
  `lookup`、替换流程兼容、emblem 相关属性）
- `crates/m590-daemon/src/windows_virtual_file_manager.rs` 与
  `crates/m590-clipboard/src/windows_virtual_file.rs`（OLE 集合第二次打开）
- `docs/domain/protocol-draft.md`、`docs/plans/current.md`、`AGENTS.md`、`项目说明.md`
  中关于「一次性消费/可重开」边界的说明
- 本 task

## 禁止修改

- task-055 已定的 wire 字段和路径安全规则。
- 断点续传、并行传输、独立数据连接相关逻辑。
- Android/macOS 与远程键鼠控制。
- 安装器、自启、打包流程。

## 验证命令与完成标准

```bash
cargo test -p m590-core file -- --nocapture
cargo test -p m590-daemon virtual_file -- --nocapture
cargo test -p m590-daemon linux_virtual -- --nocapture
cargo test -p m590-daemon -- --nocapture
cargo check --workspace
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
cargo fmt --all -- --check
git diff --check
```

真机验收（两端各连续两次 `Ctrl+V`，校验内容一致）：

- [ ] Linux → Windows 单文件：第二次粘贴成功。
- [ ] Linux → Windows 多文件：第二次粘贴全部成功（不再是只第一个）。
- [ ] Windows → Linux 单文件：第二次粘贴成功。
- [ ] Windows → Linux 多文件：第二次粘贴成功（不再「拼接文件时出错」）。
- [ ] Windows → Linux 同名文件「替换」：不再 `ENOENT`，内容正确覆盖。
- [ ] Windows → Linux 粘贴落地文件：Nautilus 无异常「x」图标，或已记录为无害已知现象
      并说明原因。

## 实施记录

- 2026-08-26：定位四个问题的根因并完成「同一 clipboard offer 串行重开」的核心实现，
  不改 wire 字段、不升级协议版本（继续 v3）。
- **bridge 层**（`virtual_file_bridge.rs`）：`open_reader` 支持串行重开——上一轮 reader
  干净结束（consumed/finished/released/cancelled）后，重置管道状态并允许打开新 reader，
  重新发 `BridgeEvent::Request`。`PipeReader::Drop` 重置 `reader_open`，使第二次打开不再
  返回 `AlreadyExists`。仍在进行中的 reader 不允许并发重开。
- **core 发送端**（`session.rs`）：`StagedOutboundFile` 与 `ActiveOutboundSend` 新增
  `retain_for_clipboard` 标志；`offer_file`/`offer_file_path`/`offer_file_batch_paths`
  默认 `false`（一次性，行为不变）。新增 `Session::retain_outbound_file()`，剪贴板来源在
  offer 后调用。`pump_outbound_file_inner` 完成时若该标志为 true，把发送源重新插回
  `staged_outbound_files`，使同一 `transfer_id` 可响应第二次 `FileRequest`。
- **core 接收端**（`session.rs`）：stream 类型 offer 在首块到达时重新插回 `inbound_offers`，
  完成时不再 remove；disk 类型仍一次性 remove。空文件 stream 路径同样保留 offer。这样
  `request_file_stream` 第二轮仍能找到 offer。
- **hub**（`hub.rs`）：剪贴板单文件入口（`offer_local_file`）与批次入口在 offer 后对每个
  file entry 调用 `retain_outbound_file`。单文件/批次收到第二次 `BridgeEvent::Request` 时，
  重置 `completed/consumed/released/first_chunk*` 及 `completed_files/completed_bytes`，
  然后重新走 `request_file_stream` 分发网络流（不再只 `producer.finish()`）。
- **Linux FUSE**（`linux_virtual_file.rs`）：`release_handle` 在 release 后把
  `ContentState` 重置为 `Unopened`（单文件与 tree 两处），使第二次打开重新调用 bridge
  工厂而非复用已 EOF 的旧 reader，修复「拼接文件时出错：输入/输出错误」。
- **x 图标**：本轮未改 FUSE 文件属性，避免在无桌面环境引入回归；记录为待真机确认项。

## 修改文件

- `crates/m590-daemon/src/virtual_file_bridge.rs`：`open_reader` 串行重开、
  `PipeReader::Drop` 重置 `reader_open`、重开回归测试。
- `crates/m590-core/src/session.rs`：`retain_for_clipboard` 标志、`retain_outbound_file()`、
  发送端完成后重新插回 staged 源、接收端 stream offer 保留、重开回归测试。
- `crates/m590-daemon/src/hub.rs`：剪贴板单文件/批次入口调用 retain、第二次 Request 重置
  并重发网络流、批次完成计数回退、批次重开回归测试。
- `crates/m590-daemon/src/linux_virtual_file.rs`：`release_handle` 重置 `ContentState` 为
  `Unopened`（单文件与 tree）。

## 验证结果

- `cargo check --workspace`：通过。
- `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`：通过。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cargo clippy -p m590-core --lib --no-deps -- -D warnings`：通过。
- `cargo fmt --all -- --check`、`git diff --check`：通过。
- `cargo test -p m590-core --lib`：39 passed，0 failed。
- `cargo test -p m590-daemon --lib`：71 passed，0 failed，2 ignored。
- `cargo test -p m590-daemon virtual_file -- --nocapture`：22 passed，2 ignored。
- `cargo test -p m590-daemon linux_virtual -- --nocapture`：18 passed，1 ignored。
- 新增 `completed_reader_can_be_reopened_for_a_second_stream`：通过。
- 新增 `retained_clipboard_offer_can_be_requested_again_after_completion`：通过。
- 新增 `non_retained_offer_is_gone_after_completion`：通过。
- 新增 `linux_virtual_batch_completed_entry_can_be_reopened`：通过。
- 待真机验收：四类问题在 Windows/Linux 桌面连续两次 `Ctrl+V` 的真机复测（见完成标准）。

## 文档影响

- 已更新：本 task、`docs/plans/current.md`、`docs/domain/protocol-draft.md`、`AGENTS.md`
  当前阶段说明（同一 offer 可串行重开，但并发/断点续传/替换后重开仍不保证）。
- 无需更新：协议 wire 字段、Hub HTTP API、UI 交互、安装器与自启均未改变。

## 风险

- 重开生命周期可能影响取消、替换、断线后的清理，需保留现有清理路径不漏不串。
- 永久保留发送源可能造成文件句柄/路径泄漏，需区分剪贴板来源与普通 API 来源。
- FUSE `getattr`/`lookup` 在替换流程下的行为需以真机日志为准，避免回归 task-058 已通过
  的目录树、空文件、取消、断线场景。

## 下一步

- 开始任务时先以只读分析定位：发送端哪个 entry 在批次完成后被清理、Linux FUSE 第二轮
  重新打开的具体失败点、Windows OLE 集合第二次打开的请求顺序、Nautilus「替换」的
  `stat` 目标路径、emblem 触发的 `stat`/xattr 差异。
