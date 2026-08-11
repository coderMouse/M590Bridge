# M590Bridge UI / Desktop

## 四种用法

### 1. 独立桌面端（推荐日常使用）

```bash
cd ui
npm install
npm run desktop:standalone
```

该命令先构建前端，再以 release + `custom-protocol` 运行 Tauri；UI 和 Hub 都内嵌，
不需要浏览器或 `127.0.0.1:5173`。Linux 登录自启写 XDG autostart；Windows 登录
自启写当前用户 HKCU Run。两者都指向当前运行的 release/安装版程序。

### 2. 桌面开发壳

```bash
cd ui
npm run desktop:dev
```

- 开发壳使用 Vite 热更新并依赖 `127.0.0.1:5173`，不能作为登录自启目标。
- `cargo run -p m590-ui` 同样是 Tauri 开发模式，不用于日常启动或登录自启。
- 关闭窗口 → 隐藏到托盘（「打开主面板」/「退出」）
- 自动启动 hub：`http://127.0.0.1:5910`

### 3. 浏览器可操作壳

```bash
cargo run -p m590-daemon -- hub
cd ui && npm run dev
```

### 4. 设计画廊

可操作壳 → 设置 → 打开设计画廊。

## 脚本

| 命令 | 作用 |
|------|------|
| `npm run dev` | 仅 Vite |
| `npm run build` | 仅前端 dist |
| `npm run desktop:dev` | Tauri 开发 |
| `npm run desktop:standalone` | 构建并运行不依赖开发服务器的 release 桌面端 |
| `npm run desktop:build` | Tauri 构建 |
| `npm run desktop:build:windows` | 在 Windows 上构建当前用户 NSIS `.exe` |

## Ubuntu / Debian 安装包

构建机需安装 Tauri Linux 依赖；托盘打包额外依赖
`libayatana-appindicator3-dev` 提供的 `pkg-config` 元数据：

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev wget
```

从仓库根目录构建 `.deb`：

```bash
cd ui
npm ci
npm run desktop:build -- --bundles deb
cd ..
```

产物位于仓库根目录的
`target/release/bundle/deb/M590Bridge_<version>_amd64.deb`。可先检查，再安装或卸载：

```bash
dpkg-deb --info target/release/bundle/deb/M590Bridge_*_amd64.deb
sudo apt install ./target/release/bundle/deb/M590Bridge_*_amd64.deb
sudo apt remove m590-bridge
```

`target/` 是忽略的本机构建产物，不提交仓库。当前安装包未签名；包本身不预设开机
自启，安装后可在设置页为当前用户开启。

## Windows 10 NSIS 安装包

构建机需要 Node.js 22 LTS、Rust stable MSVC、Visual Studio Build Tools 2022
（Desktop development with C++ + Windows SDK）。在 Windows PowerShell 执行：

```powershell
cd ui
npm ci
npm run build
cargo test -p m590-ui --lib
npm run desktop:build:windows
Get-ChildItem ..\target\release\bundle\nsis\*.exe
```

安装包采用当前用户模式，不要求管理员权限。设置页「登录时自动启动」写入 HKCU Run；
关闭开关或卸载会删除 `M590Bridge` 值。安装包当前未签名，SmartScreen 可能提示未知
发布者。完整打包、安装、注销登录和卸载流程见 `docs/tasks/task-042.md`。

## 设计来源

- Figma Make / `docs/ui-spec.md`
