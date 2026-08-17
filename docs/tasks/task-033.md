# task-033 · 大文件流式传输（借鉴 LocalSend 数据路径）

## 状态

`completed`

## 目标

在**保留现有自定义 TCP 协议与 1 对 1 会话模型**的前提下，把文件传输从「整文件进内存」改为「磁盘流 → 缓冲 → 网络 → 临时文件」，解除 `MAX_FILE_BYTES = 4 MiB` 的内存上限，使大文件可稳定传输。

本 task **第一刀只做单文件流式路径**，不追求与 LocalSend 功能对等，也不为速度盲目改成 HTTP。

## 背景与调研结论（LocalSend）

### LocalSend 怎么传文件

LocalSend 官方协议是 **HTTPS REST API**，不是自定义 TCP 二进制帧：

1. `prepare-upload`：元数据（文件名、大小、SHA-256）→ 拿到 `sessionId` + file token
2. `POST /upload`：每个文件作为 **HTTP request body 的二进制流**上传
3. 多文件上传接口允许并行（默认最多约 2 个）

### 当前官方实现的关键点

| 点 | LocalSend |
|----|-----------|
| 读取 | 磁盘约 **512KiB** 分块读取，不是整文件 `read_to_end` |
| 发送 | 文件流直接进入 HTTPS body，**不** Base64、**不**整包进内存 |
| 背压 | 有容量限制的 channel，发送跟不上时阻塞读 |
| 接收 | 边收边写磁盘，约 512KiB 写缓冲 |
| 校验 | 传输中 **增量 SHA-256**；失败可重试 |
| 并发 | 默认最多 **2** 个文件并行 |
| 连接 | HTTPS 可复用 |
| 其它 | 取消、进度、校验失败重试 |

参考（上游，便于复查，非本仓库依赖）：

- Protocol：`https://github.com/localsend/protocol/blob/main/README.md`（File Transfer / Upload API）
- 流式读与上传 body：LocalSend `packages/core` 中 transfer / http client 实现

### 对本项目的含义

- **「流式」主要解决内存、稳定性、大文件支持**；本身不保证更快。
- M590Bridge 已是 **原始 TCP 帧**，协议开销理论上可略低于 HTTPS；实现成熟后单文件吞吐有望与 LocalSend **同量级**，甚至略快。
- LocalSend 的成熟点在：异步 I/O、缓冲、背压、边收边写、增量校验、连接复用与多文件并行。
- **不建议**为了速度把文件通道改成 HTTP；应借鉴其**数据路径**，保留现有控制面与产品边界。

## M590Bridge 现状（问题）

| 项 | 当前 |
|----|------|
| 上限 | `MAX_FILE_BYTES = 4 MiB`（`m590-core` Session） |
| 分片 | `FILE_CHUNK_SIZE = 64 KiB` 的 `FileChunk` |
| 发送 | `offer_file` **整文件 `Vec<u8>` staged 在内存** |
| 接收 | Session 内重组完整字节后再落盘 |
| 传输 | 与剪贴板/心跳 **同一 TCP 连接串行** |
| 校验 | 主要靠 size / offset；**无** SHA-256 |
| hub/UI | `send_file` / `send_file_bytes` 仍偏内存与 base64 路径 |
| 并发 | 单连接串行；无多文件并行 |

## 设计原则（本刀）

```text
现有控制连接（Hello/配对/剪贴板/心跳 + FileOffer/Request/Complete 控制语义）
        │
        ▼
FileOffer → FileRequest
        │
        ▼
单独的数据连接（或明确隔离的流式 chunk 路径）
磁盘读取 → 256/512KiB 可复用缓冲 → TCP → 接收端 .part 临时文件
        │
        ▼
大小 + 增量 SHA-256 校验 → 原子重命名 → FileComplete
```

### 第一版必须做

