# task-060 · 重复粘贴与替换/图标回归

## 状态

`in_progress`（Q1/Q2 真机通过；Q3/Q4 待修复；Q3 触发后 Linux→Windows 剪贴板卡死待排查）

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

- [x] Linux → Windows 单文件：第二次粘贴成功。（2026-08-27 真机通过）
- [x] Linux → Windows 多文件：第二次粘贴全部成功（不再是只第一个）。（2026-08-27 真机通过）
- [x] Windows → Linux 单文件：第二次粘贴成功。（2026-08-27 真机通过）
- [x] Windows → Linux 多文件：第二次粘贴成功（不再「拼接文件时出错」）。（2026-08-27 真机通过）
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
- `crates/m590-daemon/src/hub.rs`（二轮）：`VIRTUAL_PUBLISH_IDLE_TIMEOUT` 与
  `virtual_receive_publish_is_idle`、单文件/批次 keep_current 兜底 detach、
  `LinuxVirtualBatchReceive.published_at` 字段、idle 回归测试。

### 二轮（2026-08-27）：Q3 连锁故障兜底

真机复测确认 Q1/Q2 已通过，Q3（替换 `ENOENT`）出现后 Linux→Windows 剪贴板完全卡死、
重启不复位。只读定位根因：已发布但**从未被请求**（OS 消费者在 `read` 之前失败，例如
Nautilus 替换/冲突流程未进入读取）的虚拟 offer，会让 `virtual_receive`/
`virtual_batch_receive` 永久保留为 `Some`，使主循环里 `virtual_clipboard_active` 恒为
true，**本地剪贴板轮询被永久阻塞**——这就是「复制不更新、重启不复位」的成因。原代码无
任何 publish-idle 超时或兜底 detach 路径。

- **hub**（`hub.rs`）：新增 `VIRTUAL_PUBLISH_IDLE_TIMEOUT`（120s）与谓词
  `virtual_receive_publish_is_idle(requested, completed, published_at, now)`。Linux 单文件
  与批次 receive 的 keep_current 评估中，在 is_current 与原有 detach 判定之间插入：当
  从未被请求且未完成、且发布已超过 120s 时，`producer.fail` + `cancel_file` +
  `fuse_manager.clear()` + 清 `latest_clipboard_file_offer_id` 并 detach，释放本地剪贴板
  轮询。被请求过的 offer 不受影响（仍由 stream 生命周期/重开逻辑负责清理），避免回归
  Q1/Q2。
- `LinuxVirtualBatchReceive` 新增 `published_at: Instant` 字段（构造时设为 `now`），供
  批次 idle 判定使用。
- **Q3 替换流程的 `ENOENT` 与 Q4 Nautilus「x」角标**：无桌面环境，本轮未改 FUSE 属性，
  避免引入回归。怀疑 Q3 的 ENOENT 来自替换流程中 reader 在未完成时被 release 触发
  `BridgeEvent::Cancel` → `fuse_manager.clear()` → FUSE 卸载，随后 Nautilus 再 stat 即
  ENOENT；Q4 疑与 FUSE 文件 `atime/mtime=UNIX_EPOCH`/`perm 0o444` 等元数据被 GVFS 误判
  为占位/失效有关。两者列为待真机排查开放项。

## 真机复测反馈（2026-08-27）

提交 `796b8eb` 推送后真机复测结果：

- **Q1（Linux→Windows 多文件第二次只粘第一个）**：已修复，真机通过。
- **Q2（Windows→Linux 多文件第二次「拼接文件时出错」）**：已修复，真机通过。
- **Q3（Windows→Linux 同名文件「替换」报 `ENOENT`）**：问题依旧，未修复。
  - 现象：目标目录存在同名文件，`Ctrl+V` 选「替换」仍报
    「获取文件 "/tmp/m590bridge-fuse-263923-13/DJI_20260412164952_0709_D.MP4" 的信息时出错：
    没有那个文件或目录」。
