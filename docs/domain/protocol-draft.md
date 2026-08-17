# 协议草案 · M590Bridge

> 状态：draft（至 **task-056** 批次清单与串行运行时）
> 范围：局域网 1 对 1；文本 + 图片（RGBA/PNG）；**文件 offer/request/chunk/complete（磁盘流 + SHA-256）**；多文件/目录批次清单与串行传输

## 版本

- 应用协议版本：`PROTOCOL_VERSION = 3`
- 帧魔数：ASCII `M590`

版本 3 在版本 2 的 SHA-256 文件字段基础上增加 `FileCancel`；task-055 在不改变既有消息
字段的前提下增加 type 16 `FileBatchOffer`。版本 1/2 帧会在帧头解码阶段返回
`UnsupportedVersion`，未知消息类型会返回 `UnknownMessageType`。

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
图片声明尺寸与 PNG 实际解码尺寸的像素上限：`MAX_IMAGE_PIXELS = 16 Mi`；PNG 解码还受分配上限保护，不能只按压缩字节数判断安全。
文件软上限：`Session::MAX_FILE_BYTES = 8 GiB`（**不是**整文件内存 cap）。
内存/base64 offer 上限：`MAX_MEMORY_FILE_BYTES = 64 MiB`。
分片：`FILE_CHUNK_SIZE = 256 KiB`；每轮泵送最多 `OUTBOUND_CHUNKS_PER_PUMP = 4` 片；单个 `FileChunk` 不得超过 256 KiB。
`transfer_id`：最多 128 字节，只允许 ASCII 字母、数字、`.`、`_`、`-`，且不能为 `.` / `..`；它只能作为接收临时目录下的单路径组件。

批次清单限制：`batch_id` 和 `entry_id` 均为安全单路径组件；相对路径统一使用 `/`，拒绝绝对路径、Windows 盘符/UNC、反斜杠、空组件、`.`、`..` 和 NUL。单批次最多 4096 个条目、路径最多 64 层、编码清单最多 4 MiB、文件总字节最多 8 GiB。条目类型为 `file` 或 `directory`；目录的 `size` 固定为 0 且 `sha256_hex` 必须为空。清单还拒绝重复 entry id 和相对路径。task-056 将文件条目的 `entry_id` 作为现有单文件请求链路的安全传输 id；发送端用随机批次 nonce + 递增序号保证活动会话内唯一。

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
| 15 | FileCancel | device_id, transfer_id, message string |
| 16 | FileBatchOffer | device_id, batch_id, display_name, entry_count u32, entries (`entry_id`, `relative_path`, `kind` u8: 0=file/1=directory, `size` u64, `sha256_hex`) |

`ClipboardImage`：`encoding=0` 时 data 为 row-major **RGBA8**（长度 `width*height*4`）；`encoding=1` 时 data 为 **PNG**（推荐，截图更小）。

`sha256_hex`：小写 hex，长度 0（未提供）或 64。Offer 可空（路径流式边发边算）；Complete 成功时携带实际 digest。

### 文件通道（task-020..033）

按需拉取 + 流式：

1. 发送方 `offer_file_path` / `offer_file`：发 `FileOffer`（basename only）
2. Linux/磁盘接收端 `request_file` → `FileRequest`；Windows OLE 接收端先发布元数据，Explorer 首次请求 `CFSTR_FILECONTENTS` 时才调用 `request_file_stream` → `FileRequest`
3. 发送方按 `FILE_CHUNK_SIZE` 分批 `FileChunk`（`pump_outbound_file`，有界背压），再 `FileComplete(ok=1, sha256=…)`
4. 接收方边收边写 **`.part`**，校验 size + SHA-256 后交给 hub `finalize_part_file` 原子落到保存目录
5. 空文件：无 chunk，直接 Complete
6. 失败：`FileComplete(ok=0)`、`FileCancel` 或 `InboundFileResult::Failed`；磁盘目标清理 `.part`，Windows 流目标清理有界内存管道

