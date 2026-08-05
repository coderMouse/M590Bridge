# 常用命令 · M590Bridge

> 更新日期：2026-08-05（task-022）

## 桌面（推荐）

```bash
cargo build -p m590-ui
cargo run -p m590-ui
cd ui && npm run desktop:dev    # 前端热更 + tauri
cd ui && npm run build          # 仅前端
```

内嵌 hub：`http://127.0.0.1:5910`。

## Rust 测试 / CLI

```bash
cargo test
cargo test -p m590-core -p m590-net -p m590-clipboard -p m590-daemon
cargo run -p m590-daemon -- --help
cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
cargo run -p m590-daemon -- listen --code 123456 --port 5901
cargo run -p m590-daemon -- connect --code 123456 --addr 127.0.0.1:5901
```

剪贴板探测（调试）：

```bash
cargo run -p m590-clipboard --example read_once
cargo run -p m590-clipboard --example probe_clipboard
```

## 已验证能力

- 文本：Linux ↔ Windows 双向  
- 图片位图：Linux ↔ Windows 双向（线载优先 PNG；Word 等可粘贴）  
- 复制图片**文件**：可提升为图片同步（非传原文件字节流）  
- 发大图：TCP 写满帧，避免 EAGAIN 误判断线  
- 文件：`FileOffer/Request/Chunk/Complete` 帧 + Session 小文件 memory loopback（task-020）  

## 文件 API（task-021）

```bash
curl -s -X POST http://127.0.0.1:5910/api/config \
  -H 'content-type: application/json' \
  -d '{"file_save_dir":"/path/to/inbox"}'
curl -s -X POST http://127.0.0.1:5910/api/send_file \
  -H 'content-type: application/json' \
  -d '{"path":"/path/to/file.bin"}'
curl -s -X POST http://127.0.0.1:5910/api/send_file_bytes \
  -H 'content-type: application/json' \
  -d '{"name":"a.txt","data_base64":"aGVsbG8="}'
curl -s http://127.0.0.1:5910/api/status   # file_transfer_phase / last_file_* / file_bytes_*
```

UI：`m590-ui` 主面板「选择并发送文件」；设置页可改 `file_save_dir`。

**重要**：Linux/Windows **必须同一构建**。若一端报 `unknown message type 11`，对端仍是旧版——两端 `git pull && cargo build -p m590-ui` 后重开。

文件管理器复制：图片仍走位图同步；**非图片**单文件 ≤4MiB 自动 FileOffer（对端落盘到 `file_save_dir`）。

默认保存目录：平台 data 目录下 `m590bridge/inbox`。单文件 ≤ 4MiB。

## 未做

- file_list → 原文件 offer  
- 文件夹 / >4MiB  
- mDNS、安装包  
- （已取消）019A  

## 配置

- `M590_CONFIG` 覆盖配置路径  
- 默认：Linux `~/.config/m590bridge/config.cfg`；Windows `%APPDATA%\M590Bridge\config.cfg`  
- `GET/POST /api/config`；status 含 `last_sync_text` / `last_error` / `auto_reconnect`

## 文档

```text
docs/plans/current.md
docs/plans/current.md
docs/domain/protocol-draft.md
```
