# 常用命令 · M590Bridge

> 更新日期：2026-08-05

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

## 未做

- 桌面/文件夹「粘贴出文件」（需文件剪贴板，见 task-019）  
- 通用文件分片传输、mDNS、安装包  

## 配置

- `M590_CONFIG` 覆盖配置路径  
- 默认：Linux `~/.config/m590bridge/config.cfg`；Windows `%APPDATA%\M590Bridge\config.cfg`  
- `GET/POST /api/config`；status 含 `last_sync_text` / `last_error` / `auto_reconnect`

## 文档

```text
docs/plans/current.md
docs/tasks/task-019.md
docs/domain/protocol-draft.md
```
