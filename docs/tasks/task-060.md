# task-060 · 重复粘贴与替换/图标回归

## 状态

`completed`（2026-09-01；2026-09-02 补充验证）—— Q1/Q2/Q3/Q4 均真机通过，七轮修复（忽略旧轮在途 FileComplete）后 mp4/多个 pdf 替换通过。**原标记为「已知绕行」的「目录 + 其他文件混合批次替换」已于 2026-09-02 真机复测通过**（场景 A 无冲突粘贴 + 场景 B 替换已存在同名条目，均成功）—— 七轮代码已覆盖此场景，八轮的「条目级失败隔离」改动不需要。日常使用无需规避。

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
- [x] Windows → Linux 同名文件「替换」：不再 `ENOENT`，内容正确覆盖。（2026-08-28 真机通过）
- [x] Windows → Linux 粘贴落地文件：Nautilus 无异常「x」图标。（2026-08-27 真机通过）
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

结论：串行重开（Q1/Q2）与 Q3 连锁故障已真机通过；Q3 替换与 Q4 角标在第四轮找到真根因
（FUSE 权限只读）并修复，待真机复测。

## Q3 替换 ENOENT 与 Q4 角标修复（2026-08-27，第四轮——真根因）

真机复测发现第三轮的「稳定 URI」与「真实时间戳」修复**无效**（问题依旧）。用户手动排查
找到真根因：从 Windows 粘贴到 Linux 的落地文件/文件夹权限为**只读**（`0o444` /
`0o555`）。

- **Q4（Nautilus「x」角标）真根因**：Nautilus 从 FUSE 复制时会**保留源文件权限**。FUSE
  虚拟文件 `perm = 0o444`（只读），落地后真实文件也是只读 → Nautilus 对只读文件显示「不可
  写」限制角标（即「x」）。用户手动改为读写后角标消失。
- **Q3（替换「权限不够」）真根因**：Nautilus「替换」流程需**写入/删除目标文件**，但目标
  文件是从 FUSE 继承的只读权限（`0o444`），Nautilus 无法删除/覆盖 → 报
  「打开文件 "/home/huang/视频/Node.txt" 出错：权限不够」。本机复制粘贴的文件权限正常
  （读写），故替换无碍。第三轮日志中的 `ENOENT` 是替换流程在权限检查失败后的二次表现。
- **修复**：FUSE 虚拟文件权限从 `0o444` → `0o644`（owner 读写），目录从 `0o555` →
  `0o755`（owner 读写执行）。FUSE 挂载本身仍 `MountOption::RO`（只读），但 `perm` 位决定
  Nautilus 复制后落地文件的权限。改为 `0o644`/`0o755` 后落地文件可读写 → Nautilus 不再
  显示「x」角标、替换流程可正常删除/覆盖目标文件。
- **代码清理**：撤销第三轮添加的 `remount`/`take_mount_point`/`reuse_mount_point` 与
  `published: SystemTime` 时间戳机制（经真机验证无效，属无用代码），恢复
  `linux_virtual_file.rs` 与 `linux_virtual_file_manager.rs` 到 `796b8eb` 基线后仅改权限。
## 第五轮（2026-08-27）：Q3 缩略图探测 + 权限确认

真机复测确认：权限修复（`0o644`/`0o755`）后 Q4「x」角标消失、txt/apk/zip 可正常替换。
但 **mp4/pdf 替换仍报 `ENOENT`**，txt/apk/zip 成功。根因：Nautilus 为 mp4/pdf 生成缩略图，
打开 FUSE 文件读几 KB 后关闭 → `release_reader` 发送 `BridgeEvent::Cancel`（因 `!consumed`）
→ hub `Cancel` 处理器调用 `fuse_manager.clear()` 卸载 FUSE → 随后 Nautilus「替换」流程
`stat` 源 FUSE 路径时挂载已卸载 → `ENOENT`。txt/apk/zip 不生成缩略图故不触发此路径。

**修复**：
- `crates/m590-core/src/session.rs`：新增 `cancel_file_stream`——通知对端停止网络流但**保留
  `inbound_offers`**，使后续串行重开可重新请求同一 `transfer_id`。`abort_active_outbound_if`
  /`abort_active_outbound` 在 `retain_for_clipboard` 为真时重新 stage 源（与完成路径一致），
  使对端取消后仍能响应第二次 `FileRequest`。
- `crates/m590-daemon/src/hub.rs`：单文件与批次两条 `BridgeEvent::Cancel` 路径增加
  **缩略图探测判定**——当 `reason == "virtual file reader closed" && !completed` 时走
  软取消：`producer.fail()` + `cancel_file_stream` + 重置 round-local 状态
  （`requested=false` 等）供重开，**不调用** `fuse_manager.clear()`、不丢弃 receive。
  网络分片处理器增加 `!requested` 守卫，丢弃软取消后到达的陈旧分片。
- 权限已为 `0o644`（文件）/`0o755`（目录），`uid`/`gid` 为 `request.uid()`（当前用户）。

