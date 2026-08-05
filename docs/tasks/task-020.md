# task-020 · V2 文件通道：协议 + 会话 + 小文件 loopback

## 状态

`completed`

## 目标

落地 V2 文件通道的**第一刀**：

1. 线协议：`FileOffer` / `FileRequest` / `FileChunk` / `FileComplete`
2. `Session`：报价、按需请求、分片发送、接收重组（内存）
3. 小文件 memory pipe loopback 真实验证

**不做**：OS 文件剪贴板、落盘保存目录、hub/UI 进度、文件夹、大文件背压调优、019A 捷径。

## 背景

- 图片位图通道已完成（task-014..018）
- task-019 已 cancelled
- 计划下一主线：文件元数据 + 按需传输 + 进度

## 允许修改

- `crates/m590-core/**`（protocol / session / error / exports）
- `crates/m590-net/**`（frame 编解码 + 相关测试）
- `docs/domain/protocol-draft.md`
- `docs/plans/current.md`
- `docs/discovery/*`（若能力边界变化）
- 本 task 文件

## 禁止修改

- `ui/**`、hub 业务逻辑大改
- OS 剪贴板 file_list 语义改成「传原文件」
- 复活 task-019A
- 无关重构

## 验证命令

```bash
cargo test -p m590-core -p m590-net
```

完成标准：

- 帧编解码覆盖四种文件消息
- 双 Session + MemoryPipe：offer → request → chunk(s) → complete，接收字节与文件名一致
- 既有文本/图片测试不回归

## 实施记录

- 协议新增 msg_type 11..14 与 payload 校验（basename、非空 transfer_id、非空 chunk）
- Session：`offer_file` / `request_file`，入站重组，`InboundFileResult`，内存上限 4MiB，分片 64KiB
- frame 编解码 + `frame_roundtrip_all_message_kinds` / pipe loopback 测试
- 未改 hub/UI

## 修改文件

- `crates/m590-core/src/{error,protocol,session,lib}.rs`
- `crates/m590-net/src/{frame,lib,pipe}.rs`
- `docs/domain/protocol-draft.md`
- `docs/plans/current.md`
- `docs/discovery/{project-map,commands}.md`
- `docs/tasks/task-020.md`

## 验证结果

```text
cargo test -p m590-core -p m590-net --lib -- --skip tcp::
# m590-core: 18 passed
# m590-net: 9 passed (3 tcp filtered)

cargo test -p m590-net --lib tcp::   # 需本机 loopback 权限
# 3 passed

cargo test -p m590-daemon --lib
# 2 passed
```

关键用例：`file_offer_request_chunk_complete_small_file`、`file_empty_completes_without_chunks`、`roundtrip_file_messages`、`memory_pipe_transfers_small_file_on_demand`。

## 文档影响

- 已更新：`protocol-draft.md`、`plans/current.md`、`discovery/project-map.md`、`discovery/commands.md`、本 task
- 无需更新：`ui-spec.md`（无 UI 变更）
- 待补：hub/落盘接入后的 status 字段说明（下一 task）

## 风险

- 仅内存 staged；断线清状态，无断点续传
- 单文件、无并发多 transfer 调度优化
- hub 尚未接入，实机还不能传文件
- 旧端收到 11..14 会 `UnknownMessageType`

## 下一步

hub：接收 offer 后可配置目录落盘 + status 进度；或剪贴板 file_list 触发 offer。