- **Q4（Nautilus「x」角标）**：问题依旧，未修复。
- **新 blocker（最高优先级）**：Q3 报错出现后，**Linux 无法再复制新内容到 Windows**——
  Linux 复制时剪贴板内容不会更新，**重启两端应用也不恢复**。怀疑 Q3 的替换失败把某处
  FUSE/会话/剪贴板状态卡死，导致后续剪贴板发布被阻断。
  - **已定位+修复（2026-08-27，第一轮）**：根因是已发布但从未被请求的虚拟 offer
    永久保留 `virtual_receive`/`virtual_batch_receive`，使
    `virtual_clipboard_active` 恒为 true，本地剪贴板轮询被永久阻塞。已加
    `VIRTUAL_PUBLISH_IDLE_TIMEOUT`（120s）兜底 detach。
  - **已定位+修复（2026-08-27，第二轮，真机复测 `a66ba2e` 后）**：120s 兜底虽最终触发
    （日志 `single_virtual_publish_idle_detached … since_publish_ms=120031`），但在此之前
    `ui-file-14168-5` 被发布但从未被请求，`is_current` 返回 `false` 后代码进入
    `single_clipboard_not_current_before_request_kept` 分支——该分支只记日志并 **保留**
    receive，导致每秒数百次 hot-loop、剪贴板被阻塞整整 120s。根因：never-requested 且
    剪贴板已移走时应立即 detach，而非等待 idle 兜底。已将单文件与批次两条
    `*_before_request_kept` 路径改为立即 `fuse_manager.clear()` +
    `cancel_file`/`cancel_linux_virtual_batch("clipboard replaced before request")` +
    `keep_current = false`（日志改为 `*_before_request_detached`）。120s idle 兜底保留
    作为 backstop（理论上不再先命中）。

结论：串行重开本身（Q1/Q2）已在真机验证有效；Q3 连锁故障（发布后卡死 120s）已在第二轮
代码修复，待真机复测；剩余 Q3 替换 `ENOENT` 与 Q4 角标为待真机排查开放项。

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
- 二轮（2026-08-27）：`cargo test -p m590-daemon --lib`：72 passed，0 failed，2 ignored
  （新增 `virtual_publish_idle_detaches_unrequested_offers`）。
- 二轮：`cargo check --workspace`、`cargo check -p m590-daemon --target
  x86_64-pc-windows-gnu --lib`、`cargo clippy -p m590-{core,daemon} --lib --no-deps
  -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：全部通过。
- 三轮（2026-08-27，before-request 立即 detach）：`cargo test -p m590-daemon --lib`：
  72 passed，0 failed，2 ignored；`cargo test -p m590-daemon virtual_file`：22 passed，
  2 ignored；`cargo test -p m590-daemon linux_virtual`：18 passed，1 ignored；
  `cargo test -p m590-core file`：18 passed，0 failed。
  `cargo check --workspace`、`cargo check -p m590-daemon --target
  x86_64-pc-windows-gnu --lib`、`cargo clippy -p m590-{core,daemon} --lib --no-deps
  -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：全部通过。
- 待真机验收：Q3 连锁修复（替换失败后本地剪贴板立即恢复，不再卡 120s）、Q3 替换
  `ENOENT`、Q4 角标。

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

- 真机复测 Q3 连锁修复：在 Windows→Linux 触发替换 `ENOENT` 后，确认本地剪贴板**立即**
  恢复轮询、可重新复制内容到 Windows（不再卡 120s；日志应出现
  `single_clipboard_not_current_before_request_detached`）。
- 真机排查 Q3 替换 `ENOENT`：用 FUSE 日志确认是否为替换流程中 reader 未完成时 release
  触发 `BridgeEvent::Cancel` → FUSE 卸载，随后 Nautilus 再 stat 而报错；若是，评估将
  「已请求但未完成」的 release 改为保留挂载以支持重开，而非立即 cancel。
- 真机排查 Q4：对比落地文件与本机文件的 `stat`/xattr/MIME（尤其 `atime/mtime`、
  `perm 0o444`），消除 Nautilus「x」角标或记录为无害已知现象。
- Q1/Q2 已真机通过，不再重复验证。