- - **120s `VIRTUAL_PUBLISH_IDLE_TIMEOUT` 与大文件传输**：idle 判定条件是
  `!requested && !completed && elapsed >= 120s`。一旦 OS 打开 FUSE 文件 `requested=true`
  即不再命中 idle；正在传输的大文件不受影响。

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
- 四轮（2026-08-27，FUSE 权限修复 + 撤销无用代码）：`cargo test -p m590-daemon
  --lib`：72 passed，0 failed，2 ignored；`cargo test -p m590-daemon virtual_file`：
  22 passed，2 ignored；`cargo test -p m590-daemon linux_virtual`：18 passed，
  1 ignored；`cargo test -p m590-core file`：18 passed，0 failed。
  `cargo check --workspace`、`cargo check -p m590-daemon --target
  x86_64-pc-windows-gnu --lib`、`cargo clippy -p m590-{core,daemon} --lib --no-deps
  -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：全部通过。
- 五轮（2026-08-27，缩略图探测软取消）：修复 4 个实现缺陷：
  (1) `open_reader` 的 `cancelled && !consumed` 提前返回阻止重开——改为
  `cancelled && !consumed && !finished`，使 `producer.fail()` 后 `prior_round`
  重置可运行；
  (2) `on_file_cancel` 无条件删除 `staged_outbound_files`——改为
  `retain_for_clipboard` 时保留，使对端可响应第二次 `FileRequest`；
  (3) 软取消条件用 `!completed` 而非 `!consumed`——小文件 `StreamCompleted`
  先到设 `completed=true` 导致软取消不触发、走硬取消卸载 FUSE；改用
  `!consumed`（读者未读完即触发）；
  (4) 软取消后 `pending_virtual_chunk` 对 `fail()` 的 producer 无限重试——
  软取消时清空；并对单文件/批次 `StreamCompleted` 增加 `!requested` 陈旧守卫。
  `cargo test -p m590-core --lib`：39 passed；
  `cargo test -p m590-daemon --lib`：73 passed，2 ignored；`cargo test -p m590-daemon
  virtual_file`：23 passed，2 ignored；`cargo test -p m590-daemon linux_virtual`：18 passed，
  1 ignored。
  `cargo check --workspace`、`cargo check -p m590-daemon --target
  x86_64-pc-windows-gnu --lib`、`cargo clippy -p m590-core -p m590-daemon --lib --no-deps
  -D warnings`、`cargo fmt --all -- --check`：全部通过。
- 待真机验收：Q3 mp4/pdf 替换（缩略图探测软取消后 FUSE 不卸载、可重开）。Q4 角标已真机
  确认消失。
- 六轮（2026-08-28，mp4/pdf 替换修复）：定位并修复「旧网络流在途时 FUSE 重开」的根因：
  - **根因**：Nautilus 缩略图探测（mp4/pdf）打开 FUSE 文件读到一小段后 release，只标
    `released` 不取消（防卸载），网络流继续在途；用户替换粘贴重开 FUSE 再发 `FileRequest`
    时，发送端 `active_outbound` 仍在，按协议回 `FileComplete(false, "sender busy with
    another transfer")`，接收端 `producer.fail` → FUSE `read` EIO → Nautilus 报
    「拼接文件时出错：输入/输出错误」。txt/apk/zip 无缩略图探测，故正常。
  - **hub 单文件/批次**（`hub.rs`）：`BridgeEvent::Request` 中若
    `network_started_at.is_some() && !completed`（单文件）或
    `active_index == Some(index) && !completed`（批次 entry），即旧轮仍在途，先
    `session.cancel_file_stream(tid, "stream reopened while previous stream active")`
    并 flush outbox，丢弃 `pending_virtual_chunk`，重置轮次状态，再发新 `FileRequest`；
    dispatch 成功后在 `producer.start()` 前调用 `producer.arm()`。
  - **bridge**（`virtual_file_bridge.rs`）：`PipeState` 新增 Linux `resetting` 门控——
    `open_reader` 在 prior_round 重置时置位，`try_push` 在重置中直接返回 `Ok(false)`
    （backpressure，不留旧字节进管道）；新增 `PipeProducer::arm()`，hub 在新请求发出后
    清除门控。这样重开与 re-arm 之间的任何旧轮数据都无法进入新 reader。
  - **core 接收端**（`session.rs`）：`on_file_chunk` 对「无 incoming 文件、transfer 在
    `stream_inbound`（已 re-arm）、offset != 0」的旧轮在途续传 chunk 静默丢弃，而不是走
    「first chunk offset must be 0」失败路径清掉 offer。
  - **陈旧守卫**：Linux 单文件 `Failed` 增加 `!requested` 守卫（与 `StreamCompleted` 一致）；
    批次 `Failed` 对已软取消 entry 忽略。旧轮被 abort 时不回发任何 complete（
    `abort_active_outbound` 仅 re-stage），故不存在旧轮 complete 污染新轮的常规路径。
  - 新增单测：`virtual_file_bridge::reopened_reader_ignores_old_round_pushes_until_armed`
    （Linux）、`session::stale_continuation_chunk_is_dropped_after_stream_rearm`。
  - 验证：`cargo test -p m590-core --lib`：40 passed；`cargo test -p m590-daemon --lib`：
    74 passed，2 ignored；`cargo test -p m590-daemon virtual_file`：24 passed，2 ignored；
    `cargo test -p m590-daemon linux_virtual`：18 passed，1 ignored。`cargo check
    --workspace`、`cargo check -p m590-daemon --target x86_64-pc-windows-gnu --lib`、
    `cargo clippy -p m590-core -p m590-daemon --lib --no-deps -- -D warnings`、
    `cargo fmt --all -- --check`、`git diff --check`：全部通过。
- 待真机复测：Q3 mp4/pdf 替换修复（Windows→Linux，目录已有同名文件选「替换」）。
- 真机复测（2026-08-28，用户）：mp4 替换成功；`mm 450 (Chivoc) - App Feedback Basic.pdf`
  替换成功；`MES需求解决方案优化汇报.pdf` 替换仍报「拼接文件时出错：输入/输出错误」，
  失败后目标文件损坏无法打开。
- 七轮（2026-08-28，旧轮 complete 残留修复）：定位二轮真机失败的根因——失败 pdf 的
  旧网络轮次在 FUSE 重开前已在线上传完，其 `FileComplete(ok=true)` 与旧轮尾 chunk
  一起积压在 recv 缓冲区；re-arm（cancel + 新 `FileRequest`）先执行，随后旧轮 complete
  到达。`on_file_complete` 发现无 incoming 文件，按「unknown transfer / complete
  without data」失败并删除 offer，直接杀掉新轮：FUSE `read` 中断 → Nautilus 报
  「拼接文件时出错」且目标文件只写入部分。成功案例（mp4/另一 pdf）旧轮仍在途阻塞，
  无 complete 积压，故不受影响。
  - **修复**（`session.rs` `on_file_complete`）：ok=true 的 complete 到达时若无 active
    incoming、该 transfer 在 `stream_inbound`（已 re-arm 等待新轮首块），视为旧轮残留
    并静默忽略（保留 offer），排除空文件 legitimate complete 场景。非 stream 与
    ok=false 路径行为不变（新轮真失败仍需透传）。
  - 新增单测：`session::stale_old_round_complete_is_ignored_after_stream_rearm`。
  - 验证：`cargo test -p m590-core --lib`：41 passed；`cargo test -p m590-daemon --lib`：
    74 passed，2 ignored；`cargo check -p m590-daemon --target
    x86_64-pc-windows-gnu --lib`、`cargo clippy -p m590-core -p m590-daemon --lib
    --no-deps -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：全部通过。
- 真机复测（2026-08-28，用户）：七轮修复后 mp4/单个 pdf 替换通过（
  「MES需求解决方案优化汇报.pdf」不再失败），用户确认「测试通过」。
- 八轮（2026-08-28，「目录 + mp4」批次替换，已回滚）：尝试条目级失败隔离（某 entry
  网络流失败时不再卸载整棵 FUSE 树，commit 3ec0dd9）。真机复测仍有问题，按用户决定
  **已回滚到七轮版本 8593776**，八轮代码不再保留；`3ec0dd9` 已从分支移除。
  日常规避：文件夹与其他文件分开复制。
- 待真机复测：无。当前按七轮版本使用；若后续要继续修「文件夹+其他文件」批次替换，
  另开新 task/记录。

## 文档影响

- 已更新：本 task、`docs/plans/current.md`、`docs/domain/protocol-draft.md`、`AGENTS.md`
  当前阶段说明（同一 offer 可串行重开，但并发/断点续传/替换后重开仍不保证）。
- 无需更新：协议 wire 字段、Hub HTTP API、UI 交互、安装器与自启均未改变。

## 风险

- 重开生命周期可能影响取消、替换、断线后的清理，需保留现有清理路径不漏不串。
- 永久保留发送源可能造成文件句柄/路径泄漏，需区分剪贴板来源与普通 API 来源。
- FUSE `getattr`/`lookup` 在替换流程下的行为需以真机日志为准，避免回归 task-058 已通过
  的目录树、空文件、取消、断线场景。
- 残留边界（已记录不保证）：旧轮与新一轮在极窄窗口内（首块 0 号 chunk 仍在途时被取消）
  可能产生理论上等价的重复首块；旧轮 complete 恰与 re-arm 交叉时的处理以真机复测为准。
- 旧轮 `FileComplete(ok=false)` 在 re-arm 后到达仍会透传失败（现有发送端不再产生
  busy 拒绝，此路径为防御性保留）。

## 下一步

无。task-060 已完成（2026-09-01）。Q1/Q2/Q3/Q4 均已真机通过。**2026-09-02 补充验证**：原标记为「已知绕行」的「目录 + 其他文件混合批次替换」场景 A（无冲突）+ 场景 B（替换同名）均已真机通过，七轮代码已覆盖此场景，无需另开新 task。
