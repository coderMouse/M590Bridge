# 常用命令 · M590Bridge

> 更新日期：2026-08-04

## Rust

```bash
cargo build
cargo test
cargo run -p m590-daemon
cargo run -p m590-daemon -- --clipboard-demo
cargo run -p m590-daemon -- --help
cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
# 配置（可选 M590_CONFIG=/path/to.cfg）
# GET/POST http://127.0.0.1:5910/api/config

# UI
cd ui && npm run dev    # 可操作壳，默认 API 5910
cd ui && npm run build

# 双端文本同步（本机示例）
cargo run -p m590-daemon -- listen --code 123456 --port 5901
cargo run -p m590-daemon -- connect --code 123456 --addr 127.0.0.1:5901

# 自动化单次推送/期望（测试）
cargo run -p m590-daemon -- listen --code 123456 --port 19591 \
  --expect-text 'hello' --timeout-secs 15
cargo run -p m590-daemon -- connect --code 123456 --addr 127.0.0.1:19591 \
  --push-text 'hello' --timeout-secs 15
```

已验证：

- Linux/Windows 剪贴板 demo
- `cargo test` 含 TCP loopback 配对+文本
- 本机双进程：`push_text` → 对端 `sync_rx` / `expect_text=ok`；OS 路径 `clipboard_write=ok`

## 跨机

> 产品决定：完整实机联调可等有界面后再做；以下 CLI 步骤供需要时使用。


1. 防火墙放行 TCP 端口（默认草案 5901，可改 `--port`）
2. A：`listen --code CODE --port PORT`
3. B：`connect --code CODE --addr A_IP:PORT`
4. 任一侧复制文本（轮询）或使用 `--push-text` 测试

## 文档

```text
docs/plans/current.md
docs/tasks/task-010.md
docs/domain/protocol-draft.md
```


## 桌面壳（Tauri）

```bash
cargo build -p m590-ui
cargo run -p m590-ui
cd ui && npm run desktop:dev
cd ui && npm run desktop:build
```

内嵌 hub：`http://127.0.0.1:5910`（一般无需再手动 `hub`）。


## 配置

- 环境变量 `M590_CONFIG` 可覆盖配置文件路径
- 默认：Linux `~/.config/m590bridge/config.cfg`；Windows `%APPDATA%\\M590Bridge\\config.cfg`
- Hub：`GET/POST /api/config`；status 含 `auto_reconnect` / `reconnect_attempt`