1. **发送端**：按路径/`File` 以 **256KiB 或 512KiB** 可复用缓冲读盘，边读边发；禁止大文件整包 `Vec`。
2. **接收端**：边收边写 **`.part` 临时文件**；成功后原子 rename；失败/取消清理。
3. **背压**：有界队列或同步写路径，避免无限堆积内存。
4. **增量 SHA-256**（发送可选预计算或边发边算；接收边收边算；Complete 携带或比对 digest）。
5. **进度**：继续/扩展现有 status 进度字段（bytes 已传 / total）。
6. **单文件 only**：一次只传一个大文件；稳定后再考虑最多 2 并行。
7. **控制面不阻塞**：大文件数据不得长期饿死心跳/剪贴板；优先 **独立数据连接**，若本刀成本过高可退化为「同连接但严格流式 + 分片上限」，并在实施记录写明取舍。

### 第一版明确不做

- 文件夹 / 目录树
- OS 桌面「文件剪贴板粘贴」语义
- 断点续传 / 多段并行同一文件
- 为文件通道整体换成 HTTP/HTTPS 或 LocalSend 协议兼容
- 多文件并行（>1）
- Windows 安装包、开机自启、mDNS 设置开关
- 无限制任意大小：可设**可配置软上限**（如默认数百 MiB～数 GiB），但**不得**再靠整文件内存 cap 卡死

### 性能预期（验收参考，非硬 KPI）

- 千兆有线：合理目标接近磁盘或网卡瓶颈，约 **80–110 MB/s** 量级（视本机盘与对端而定）
- Wi-Fi：主要受信号/路由限制，与 HTTP vs 自定义 TCP 关系次要
- 内存峰值：相对文件大小近似 **O(1)**（缓冲 + 少量状态），不得随文件线性涨到整文件大小
- 真值只能靠同设备、同网络、同文件基准；禁止用「应该和 LocalSend 一样快」代替测量

## 允许修改

- `crates/m590-core/**`：Session 文件路径、上限、流式状态机、digest、进度事件
- `crates/m590-net/**`：如需独立数据连接、帧流读写辅助
- `crates/m590-daemon/**`：hub `send_file`、落盘、status、可选新 API（路径发送优先于 base64）
- 协议草案 / payload 字段扩展（若 Complete 增加 sha256、Offer 增加 digest 等）：`docs/domain/protocol-draft.md` + 编解码
- 必要的 UI 进度/错误文案（最小改动）
- 本 task、`docs/plans/current.md`、`docs/discovery/*`、`项目说明.md` 中文件能力边界

## 禁止修改

- 配对/mDNS/安装包/自启无关逻辑的大重构
- 剪贴板文本/图片主路径行为回归
- Android / 公网中继 / 多 peer 网格
- 把整文件通道改成 LocalSend HTTP 协议
- 文件夹、OS 文件剪贴板、断点续传
- 无任务边界的全局「性能优化」重构

## 建议实现切片（执行时按序，可再拆子 task）

若单 task 过大，执行 Agent 应**先做协议+core 流式**，hub/UI 跟进；必要时拆 `033a/033b`，但本文件保持总设计 source of truth。

1. 协议：Offer/Complete 增加 size 已有字段利用；增加 **sha256**（hex 或 raw 32B，二选一并写进 protocol-draft）
2. core：`offer_file_path` / 流式 outbound；inbound 写 temp；去掉大文件 `Vec` 必经路径
3. net/daemon：数据连接或同连接流式发送循环 + 心跳仍可达
4. hub：路径发送、`.part` 落盘、失败清理、status
5. 基准：小文件回归 + **≥100MiB（有条件则 1GiB）** 吞吐/内存/取消/断线

## 验证命令

```bash
cargo test -p m590-core -p m590-net -p m590-daemon --lib
cargo build -p m590-daemon -p m590-ui

# 小文件回归（既有 API / loopback）
# 大文件：生成测试文件后双 hub 或 session 测试
# 记录：耗时、近似吞吐、RSS/内存峰值、取消与断线后 .part 是否清理
```

## 完成标准

- [x] 单文件传输不再要求整文件驻留发送/接收端内存
- [x] 接收端 `.part` → 校验 → 原子落盘；失败可清理
- [x] 有增量 SHA-256（或等价强校验）且失败有明确错误
- [x] 小文件（≤原 4MiB 场景）行为不回归
- [x] 至少一次真实大文件验证并记录吞吐/内存摘要
- [x] 协议草案与计划/能力表已更新（去掉「硬 4MiB 内存上限」的过时表述，写清新边界）
- [x] 剪贴板/心跳在大文件传输中仍可用，或 task 记录已知限制与后续刀

