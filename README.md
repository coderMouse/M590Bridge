# M590Bridge

**跨机剪贴板同步工具 · 为双机切换鼠标（如罗技 M590）设计**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.4-green.svg)](Cargo.toml)

## 📋 项目简介

罗技 M590 等支持双机切换的鼠标，只能切换键鼠连接，**不能**同步剪贴板或文件。

M590Bridge 在两台电脑上运行桥接服务：**复制内容通过局域网同步，切机后可直接粘贴**。

## ✨ 功能特性

| 功能                      | 状态                                    |
| ------------------------- | --------------------------------------- |
| 📝 文本剪贴板双向同步     | ✅ 已完成                                |
| 🖼️ 图片剪贴板双向同步     | ✅ 已完成（位图 + PNG）                  |
| 📁 文件/目录传输          | ✅ 已完成（按需传输 + 进度 + 流式）      |
| 🔍 局域网自动发现（mDNS） | ✅ 已完成                                |
| 🚀 登录自启动              | ✅ 已完成（Linux / Windows）             |
| 📦 安装包                 | ✅ 已完成（Linux `.deb` / Windows NSIS） |
| 🔒 加密传输               | ⏳ 计划中                                |
| 📱 Android 支持           | ❌ 暂缓                                  |

## 🖥️ 支持平台

| 平台        | 状态         | 测试环境                         |
| ----------- | ------------ | -------------------------------- |
| **Linux**   | ✅ 已验收     | Ubuntu 26.04 LTS (GNOME Wayland) |
| **Windows** | ✅ 已验收     | Windows 10                       |
| macOS       | ❌ 未支持     | -                                |
| Android     | ❌ 已明确暂缓 | -                                |

## 🚀 快速开始

### 安装

#### Linux（Ubuntu / Debian）

```bash
# 1. 构建安装包
./ui/scripts/package-linux.sh

# 2. 安装
sudo apt install ./target/release/bundle/deb/M590Bridge_*_amd64.deb

# 3. 启动（已自动创建桌面快捷方式）
```

#### Windows 10

```powershell
# 1. 构建安装包（在 Windows 开发终端中）
.\ui\scripts\package-windows.ps1

# 2. 运行安装程序
.\target\release\bundle\nsis\M590Bridge_*_x64-setup.exe

# 3. 启动（已自动创建开始菜单快捷方式）
```

### 开发运行

```bash
# 前提条件：Rust 1.97+, Node.js 24+

# 1. 克隆仓库
git clone <repository-url>
cd M590Bridge

# 2. 安装前端依赖
cd ui && npm ci

# 3. 运行独立桌面端（内嵌 UI + Hub）
npm run desktop:standalone
```

**注意**：`desktop:dev` 与 `cargo run -p m590-ui` 仅用于开发，依赖 Vite，不能作为登录自启目标。

## 📖 使用说明

### 配对流程

1. 在两台电脑上启动 M590Bridge
2. 打开任一台的主界面，点击「发现设备」
3. 选择对方设备，输入配对码完成配对

### 自动同步

- 在设置页开启「自动同步剪贴板」（默认开启）
- 复制内容后自动推送到对端
- 切换鼠标后直接粘贴

### 文件传输

- **单文件/目录**：通过文件管理器复制 → 切机 → 粘贴（支持系统原生进度）
- **多文件批次**：支持多选 / 目录扫描 / 拖放
- **手动发送**：主界面「发送文件」按钮

## 🔧 技术栈

- **后端**：Rust（Tokio 异步运行时）
- **前端**：React + TypeScript + Vite
- **桌面框架**：Tauri 2
- **网络**：TCP + mDNS（mdns-sd）
- **文件系统**：FUSE（Linux 按需读取）/ OLE 虚拟剪贴板（Windows）
- **剪贴板**：arboard + wl-clipboard-rs（Linux）

## 📂 项目结构

```
M590Bridge/
├── crates/
│   ├── m590-core/        # 协议核心（会话、帧、消息）
│   ├── m590-clipboard/   # 剪贴板抽象（文本/图片/文件）
│   ├── m590-net/         # 网络传输（TCP、管道、流）
│   └── m590-daemon/      # 守护进程（Hub、配对、文件保存）
├── ui/                   # Tauri 桌面端（React + TypeScript）
│   ├── src/              # 前端代码
│   └── src-tauri/        # Tauri 后端（m590-ui crate）
└── docs/                 # 文档
    ├── plans/            # 计划与进展
    ├── tasks/            # 任务记录（task-001 ~ task-063）
    ├── discovery/        # 开放问题、命令
    └── domain/           # 协议草案
```

## 🔐 安全特性

- ✅ localhost Hub API 使用进程临时令牌鉴权
- ✅ 文件传输 SHA-256 完整性校验
- ✅ 路径遍历保护（transfer_id 隔离）
- ✅ 图片解码前元数据检查（防止像素炸弹）
- ✅ 预留空间与可用空间检查
- ✅ 大文件流式传输（不进内存，软上限 8GiB）

## 📝 开发指南

详细规则见：

- [`AGENTS.md`](AGENTS.md) - Agent 工作流规则
- [`CLAUDE.md`](CLAUDE.md) - Claude Code 项目规则
- [`docs/plans/current.md`](docs/plans/current.md) - 当前计划与任务进展（**必读**）

### 运行测试

```bash
cargo test --workspace --lib
```

### 代码检查

```bash
cargo clippy --workspace --all-targets -- -D warnings
cd ui && npm run lint
```

## 📄 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件

## 🤝 贡献

当前为内部项目，暂不接受外部贡献。

---

> 📌 **文档导航**  
> 中文详细说明：[`项目说明.md`](项目说明.md)  
> 任务进展：[`docs/plans/current.md`](docs/plans/current.md)  
> 协议草案：[`docs/domain/protocol-draft.md`](docs/domain/protocol-draft.md)
