# 常用命令 · M590Bridge

> 更新日期：2026-08-18（task-058 增加 Linux FUSE tree 本地与 Nautilus 验收入口）

## 桌面（推荐）

```bash
cd ui && npm run desktop:standalone # 日常桌面：release + 内嵌 UI/Hub，不需要浏览器
cd ui && npm run desktop:dev        # 开发：Vite 热更 + Tauri，仅开发会话使用
cd ui && npm run build              # 仅前端
```

内嵌 hub：`http://127.0.0.1:5910`。Tauri WebView 自动取得进程临时令牌，无需手工配置。
`desktop:dev` 与 `cargo run -p m590-ui` 会加载 `127.0.0.1:5173`，不可作为登录自启目标；
`desktop:standalone` 使用内嵌前端资源，适合不使用 Web 端的源码运行方式。
重新构建测试前应先从托盘菜单退出所有旧实例；关闭 Windows 主窗口只会最小化。启动脚本
会预检 `127.0.0.1:5910`，旧 Hub 仍占用时直接报错，不再启动一个连接不到新 Hub 的窗口。
task-057 排障期间，该命令还会临时启用 `task-057-diagnostics`；Windows 从源码运行时保留
控制台并输出 `[task-057]` OLE/批次/速率行。NSIS 与普通 `desktop:build` 不启用该 feature，
仍为无控制台正式构建。
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

### Windows Explorer 多文件/目录真机验收（task-057）

在接入网络前，可先用本机 OLE 集合探针确认 Explorer 能识别两个顶层文件、嵌套文件、
嵌套空目录和顶层空目录；终端的 `content_opened` 应只在粘贴时出现：

```powershell
cargo run -p m590-clipboard --example windows_virtual_file_collection
```

探针成功后再进行下面的 Linux↔Windows 网络批次验收。

先在 Windows 构建并运行当前代码，发送端运行同一提交。配对后可直接在文件管理器复制
多个顶层文件或一个目录；也可通过“选择文件”“选择文件夹”发送。测试批次应包含嵌套目录、
空目录、空文件和大文件：

1. Windows 接收端未在 Explorer 粘贴前，不应向网络请求文件内容，接收目录也不应出现
   `.partial` 批次树。
2. 在 Explorer 目标目录按 `Ctrl+V`；应显示系统复制进度，最终相对路径、文件大小和内容
   与发送端一致，多个文件流按 entry 顺序串行完成。
3. 分别在复制中取消、复制中替换 Windows 本机剪贴板、传输中断开连接；Explorer 不应
   永久阻塞，Hub 不应保留活动批次或临时文件。
4. 重新发送同一批输入后再次粘贴，并回归一个普通单文件的按需粘贴。

当前 standalone 会在 Windows 终端输出 `[task-057][ole]` 与 `[task-057][hub]`。先用
2 个小文件、1 个嵌套文件和 1 个空目录复现一次，再用同一大文件做单文件速度对照；保留
从 `batch_received` / `publish_collection` 到 `network_stream_completed` 的所有
`[task-057]` 行。日志中的 `effective_mib_s` 包含请求到完成的等待，`data_mib_s` 从首块到
完成，可据此区分网络吞吐下降与文件间调度等待。
发送端终端还应先出现 `clipboard_file_list_detected ... action=batch`、
`clipboard_batch_queued` 和 `batch_offer_sent`；接收端应出现 `batch_received entries=`，
若仍是 `single_ole_stream_request`，说明发送端剪贴板后端只暴露了一个根路径。

同一剪贴板 offer 完成后直接再次 `Ctrl+V` 仍受一次性 offer 限制，不属于本 task 的重复
发送验收；需要支持时应另建生命周期任务。

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

这些自动化命令不能代替 GNOME Wayland + Nautilus 挂载验收。真机测试时两端运行
`cd ui && npm run desktop:standalone`；Windows 复制单个文件后 Linux 只应先显示文件
URI，Nautilus `Ctrl+V` 后才发起网络请求并显示系统进度，完成后检查内容一致。还需测试系统
取消、粘贴前/传输中替换剪贴板、断线和同一文件再次复制。

### Linux FUSE 虚拟目录树（task-058，真机待验收）

本地自动化与显式 FUSE 挂载 smoke：

