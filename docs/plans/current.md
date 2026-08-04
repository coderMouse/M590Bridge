# 当前计划 · M590Bridge

> 更新：2026-08-04  
> 阶段：文本同步 MVP + 桌面壳 + 配置持久化/自动重连

## 目标（近期）

Linux + Windows 文本剪贴板同步 MVP；托盘常驻；参数可记住、断线可重连。

## 已完成

- [x] task-001..009 核心、UI、Tauri 壳
- [x] **跨机实机**：Linux hub/UI ↔ Windows `connect`，`sync_rx` + `clipboard_write=ok`
- [x] **task-010** 配置持久化 + 断线自动重连（`GET/POST /api/config`）
- [x] **task-011** 修复 listen 间歇 code required（完整读 HTTP body）
- [x] **task-012** 设置页正式配置项（去 JSON）

## 进行中

- [ ] 无

## 下一步（有序）

1. Windows 上构建/运行 `m590-ui`（托盘 + 内嵌 hub）
2. 图片/文件通道；mDNS 发现
3. 安装包 / 开机自启

## 非目标（本周期）

- Android、公网中继、完整加密定稿

## 风险

- 托盘在部分 Linux 桌面表现不一致
- 自动重连在错误参数下会周期性重试（可在设置关闭）

## 用户怎么用

```bash
# Linux 桌面（推荐）
cargo run -p m590-ui
# 或
cd ui && npm run desktop:dev
```

主面板可创建配对 / 加入；设置里开关「自动同步」「断线自动重连」。  
配置默认写入本机 config（`M590_CONFIG` 可覆盖）。

Windows 对端仍可用：

```bat
cargo run -p m590-daemon -- connect --code <CODE> --addr <LINUX_IP>:5901 --device-id win-joiner
```