安全边界（task-037）：接收端把待接收 offer 与进行中接收的总预留量限制在 `MAX_IN_FLIGHT_FILE_BYTES`（当前等于 8 GiB），创建 `.part` 前检查目标卷可用空间；同名临时文件使用不覆盖的创建方式。会话设置 `.partial` 目录时只清理直接子项中的残留 `.part` 文件。

**本刀取舍**：仍走**同一 TCP 控制/数据连接**串行帧（未开独立数据连接）；靠分批 pump 避免一次把整文件 chunk 堆进 outbox，心跳/剪贴板可在 pump 间隙处理。
Windows 单文件 OS 文件剪贴板现已接入：OLE STA 仅持有虚拟 `IDataObject`，网络 Session 线程负责分片和校验，有界管道提供背压。读取端关闭、剪贴板被替换或已开始读取后 30 秒无进展时发送 `FileCancel`；尚未粘贴的 offer 保留到剪贴板被替换或会话断开。`IStream` 只支持当前位置 no-op seek；向前/向后移动均明确失败。

### 批次运行时（task-056）

1. Hub 的 `/api/send_batch` 接受多个本地文件/目录路径；目录按平台无关相对路径稳定排序，
   不跟随 symlink，扫描结果构造成一个 `FileBatchOffer`。
2. 接收端验证清单后按 manifest 顺序一次只请求一个文件条目；每个条目继续复用
   `FileRequest → FileChunk → FileComplete`，不并行、不新增连接。
3. 完成条目先从 `.partial/<entry_id>.part` 移入 `.partial/<batch_id>.batch/` 暂存树；
   整批成功后才把单一顶层节点或多顶层容器发布到保存目录，同名目标使用后缀避让。
4. 新批次替换旧批次；`/api/cancel_batch`、对端取消、条目失败和断线均清理未完成
   `.part` 与批次暂存树。status 同时暴露批次文件数/总字节和当前条目进度。

**仍未做**：Windows OLE 多文件 `IDataObject`、Linux FUSE 虚拟目录树、断点续传、
多文件并行、独立数据连接。当前批次由 UI 手动选择/拖放后自动保存，OS 文件管理器
“复制多个文件/文件夹后在对端直接粘贴”分别留给 task-057/task-058。

**hub**：单文件自动 request/虚拟剪贴板；`POST /api/send_file`（路径流式）；
`send_file_bytes` 仍限内存 cap；`POST /api/send_batch` 扫描路径并串行传输；
`POST /api/cancel_batch` 取消当前批次。

所有业务消息在类型上携带或保留 `DeviceId`，便于以后扩展；**运行时 MVP 仅 1 peer**。

## 本机 Hub 控制 API（task-035）

- 默认地址：`http://127.0.0.1:5910`，桌面壳仅接受 loopback API 地址。
- Hub 每个进程使用随机 256 位令牌；除允许来源的 `OPTIONS` 预检外，所有 API 请求都必须携带 `X-M590-Token`。
- Tauri WebView 通过受限 command 获取内嵌 Hub 的进程临时令牌；独立 daemon 可用 `M590_HUB_TOKEN` 注入，未注入时只输出到当前终端。
- CORS 仅回显 Tauri origin；debug 构建额外允许带有效端口的 localhost/loopback 开发 origin。无 `Origin` 的 CLI 请求仍必须鉴权。
- `send_file_bytes` 的源文件上限为 4 MiB；HTTP reader 为 Base64 JSON 放大预留空间，并在读取 body 前校验 `Content-Length`。
- `send_batch` 的 JSON 为 `{"paths":["..."]}`；路径只在本机 Hub 解析，不在线上传输绝对路径。

## 会话状态

`Disconnected → Pairing → Connected`（也可因 reject/goodbye/disconnect 回到 Disconnected）。

实现：`m590_core::Session` + `SessionEvent`。  
帧编解码：`m590_net::encode_frame` / `decode_frame` / `try_decode_frame`。  
内存联调：`MemoryPipe`；TCP：`TcpFrameStream` + daemon `listen`/`connect`。

## 非目标（仍后置）

- QUIC / 公网中继
- 加密套件定稿（见 open-questions Q7）
- 多文件/文件夹 OS 剪贴板粘贴 / 断点续传 / 多文件并行
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
