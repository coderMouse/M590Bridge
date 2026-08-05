# task-022 · V2 文件通道：UI 发送 / 进度 / 保存目录

## 状态

`completed`

## 目标

在 task-021 hub API 上接 **可操作 UI**：

1. 主面板：选择/发送文件 + 传输进度展示
2. 设置：编辑并保存 `file_save_dir`
3. `POST /api/send_file_bytes`（name + base64，≤4MiB）供浏览器选文件

## 实施记录

- hub：`/api/send_file_bytes` + `PENDING_FILE_BYTES` + `offer_file_bytes`
- `bridgeApi`：文件 status 字段、`postSendFileBytes`、`bytesToBase64`、进度辅助函数
- `OperableApp`：主面板文件卡片；设置「文件」节；保存配置含 `file_save_dir`

## 修改文件

- `crates/m590-daemon/src/hub.rs`、`Cargo.toml`（base64）
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`
- docs：本 task、plans/current、discovery

## 验证结果

```text
cargo test -p m590-core -p m590-daemon   # passed
cd ui && npm run build                   # tsc + vite ok
```

## 文档影响

- 已更新：task-022、current、commands
- 无需更新：ui-spec 大段（行为与主面板一致，未改 Figma 画廊 mock）
- 待补：设计画廊 TransferScreen 仍为 mock

## 风险

- base64 上传放大约 33%；仍受 4MiB 原文件上限
- 进度依赖 1s 轮询 status
- 未做多文件队列 / 取消传输

## 下一步

file_list → offer；或独立 Transfer 多文件页。
