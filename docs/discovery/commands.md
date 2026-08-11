# 常用命令 · M590Bridge

> 更新日期：2026-08-10（task-039）

## 桌面（推荐）

```bash
cd ui && npm run desktop:standalone # 日常桌面：release + 内嵌 UI/Hub，不需要浏览器
cd ui && npm run desktop:dev        # 开发：Vite 热更 + Tauri，仅开发会话使用
cd ui && npm run build              # 仅前端
```

内嵌 hub：`http://127.0.0.1:5910`。Tauri WebView 自动取得进程临时令牌，无需手工配置。
`desktop:dev` 与 `cargo run -p m590-ui` 会加载 `127.0.0.1:5173`，不可作为登录自启目标；
`desktop:standalone` 使用内嵌前端资源，适合不使用 Web 端的源码运行方式。

仅运行浏览器开发服务器并连接独立 Hub 时，两边需使用同一个临时令牌；不要放进 URL、共享文档或提交文件：

```bash
# 以实际的至少 32 字符临时值替换 [REDACTED]
export M590_HUB_TOKEN='[REDACTED]'
cargo run -p m590-daemon -- hub --api 127.0.0.1:5910

cd ui
VITE_M590_HUB_TOKEN="$M590_HUB_TOKEN" npm run dev
```

### Linux `.deb`（task-032）

Ubuntu 构建依赖：

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev wget
```

构建、检查与安装（从仓库根目录执行）：

```bash
cd ui
npm ci
npm run desktop:build -- --bundles deb
cd ..
dpkg-deb --info target/release/bundle/deb/M590Bridge_*_amd64.deb
dpkg-deb --contents target/release/bundle/deb/M590Bridge_*_amd64.deb
sudo apt install ./target/release/bundle/deb/M590Bridge_*_amd64.deb
sudo apt remove m590-bridge
```

产物在 `target/release/bundle/deb/`，不提交仓库。当前仅验证 `amd64`、未签名；包本身不写系统级自启入口。

### Linux 用户登录自启（task-038 / task-039）

- 设置页「启动」→「登录时自动启动」会为当前用户创建 XDG autostart 入口。
- 默认入口：`~/.config/autostart/M590Bridge.desktop`；设置了绝对路径 `XDG_CONFIG_HOME` 时改用 `$XDG_CONFIG_HOME/autostart/M590Bridge.desktop`。
- 开启不需要 root；`Exec` 指向当前运行的 `m590-ui`，安装包运行时通常为 `/usr/bin/m590-ui`。
- 只能从 `.deb` 安装版或 `npm run desktop:standalone` 启动的正式桌面端开启；开发壳会明确拒绝，避免登录后因 Vite 未运行而连接 `127.0.0.1:5173` 失败。
- 若旧入口已指向 `target/debug/m590-ui`，先关闭开关（或删除该入口），再从正式/standalone 桌面端重新开启。
- 关闭开关会删除入口。`apt remove` 不会也不应遍历各用户主目录；卸载前先关闭开关，或卸载后手工删除上述文件。

## Rust 测试 / CLI

```bash
cargo test
cargo test -p m590-core -p m590-net -p m590-clipboard -p m590-daemon
cargo run -p m590-daemon -- --help
M590_HUB_TOKEN='[REDACTED]' cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
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
- 文件：`FileOffer/Request/Chunk/Complete` + 路径流式 + SHA-256 + hub 落盘 + UI 发送/进度（软上限 8GiB；`send_file_bytes` 仍限内存）
- **mDNS**：host `listen` 广播 `_m590bridge._tcp.local.`；`GET /api/discover` 列表；UI joiner 点选  
- **Linux 安装包**：Tauri `.deb`，含可执行文件、桌面入口、图标和运行时依赖（task-032）
- **Linux 用户登录自启**：设置页显式启停，写当前用户 XDG autostart；正式/standalone 桌面端可用，开发壳拒绝开启（task-038/039）

## 文件 API（task-021+）

```bash
# 与 Hub 启动时的 M590_HUB_TOKEN 相同；文档中不记录真实值
export M590_HUB_TOKEN='[REDACTED]'
curl -s -X POST http://127.0.0.1:5910/api/config \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"file_save_dir":"/path/to/inbox"}'
curl -s -X POST http://127.0.0.1:5910/api/send_file \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"path":"/path/to/file.bin"}'
curl -s -X POST http://127.0.0.1:5910/api/send_file_bytes \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"a.txt","data_base64":"aGVsbG8="}'
curl -s -H "X-M590-Token: $M590_HUB_TOKEN" http://127.0.0.1:5910/api/status
```

## 发现 API（task-029 / task-031）

```bash
# 当前缓存的对端列表（已按 device_id / addr 去重）
curl -s -H "X-M590-Token: $M590_HUB_TOKEN" http://127.0.0.1:5910/api/discover
# 清空并重新 browse
curl -s -X POST -H "X-M590-Token: $M590_HUB_TOKEN" \
  http://127.0.0.1:5910/api/discover/refresh
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

- 文件夹 / OS 文件剪贴板 / 断点续传 / 独立数据连接
- Windows 安装包 / Windows 开机自启
- 设置页「发现方式」开关  
- （已取消）019A  

## 文档

```text
docs/plans/current.md
docs/domain/protocol-draft.md
docs/tasks/task-035.md
```
