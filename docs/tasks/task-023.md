# task-023 · 修复：Windows 不识文件帧 + 选文件入口

## 状态

`completed`

## 问题

1. Linux 发文件 → Windows `unknown message type 11` 并反复重连（旧端无 FileOffer 解码）
2. Windows 主面板选文件入口不明显/旧构建无 UI

## 修改

- `FrameError::UnknownMessageType` 文案含 protocol mismatch / upgrade
- hub：协议不兼容时 **停止自动重连**，`last_error` 中文说明需两端同版本升级
- UI：显式「选择并发送文件」按钮 + 隐藏 file input（Win WebView 更可见）
- 文件卡片提示 type 11 需升级对端

## 验证

```text
cargo test -p m590-net --lib -- --skip tcp::
cargo test -p m590-daemon --lib
cd ui && npm run build
```

## 用户操作（必做）

Windows 与 Linux **两端**执行：

```bash
git pull
cargo build -p m590-ui
cargo run -p m590-ui
```

旧 Windows 二进制无法识别文件帧，也没有选文件 UI。

## 文档影响

- 已更新：本 task、plans/current 风险提示
