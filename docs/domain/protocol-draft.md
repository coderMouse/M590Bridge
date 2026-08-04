# 协议草案 · M590Bridge

> 状态：draft（task-003 + task-014 图片）  
> 范围：局域网 1 对 1；文本 + 小图剪贴板；文件分片仍后置

## 版本

- 应用协议版本：`PROTOCOL_VERSION = 1`
- 帧魔数：ASCII `M590`

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
| 10 | ClipboardImage | device_id, content_id, width u32, height u32, rgba bytes |

`ClipboardImage` 的 `rgba` 为 row-major **RGBA8**，长度必须为 `width * height * 4`。

所有业务消息在类型上携带或保留 `DeviceId`，便于以后扩展；**运行时 MVP 仅 1 peer**。

## 会话状态

`Disconnected → Pairing → Connected`（也可因 reject/goodbye/disconnect 回到 Disconnected）。

实现：`m590_core::Session` + `SessionEvent`。  
帧编解码：`m590_net::encode_frame` / `decode_frame` / `try_decode_frame`。  
内存联调：`MemoryPipe`；TCP：`TcpFrameStream` + daemon `listen`/`connect`。

## 非目标（仍后置）

- QUIC / 公网中继
- 加密套件定稿（见 open-questions Q7）
- 文件分片 / 按需拉文件
- 多 peer 网格

## 传输（task-006）

- 载体：TCP，一连接一会话（MVP 单 peer）
- 字节流上连续拼接多个完整帧（`try_decode_frame`）
- 实现：`m590_net::TcpFrameStream`；daemon 命令：`listen` / `connect`
- 配对：host listen 持有 pairing code；joiner connect 发送 Hello + PairRequest
- 文本：Connected 后发送 `ClipboardText`；接收方可写 OS 剪贴板
- 图片（task-014）：Connected 后发送 `ClipboardImage`；超限则日志 skip，不分片

## 运行时硬化（task-007）

- **心跳**：Connected 后定期 `Heartbeat` / `HeartbeatAck`；连续未 ack 达到阈值则会话视为 peer suspect
- **空闲**：长时间无任何对端帧则超时断开
- **content_id 去重**：同一 id 只应用一次（收发两侧，文本与图片共用）
- **回环抑制**：与最近一次剪贴板文本/图片指纹相同则不再发送