```bash
cargo test -p m590-daemon virtual_file
cargo test -p m590-daemon linux_virtual
cargo test -p m590-daemon mounted_tree_smoke_browses_and_reads_nested_content -- --ignored --nocapture
cargo test -p m590-daemon mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure -- --ignored --nocapture
cargo check --workspace
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
```

第三、四条命令需要可用的 `/dev/fuse`。前者浏览临时只读 tree 的嵌套/空目录；后者按
256 KiB 网络块分别读取并逐字节校验 24 MiB 单文件和 tree 文件。它们不连接跨机网络，也
不代替 Nautilus。当前两项本地 smoke 均已通过。

Linux GNOME Wayland + Nautilus 与同一局域网 Windows 真机验收：

1. 两端退出旧托盘实例后运行同一提交的 `cd ui && npm run desktop:standalone` 并配对。
2. 先由 Windows 复制一个几十 MiB 普通文件到 Linux，确认完整粘贴，并在另一次传输中点击
   断开，确认 Hub 立即恢复可操作。随后发送一个批次，固定包含两个顶层文件、一个嵌套目录、
   空目录、空文件和一个可观察进度的大文件。Linux 收到 offer 后、Nautilus 粘贴前不应下载
   文件内容，也不应在接收目录创建 `.partial` 批次树。
3. Linux 在 Nautilus 目标目录按 `Ctrl+V`。应显示系统复制进度；最终所有顶层项、相对路径、
   空目录、文件大小和哈希均与 Windows 一致，网络文件请求保持串行。
4. 分别复测 Nautilus 取消、传输中替换 Linux 本机剪贴板、传输中断开连接。Hub 不应残留
   活动批次，临时 FUSE 挂载应清理，Nautilus 不应永久阻塞。
5. 重新发送同一批输入后再次粘贴，并回归一个普通单文件。现有网络 reader 是一次性 offer；
   同一 clipboard offer 完成后直接第二次 `Ctrl+V` 不属于已保证能力，若产品要求任意次重开
   需另建协议生命周期任务。

task-058 首轮跨机发现几十 MiB 单文件在接收数百 KiB 后卡住且 Hub 无法及时断开；Linux
接收路径已改为非阻塞背压并通过大文件本地挂载校验，但尚不能宣称 Linux Nautilus 单文件
回归或多文件/目录跨机粘贴已通过。

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
- 文件：单文件 `FileOffer/Request/Chunk/Complete` + 路径流式 + SHA-256；task-056 已接入
  `FileBatchOffer`、安全目录扫描、按 manifest 串行请求、整批暂存/发布与整体/当前条目进度
  （批次总上限 8GiB；`send_file_bytes` 仍限内存）
- Linux FUSE：单文件网络惰性读取、Nautilus 系统进度、内容和取消已跨机真机通过
  （task-052）；tree 已通过自动化与本地真实挂载，跨机 Nautilus 待验收（task-058）
- Linux 托盘：AppIndicator 菜单挂接后刷新标签，GNOME/Wayland“打开主面板 / 退出”真机通过（task-053）
- **mDNS**：host `listen` 广播 `_m590bridge._tcp.local.`；`GET /api/discover` 列表；UI joiner 点选  
- **Linux 安装包**：Tauri `.deb`，含可执行文件、桌面入口、图标和运行时依赖（task-032）
- **Linux 用户登录自启**：设置页显式启停，写当前用户 XDG autostart；正式/standalone 桌面端可用，开发壳拒绝开启（task-038/039）
- **Windows NSIS/登录自启**：当前用户 NSIS、HKCU Run、卸载清理和 Windows↔Linux 回归均已真机验收通过（task-042）
- **Windows OLE 虚拟文件**：单文件与多文件/目录集合、按 entry 串行延迟 `IStream` 和
  批次清理均已完成 Windows 10 Explorer 真机验收（task-057）

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
curl -s -X POST http://127.0.0.1:5910/api/send_batch \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"paths":["/path/to/folder","/path/to/other.bin"]}'
curl -s -X POST http://127.0.0.1:5910/api/cancel_batch \
  -H "X-M590-Token: $M590_HUB_TOKEN" \
  -H 'content-type: application/json' \
  -d '{}'
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

- Linux FUSE 虚拟目录树的 Nautilus 多文件/文件夹跨机真机验收（task-058 代码与本地
  挂载 smoke 已通过）
- 断点续传 / 多文件并行 / 独立数据连接
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
