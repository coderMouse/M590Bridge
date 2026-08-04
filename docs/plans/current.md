# 当前计划 · M590Bridge

> 更新：2026-08-04  
> 阶段：V2 — 图片剪贴板已落地；文件通道待做

## 目标（近期）

Linux + Windows 文本剪贴板同步 MVP；托盘常驻；参数可记住、断线可重连。

## 已完成

- [x] task-001..009 核心、UI、Tauri 壳
- [x] **跨机实机（CLI）**：Linux hub/UI ↔ Windows `connect`，`sync_rx` + `clipboard_write=ok`
- [x] **task-010** 配置持久化 + 断线自动重连（`GET/POST /api/config`）
- [x] **task-011** 修复 listen 间歇 code required（完整读 HTTP body）
- [x] **task-012** 设置页正式配置项（去 JSON）
- [x] **task-013** Windows 构建/运行 `m590-ui` 并联调通过（用户实机确认）
- [x] **task-014** 图片剪贴板同步（小图内联 RGBA）

## 进行中

- [ ] 无

## 下一步（有序）

1. 文件元数据 + 按需传输
2. 大图压缩/分片（可选，提升截图成功率）
3. mDNS 发现
4. 安装包 / 开机自启

## 非目标（本周期）

- Android、公网中继、完整加密定稿

## 风险

- 托盘在部分 Linux 桌面表现不一致
- 自动重连在错误参数下会周期性重试（可在设置关闭）
- 安装包/签名/开机自启尚未做

## 用户怎么用

```bash
# Linux / Windows 桌面（推荐）
cargo run -p m590-ui
# 或
cd ui && npm run desktop:dev
```

主面板可创建配对 / 加入；设置里开关「自动同步」「断线自动重连」。  
配置默认写入本机 config（`M590_CONFIG` 可覆盖）。

仍可用 CLI 对端：

```bat
cargo run -p m590-daemon -- connect --code <CODE> --addr <PEER_IP>:5901 --device-id win-joiner
```
