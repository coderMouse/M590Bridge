# task-006 · 双端文本同步最小路径（daemon + 传输）

## 状态

`completed`

## 目标

把已有 **协议会话**、**帧编解码**、**Linux 文本剪贴板** 串成最小可用路径：两台设备（或本机双进程）配对后，一端复制文本，另一端可写入系统剪贴板。  
允许先做 **本机 loopback TCP**；不要求 UI、mDNS、文件传输、加密定稿。

## 背景

- task-003：Message / Session / frame / MemoryPipe
- task-004/005：PlatformClipboard（Linux/Windows 已验证）
- MVP 核心用户价值：跨机粘贴文本

## 允许修改

- `crates/m590-net/**`（TCP listener/dial、读写帧）
- `crates/m590-daemon/**`（CLI：listen/connect/code、主循环）
- `crates/m590-clipboard/**`（仅当需小幅 API 调整）
- `crates/m590-core/**`（仅当会话/消息需小修）
- `docs/discovery/*`、`docs/domain/*`、`docs/plans/current.md`、本 task

## 禁止修改

- 完整文件/图片通道
- Tauri / `ui/` 大改
- Android
- 公网中继
- 无关大重构
- git commit（除非用户明确要求）

## 验证命令

```bash
cargo test
cargo build
# 本机双进程：
cargo run -p m590-daemon -- listen --code 123456 --port 19591 --expect-text hi --timeout-secs 15
cargo run -p m590-daemon -- connect --code 123456 --addr 127.0.0.1:19591 --push-text hi --timeout-secs 15
```

## 完成标准

- [x] 至少一种传输：loopback TCP（推荐）可收发协议帧
- [x] 配对成功后，文本从一端到另一端（剪贴板写入或明确日志 + 可选写入）
- [x] 有真实命令与输出摘要；失败则记 blocker
- [x] 不把密钥/本机私有路径写入共享 docs
- [x] plan / discovery / 本 task 已更新

## 实施记录

### 修改文件

- `crates/m590-net/src/tcp.rs`（新建：TcpFrameStream / listen / connect）
- `crates/m590-net/src/frame.rs`（`try_decode_frame`）
- `crates/m590-net/src/lib.rs`
- `crates/m590-core/src/session.rs`（`last_clipboard_text`）
- `crates/m590-daemon/src/main.rs`（`listen` / `connect` CLI + 同步循环）
- `docs/domain/protocol-draft.md`
- `docs/discovery/*`、`docs/plans/current.md`、本 task
- `docs/tasks/task-007.md`（下一任务：硬化）

### 验证结果

- 命令：`cargo test`
  - 结果：**通过**（含 `tcp_loopback_pairs_and_syncs_clipboard_text`）
- 命令：本机双进程 protocol-only
  ```bash
  cargo run -p m590-daemon -- listen --code 424242 --port 19591 --expect-text <marker> --timeout-secs 15 --no-clipboard
  cargo run -p m590-daemon -- connect --code 424242 --addr 127.0.0.1:19591 --push-text <marker> --timeout-secs 15 --no-clipboard
  ```
  - 结果：两端 exit 0；listen 侧 `paired=ok` / `sync_rx` / `expect_text=ok`；connect 侧 `push_text=ok`
- 命令：本机双进程 + OS 剪贴板（Wayland）
  - 结果：listen 侧 `clipboard_write=ok` + `expect_text=ok`；connect 侧 `clipboard_open=ok backend=Wayland`

### 文档影响

- 已更新：本 task、plan、discovery、commands、protocol-draft、task-007
- 无需更新：UI
- 待补：跨物理机（Linux↔Windows）实网验证；重连与 content_id 去重硬化

### 风险 / blocker

- 仅接受 **一个** peer 连接；断线不自动重连
- 剪贴板仍为轮询；echo 抑制为简单 last-text 跳过
- 无加密；配对码明文
- 默认绑定 `0.0.0.0`（局域网可用，需防火墙放行）

### 下一步

- **task-007**：心跳/重连/去重硬化，或 Linux↔Windows 实机联调