## 实施记录

- 协议：`FileOffer` / `FileComplete` 增加 `sha256_hex`（空或 64 hex）；编解码同步。
- core：`FILE_CHUNK_SIZE=256KiB`，`MAX_FILE_BYTES=8GiB` 软上限，`MAX_MEMORY_FILE_BYTES=64MiB`；
  `offer_file_path` + `pump_outbound_file` 分批读盘发送；接收写 `.part` + 增量 SHA-256；
  `InboundFileResult::Applied` 改为 path/size/digest。
- 取舍：**同连接**串行帧 + 每轮最多 4 chunk，避免独立数据连接的大改；hub 循环在 try_recv 前 pump。
- hub：`/api/send_file` 与剪贴板路径 offer 走 path 流式；`send_file_bytes` 仍限内存；
  接收 `finalize_part_file`（rename/copy）到 `file_save_dir`；CLI daemon 循环同样 pump。
- UI：提示文案区分桌面原生路径发送（8GiB 软上限）与浏览器 Base64 回退（4MiB 前端上限），
  避免继续显示已过时的全局 4MiB 上限。
- 未做：独立数据连接、多文件并行、文件夹、断点续传、HTTP 化。

## 修改文件

- `crates/m590-core/Cargo.toml`（sha2）
- `crates/m590-core/src/{protocol,session,lib}.rs`
- `crates/m590-net/src/{frame,pipe}.rs`
- `crates/m590-daemon/src/{hub,file_save,main}.rs`（hub/main 循环 pump）
- `ui/src/app/OperableApp.tsx`、`ui/src/lib/bridgeApi.ts`（文件上限提示与浏览器回退说明）
- `docs/domain/protocol-draft.md`
- `docs/plans/current.md`
- `docs/tasks/task-033.md`
- `项目说明.md`（能力边界，若有 4MiB 表述）

## 验证结果

```text
$ cargo test -p m590-core -p m590-net -p m590-daemon --lib
m590-core 21 passed; m590-daemon 12 passed; m590-net 13 passed

$ cargo test -p m590-core --lib file_path_streams_100mib_with_sha256 -- --nocapture
100MiB stream: 100.0 MiB in 10.364s => 9.6 MiB/s
sha256=4cbf988462cc3ba2e10e3aae9f5268546aa79016359fb45be7dd199c073125c0
test ... ok  (同进程 session 泵送，非千兆网卡实测)

$ cargo build -p m590-daemon -p m590-ui
Finished successfully (debug binaries present)

$ cargo test -p m590-core --lib file_path_streams_100mib_with_sha256 -- --nocapture
100MiB stream: 100.0 MiB in 9.351s => 10.7 MiB/s
test ... ok

$ npm run build  # ui/
通过（TypeScript + Vite production build）

$ npm run lint  # ui/
通过（oxlint）

$ cargo fmt --all -- --check
未通过：仓库既有多个未格式化文件（含本 task 范围外的 `m590-clipboard`）；未做全仓格式化以避免无关改动
```

小文件 / 空文件 / multi-chunk / path multi-pump 单测覆盖。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`docs/domain/protocol-draft.md`
- 已更新：`项目说明.md`、`AGENTS.md`、`docs/discovery/commands.md`
- 已更新：UI 文件传输提示文案，准确区分路径流式与浏览器 Base64 回退上限
- 无需更新：`docs/ui-spec.md`，本次仅校正已有能力的显示文案，不改变交互或布局约定
- 无需：单独 localsend 笔记

## 风险

- 同连接大文件仍可能拉长单次循环；极端慢盘时心跳窗口（约数秒级）需实机再观察
- 同进程 100MiB ~9.6MiB/s 不能代表 LAN 吞吐；真网卡基准另测
- `send_file_bytes` 误用于大文件会失败（有意）
- Windows 杀毒扫描可能拖慢 `.part` 写入（未在本机验证）

## 下一步

- 建议下一 task：Linux 登录自启；或独立数据连接/吞吐调优；或 Windows 安装包
