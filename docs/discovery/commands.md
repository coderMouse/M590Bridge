# 常用命令 · M590Bridge

> 更新日期：2026-08-05（task-029）

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
- 文件：`FileOffer/Request/Chunk/Complete` + hub 落盘 + UI 发送/进度（≤4MiB）  
- **mDNS**：host `listen` 广播 `_m590bridge._tcp.local.`；`GET /api/discover` 列表；UI joiner 点选  

## 文件 API（task-021+）

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
curl -s http://127.0.0.1:5910/api/status
```

## 发现 API（task-029 / task-031）

```bash
# 当前缓存的对端列表（已按 device_id / addr 去重）
curl -s http://127.0.0.1:5910/api/discover
# 清空并重新 browse
curl -s -X POST http://127.0.0.1:5910/api/discover/refresh
# 示例：
# {"service_type":"_m590bridge._tcp.local.","advertising":false,
#  "peers":[{"name":"...","device_id":"...","addr":"192.168.x.x:5901",...}]}
```

TXT 仅 `id` / `ver`，**不含**配对码。配对仍需手动输入相同 code。  
UI 加入页「局域网设备」旁有刷新图标。

## 配置

- `M590_CONFIG` 覆盖配置路径  
- 默认：Linux `~/.config/m590bridge/config.cfg`；Windows `%APPDATA%\M590Bridge\config.cfg`  
- `GET/POST /api/config`；status 含 `last_sync_text` / `last_error` / `auto_reconnect`

## 未做

- 文件夹 / >4MiB  
- 安装包 / 开机自启  
- 设置页「发现方式」开关  
- （已取消）019A  

## 文档

```text
docs/plans/current.md
docs/domain/protocol-draft.md
docs/tasks/task-029.md
```
