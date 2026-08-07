# 协议草案 · M590Bridge

> 状态：draft（至 **task-035** 发布硬化）
> 范围：局域网 1 对 1；文本 + 图片（RGBA/PNG）；**文件 offer/request/chunk/complete（磁盘流 + SHA-256）**

## 版本

- 应用协议版本：`PROTOCOL_VERSION = 2`
- 帧魔数：ASCII `M590`

版本 2 在 `FileOffer` / `FileComplete` 中包含 SHA-256 字段；版本 1 帧会在帧头解码阶段返回 `UnsupportedVersion(1)`，不进入 payload 解码。

## 帧布局

| 偏移 | 长度 | 字段 |
|------|------|------|
| 0 | 4 | magic `M590` |
| 4 | 1 | version |
| 5 | 1 | msg_type |
| 6 | 2 | reserved (0) |
| 8 | 4 | payload_len (BE u32) |
| 12 | N | payload |

字符串字段：`u32 BE 长度 + UTF-8 字节`。  
原始字节字段：`u32 BE 长度 + bytes`。  
`u32` / `u64` 字段：大端。

最大 payload：`MAX_PAYLOAD_LEN = 16 MiB`。  
内联图片软上限：`Session::INLINE_IMAGE_MAX_BYTES = 12 MiB`（超出则发送侧 skip）。  
文件软上限：`Session::MAX_FILE_BYTES = 8 GiB`（**不是**整文件内存 cap）。
内存/base64 offer 上限：`MAX_MEMORY_FILE_BYTES = 64 MiB`。
分片：`FILE_CHUNK_SIZE = 256 KiB`；每轮泵送最多 `OUTBOUND_CHUNKS_PER_PUMP = 4` 片。

## 消息类型（msg_type）

| 值 | 名称 | 用途 |
|----|------|------|
| 1 | Hello | 握手：device_id, app_version |
| 2 | HelloAck | 握手应答 |
| 3 | PairRequest | 配对：device_id, pairing_code |
| 4 | PairAccept | 配对成功 |
| 5 | PairReject | 配对失败 + reason |
| 6 | Heartbeat | seq |
| 7 | HeartbeatAck | seq |
| 8 | ClipboardText | device_id, content_id, text |
| 9 | Goodbye | device_id, reason |
| 10 | ClipboardImage | device_id, content_id, width u32, height u32, encoding u8 (0=RGBA,1=PNG), data bytes |
| 11 | FileOffer | device_id, transfer_id, file_name, size u64, **sha256_hex string** |
| 12 | FileRequest | device_id, transfer_id |
| 13 | FileChunk | device_id, transfer_id, offset u64, data bytes |
| 14 | FileComplete | device_id, transfer_id, ok u8 (0/1), message string, **sha256_hex string** |

`ClipboardImage`：`encoding=0` 时 data 为 row-major **RGBA8**（长度 `width*height*4`）；`encoding=1` 时 data 为 **PNG**（推荐，截图更小）。

`sha256_hex`：小写 hex，长度 0（未提供）或 64。Offer 可空（路径流式边发边算）；Complete 成功时携带实际 digest。

### 文件通道（task-020..033）

按需拉取 + 流式：

1. 发送方 `offer_file_path` / `offer_file`：发 `FileOffer`（basename only）
2. 接收方 `request_file` → `FileRequest`
3. 发送方按 `FILE_CHUNK_SIZE` 分批 `FileChunk`（`pump_outbound_file`，有界背压），再 `FileComplete(ok=1, sha256=…)`
4. 接收方边收边写 **`.part`**，校验 size + SHA-256 后交给 hub `finalize_part_file` 原子落到保存目录
5. 空文件：无 chunk，直接 Complete
6. 失败：`FileComplete(ok=0)` 或 `InboundFileResult::Failed`；清理 `.part`

**本刀取舍**：仍走**同一 TCP 控制/数据连接**串行帧（未开独立数据连接）；靠分批 pump 避免一次把整文件 chunk 堆进 outbox，心跳/剪贴板可在 pump 间隙处理。
**仍未做**：文件夹、OS 文件剪贴板、断点续传、多文件并行、独立数据连接。

**hub**：自动 request + `.partial` → 保存目录；`POST /api/send_file`（路径流式）；`send_file_bytes` 仍限内存 cap。

所有业务消息在类型上携带或保留 `DeviceId`，便于以后扩展；**运行时 MVP 仅 1 peer**。

## 本机 Hub 控制 API（task-035）

- 默认地址：`http://127.0.0.1:5910`，桌面壳仅接受 loopback API 地址。
- Hub 每个进程使用随机 256 位令牌；除允许来源的 `OPTIONS` 预检外，所有 API 请求都必须携带 `X-M590-Token`。
- Tauri WebView 通过受限 command 获取内嵌 Hub 的进程临时令牌；独立 daemon 可用 `M590_HUB_TOKEN` 注入，未注入时只输出到当前终端。
- CORS 仅回显 Tauri origin；debug 构建额外允许带有效端口的 localhost/loopback 开发 origin。无 `Origin` 的 CLI 请求仍必须鉴权。
- `send_file_bytes` 的源文件上限为 4 MiB；HTTP reader 为 Base64 JSON 放大预留空间，并在读取 body 前校验 `Content-Length`。

## 会话状态

`Disconnected → Pairing → Connected`（也可因 reject/goodbye/disconnect 回到 Disconnected）。

实现：`m590_core::Session` + `SessionEvent`。  
帧编解码：`m590_net::encode_frame` / `decode_frame` / `try_decode_frame`。  
内存联调：`MemoryPipe`；TCP：`TcpFrameStream` + daemon `listen`/`connect`。

## 非目标（仍后置）

- QUIC / 公网中继
- 加密套件定稿（见 open-questions Q7）
- 文件夹 / OS 文件剪贴板 / 断点续传 / 多文件并行
- 多 peer 网格

## 传输（task-006）

- 载体：TCP，一连接一会话（MVP 单 peer）
- 字节流上连续拼接多个完整帧（`try_decode_frame`）
- 实现：`m590_net::TcpFrameStream`；daemon 命令：`listen` / `connect`
- 配对：host listen 持有 pairing code；joiner connect 发送 Hello + PairRequest
- 文本：Connected 后发送 `ClipboardText`；接收方可写 OS 剪贴板
- 图片（task-014..018）：Connected 后发送 `ClipboardImage`（优先 PNG）；超限 `ImageTooLarge` / `last_error`；TCP send 前恢复 blocking
- 文件（task-020..033）：`FileOffer` → `FileRequest` → 流式 `FileChunk`* → `FileComplete` + SHA-256

## 运行时硬化（task-007）

- **心跳**：Connected 后定期 `Heartbeat` / `HeartbeatAck`；连续未 ack 达到阈值则会话视为 peer suspect
- **空闲**：长时间无任何对端帧则超时断开
- **content_id 去重**：同一 id 只应用一次（收发两侧，文本与图片共用）
- **回环抑制**：与最近一次剪贴板文本/图片指纹相同则不再发送
