# task-007 · 同步路径硬化与跨机验证

## 状态

`completed`

## 目标

在 task-006 最小 TCP 文本同步之上，提升可用性：心跳、断线感知、content_id 去重/回环抑制，并尽量完成 **Linux↔Windows** 实机联调记录。

## 背景

- **用户决定（2026-08-04）**：跨机实机联调等有界面后再做；本 task 可先做硬化与本机回归，跨机验收可标延期
- 已有 `listen`/`connect` + 帧 TCP + 剪贴板写入
- 已知风险：单次连接、轮询 echo、无加密

## 允许修改

- `crates/m590-net/**`、`crates/m590-daemon/**`、`crates/m590-core/**`（会话小改）
- `crates/m590-clipboard/**`（仅 API 微调）
- docs：discovery / domain / plan / 本 task

## 禁止修改

- 文件/图片完整通道
- Tauri 大壳
- Android
- 公网中继
- git commit（除非用户要求）

## 验证命令

```bash
cargo test
cargo run -p m590-daemon -- listen --code ... --port ... --expect-text ...
cargo run -p m590-daemon -- connect --code ... --addr ... --push-text ...
```

## 完成标准

- [x] 心跳或等价保活/断线检测至少一种
- [x] 回环/重复 content_id 有明确策略与测试
- [x] 双进程回归仍通过
- [x] 跨机验证：按产品决定延期到 UI 后（已写明）
- [x] 文档已更新

## 实施记录

### 修改文件

- `crates/m590-core/src/session.rs`
  - 心跳：`outstanding_heartbeat_seq` + `missed_heartbeat_acks` + `peer_heartbeat_suspect`
  - 去重：`seen_content_ids`（容量 64）
  - `QueueClipboardResult` / `InboundClipboardResult`
- `crates/m590-core/src/lib.rs`（导出）
- `crates/m590-daemon/src/main.rs`
  - 每 2s `HeartbeatTick`
  - 15s 无消息 → peer idle timeout
  - miss≥3 → heartbeat suspect
  - TCP `Disconnected` → 干净退出
  - 去重/echo 日志
- docs：本 task、plan、discovery、protocol-draft 策略摘要

### 验证结果

- `cargo test`：**通过**（新增 heartbeat miss、content_id dedup 单测）
- 双进程回归：
  - listen `--expect-text` + connect `--push-text` → 两端 exit 0
  - `paired=ok` / `push_text=ok` / `sync_rx` / `expect_text=ok`
- 跨机实机：**延期到有界面后**（用户 2026-08-04 决定）

### 策略说明

| 项 | 策略 |
|----|------|
| 心跳 | Connected 后每 2s 发送；未 ack 累计 miss；≥3 判定 suspect 并结束会话循环 |
| 空闲 | 任意对端消息刷新；15s 无消息 → idle timeout |
| 断线 | `TcpError::Disconnected` → `SessionEvent::Disconnect` + 报错退出 |
| content_id | 发送/接收均记入 seen；重复 id 不应用、不重发 |
| 回环 | 与 `last_clipboard_text` 相同的文本不送出（`UnchangedText`） |

### 文档影响

- 已更新：本 task、`docs/plans/current.md`、`docs/discovery/*`、`docs/domain/protocol-draft.md`
- 无需更新：UI
- 待补：跨机实机（UI 后）；自动重连仍未做

### 风险 / blocker

- **无自动重连**（断线需重启 listen/connect）
- 心跳为应用层；极端 NAT 静默丢包仍可能靠 idle 发现
- 跨机实机延期

### 下一步

- 新建 UI/托盘接入 task，或继续自动重连/配置文件
