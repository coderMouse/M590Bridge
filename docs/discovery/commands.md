# 常用命令 · M590Bridge

> 更新日期：2026-08-17（task-054 增加 Linux / Windows 一键打包入口）

## 桌面（推荐）

```bash
cd ui && npm run desktop:standalone # 日常桌面：release + 内嵌 UI/Hub，不需要浏览器
cd ui && npm run desktop:dev        # 开发：Vite 热更 + Tauri，仅开发会话使用
cd ui && npm run build              # 仅前端
```

内嵌 hub：`http://127.0.0.1:5910`。Tauri WebView 自动取得进程临时令牌，无需手工配置。
`desktop:dev` 与 `cargo run -p m590-ui` 会加载 `127.0.0.1:5173`，不可作为登录自启目标；
`desktop:standalone` 使用内嵌前端资源，适合不使用 Web 端的源码运行方式。
Linux 上该命令还会刷新用户级隐藏 `m590-ui.desktop` 与应用图标，供 GNOME/Wayland
按 `app_id=m590-ui` 显示正确的任务栏图标；具体清理路径见 `ui/README.md`。

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

一键打包（从仓库根目录执行）：

```bash
./ui/scripts/package-linux.sh
```

脚本会检查 Linux 基础命令与 GTK/WebKitGTK/AppIndicator 开发库，执行 `npm ci` 和 Tauri
`.deb` 构建，成功后打印实际产物路径。打包不需要 root 权限；若误用 `sudo`，脚本会
切回发起用户环境，避免 `secure_path` 隐藏用户级 Node.js。检查与安装仍按需执行：

