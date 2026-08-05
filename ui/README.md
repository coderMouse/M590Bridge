# M590Bridge UI / Desktop

## 三种用法

### 1. 桌面壳（推荐，Tauri）

```bash
# 仓库根目录先保证依赖 OK
cd ui
npm install
npm run desktop:dev      # 开发：Vite + Tauri 窗口 + 托盘 + 内嵌 hub
npm run desktop:build    # 打包（需本机 Tauri 依赖）
```

或：

```bash
cargo run -p m590-ui
```

- 关闭窗口 → 隐藏到托盘（「打开主面板」/「退出」）
- 自动启动 hub：`http://127.0.0.1:5910`

### 2. 浏览器可操作壳

```bash
cargo run -p m590-daemon -- hub
cd ui && npm run dev
```

### 3. 设计画廊

可操作壳 → 设置 → 打开设计画廊。

## 脚本

| 命令 | 作用 |
|------|------|
| `npm run dev` | 仅 Vite |
| `npm run build` | 仅前端 dist |
| `npm run desktop:dev` | Tauri 开发 |
| `npm run desktop:build` | Tauri 构建 |

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

`target/` 是忽略的本机构建产物，不提交仓库。当前安装包未签名，也不包含开机自启。

## 设计来源

- Figma Make / `docs/ui-spec.md`