```bash
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

### Windows NSIS / 用户登录自启（task-042，真机验收通过）

Windows 构建机需要 Node.js 22 LTS、Rust stable MSVC、Visual Studio Build Tools 2022
（Desktop development with C++ + Windows SDK）。从仓库根目录执行：

```powershell
.\ui\scripts\package-windows.ps1
```

脚本会检查 Node.js、Cargo 与 Windows MSVC Rust host，执行 `npm ci` 和 Tauri NSIS 构建，
成功后打印实际 `.exe` 路径；成功或异常时均等待按键后退出。`npm run build` 已由 Tauri 的
`beforeBuildCommand` 自动执行；Rust 测试、注册表检查和跨机回归属于完整验收，不需要
每次打包手工重复输入。

- NSIS 为当前用户安装，不要求管理员权限；产物未签名，SmartScreen 可能提示未知发布者。
- 设置页开关读写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `M590Bridge` 值。
- 开启后可用 `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v M590Bridge` 检查；关闭或卸载后该值应不存在。
- 依赖 Vite 的开发壳拒绝开启自启；必须用安装版或 release/standalone。
- 用户已在 Windows 真机确认 `.exe` 生成、安装、登录自启、关闭/卸载清理和与 Linux 跨机回归通过。安装包仍未签名，SmartScreen 可能提示未知发布者。

### Windows OLE 虚拟文件原型（task-043，Explorer 真机已通过）

在 Windows 10 仓库根目录运行：

```powershell
cargo run -p m590-clipboard --example windows_virtual_file -- 268435456 8
```

- 参数依次为虚拟文件大小（字节）和每次读取延迟（毫秒）；示例为 256 MiB / 8 ms。
- 看到 `virtual_file_ready` 后，先确认尚无 `content_opened`，再到 Explorer 目标目录按 `Ctrl+V`。
- Explorer 请求 `CFSTR_FILECONTENTS` 时终端才应打印 `content_opened`；确认系统复制进度出现，最终文件大小为 268435456 字节。
- 按 Enter 退出原型。它只验证本机 OLE/Shell 按需取流，不连接 M590Bridge 网络会话，也不会先生成永久中间文件。

### Windows 按粘贴取流真机验收（task-044）

Windows 端使用安装版或 standalone 桌面端，Linux 端使用同版本桌面端。两端完成配对后：

1. A 复制单个普通文件，B 只应收到文件名/大小 offer；B 保存目录和 `.partial` 不应出现内容文件。
2. B 在 Explorer 目标目录按 `Ctrl+V`，此时才应出现 `FileRequest` 后开始网络流；Explorer 显示系统原生复制进度，目标目录只留下最终文件。
3. 粘贴过程中取消 Explorer 复制或替换 B 剪贴板，确认双方状态停止且没有残留 `.part`；已开始读取后约 30 秒无网络进展应超时取消。未粘贴的 offer 应继续保留到剪贴板被替换或会话断开。

当前 Linux 环境可运行以下协议/管道测试，但不能替代 Windows Explorer 真机验收：

```bash
cargo test -p m590-core -p m590-net -p m590-daemon
cargo check --workspace
cargo clippy -p m590-core -p m590-net -p m590-daemon --lib --no-deps -- -D warnings
```

Windows 交叉检查（若本机 Cargo 缓存和 GNU linker 可用）：

```powershell
cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples
cargo check -p m590-daemon --target x86_64-pc-windows-gnu
```

### Linux FUSE 单文件按需粘贴原型（task-051，Nautilus 真机已通过）

构建不依赖系统 `fuse3.pc`，但运行时必须有 `/dev/fuse` 且当前用户可使用：

```bash
cargo test -p m590-daemon linux_virtual_file
cargo check -p m590-daemon --examples
mkdir -p ~/M590Bridge-paste-test
cargo run -p m590-daemon --example linux_virtual_file -- 67108864 4 ~/M590Bridge-paste-test/M590Bridge-virtual.bin
```

参数依次为虚拟文件大小（字节）、每次模式数据读取延迟（毫秒）和可选的粘贴结果校验路径。
探针会创建并在退出时清理临时 FUSE 挂载目录；`virtual_file_ready` 后粘贴前不应出现
`content_opened`，Nautilus `Ctrl+V` 后才应出现 `content_opened` / `content_first_read`。
当前 GNOME Wayland 真机已验证：URI 发布后内容源保持未打开，Nautilus 粘贴时从 offset 0
开始读取并显示系统原生进度框，64 MiB 模式文件最终内容校验一致。当前机器的 `/tmp` 有独立
配额限制，粘贴测试目标应放在用户主目录。

### Linux FUSE 网络按需粘贴（task-052，真机已通过）

task-052 将 Linux 单文件 FUSE 内容源接入现有 `FileRequest` / 有界管道 / `FileCancel`，
Linux↔Windows 真机已通过；
本机可运行以下检查：

```bash
cargo test -p m590-daemon virtual_file_bridge
cargo test -p m590-daemon linux_virtual_file
cargo test -p m590-daemon linux_virtual_file_manager
cargo test -p m590-daemon
cargo check --workspace
cargo clippy -p m590-daemon --lib --examples --no-deps -- -D warnings
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples
```

当前执行环境没有 `/dev/fuse`，不能代替 GNOME Wayland + Nautilus 挂载验收。真机测试时两端
运行 `cd ui && npm run desktop:standalone`；Windows 复制单个文件后 Linux 只应先显示文件
URI，Nautilus `Ctrl+V` 后才发起网络请求并显示系统进度，完成后检查内容一致。还需测试系统
取消、粘贴前/传输中替换剪贴板、断线和同一文件再次复制。

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

GNOME Wayland 文件 URI 粘贴探针（task-050，运行前先退出 M590Bridge）：

```bash
cargo run -p m590-clipboard --example set_file_and_read -- /path/to/test-file
```

保持探针运行，在 Nautilus 另一目录按 `Ctrl+V`，完成后回到终端按 Enter 退出。
输出的 `publisher_backend` 是 `wayland-data-control`、`x11-fallback` 或 `x11`；
`readback_matches=true` 只代表发布后自读成功，不代替 Nautilus 真机粘贴。
当前 GNOME Wayland 真机已验证：`publisher_backend=x11-fallback` 时 Nautilus 可粘贴，
且源/目标文件 `cmp` 一致。task-051 已完成 FUSE 本机惰性读取真机验收；task-052
进一步完成网络按需桥接与跨机真机验收。

## 已验证能力

- 文本：Linux ↔ Windows 双向  
- 图片位图：Linux ↔ Windows 双向（线载优先 PNG；Word 等可粘贴）  
- 复制图片**文件**：可提升为图片同步（非传原文件字节流）  
- 发大图：TCP 写满帧，避免 EAGAIN 误判断线  
- 文件：单文件 `FileOffer/Request/Chunk/Complete` + 路径流式 + SHA-256 + hub 落盘 + UI 发送/进度（软上限 8GiB；`send_file_bytes` 仍限内存）；task-055 已增加 `FileBatchOffer` 清单模型与路径安全 frame 基础，但尚未接入多文件运行时
- Linux FUSE：单文件网络惰性读取、Nautilus 系统进度、内容和取消已跨机真机通过（task-052）
- Linux 托盘：AppIndicator 菜单挂接后刷新标签，GNOME/Wayland“打开主面板 / 退出”真机通过（task-053）
- **mDNS**：host `listen` 广播 `_m590bridge._tcp.local.`；`GET /api/discover` 列表；UI joiner 点选  
- **Linux 安装包**：Tauri `.deb`，含可执行文件、桌面入口、图标和运行时依赖（task-032）
- **Linux 用户登录自启**：设置页显式启停，写当前用户 XDG autostart；正式/standalone 桌面端可用，开发壳拒绝开启（task-038/039）
- **Windows NSIS/登录自启**：当前用户 NSIS、HKCU Run、卸载清理和 Windows↔Linux 回归均已真机验收通过（task-042）
- **Windows OLE 虚拟文件**：单文件 `FILEDESCRIPTORW` + 延迟 `IStream` 已接入 `FileRequest`，由网络有界管道供给；task-044 的 Windows↔Linux 端到端已真机验收通过

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

- 文件夹 / 端到端 OS 文件剪贴板 / 断点续传 / 独立数据连接
- 多文件/文件夹和断点续传仍不在当前能力范围；需另建设计任务
- 设置页「发现方式」开关  
- （已取消）019A  

## 文档

```text
docs/plans/current.md
docs/domain/protocol-draft.md
docs/tasks/task-042.md
docs/tasks/task-043.md
docs/tasks/task-054.md
```
