# task-014 · 图片剪贴板同步（小图内联）

## 状态

`completed`

## 目标

在现有 1 对 1 会话上支持 **图片剪贴板** 同步：协议帧、会话去重、Linux/Windows 读写、daemon/hub 同步环。  
大图超过帧上限则跳过并日志提示；**不做** 文件通道、分片、进度 UI、mDNS。

## 背景

- 文本 MVP（task-001..013）已双端可用
- 计划下一步：图片/文件通道；本 task 只落地 **图片** 首切片
- 线载：RGBA 内联（与 arboard 对齐），受 `MAX_PAYLOAD_LEN`（16 MiB）与 `INLINE_IMAGE_MAX_BYTES`（12 MiB）约束

## 允许修改

- `crates/m590-core`：`ClipboardImage` 消息与 session 队列/入站
- `crates/m590-net`：帧编解码 type=10
- `crates/m590-clipboard`：图片读/写/poll（arboard `image-data`）
- `crates/m590-daemon`：sync 环与 hub 同步路径
- `docs/domain/protocol-draft.md`、`docs/plans/current.md`、本 task、必要时 discovery

## 禁止修改

- 文件传输协议/分片
- UI 大改与安装包
- Android、公网、加密定稿
- 无关重构

## 验证命令

```bash
cargo test -p m590-core -p m590-net -p m590-clipboard -p m590-daemon
# 可选实机：两端 m590-ui 或 daemon，一侧复制截图，对端粘贴
```

## 完成标准

- [x] `Message::ClipboardImage` 编解码 roundtrip 测试通过
- [x] session 图片 content_id 去重 / 入站 AppliedImage
- [x] PlatformClipboard 图片 API（Null + Linux/Windows arboard）
- [x] daemon/hub 收到图可写剪贴板；本地图变更可发送（超限 skip）
- [x] 文档协议草案更新

## 实施记录

### 修改文件

- `crates/m590-core/src/{protocol,session,error,lib}.rs`
- `crates/m590-net/src/{frame,lib}.rs`
- `crates/m590-clipboard/src/{lib,arboard_text,linux,windows}.rs` + `Cargo.toml`（`image-data`）
- `crates/m590-daemon/src/{main,hub}.rs`
- `docs/domain/protocol-draft.md`、`docs/plans/current.md`、本 task

### 验证结果

- `cargo test -p m590-core -p m590-net -p m590-clipboard -p m590-daemon`：通过  
  （含 `roundtrip_clipboard_image`、`clipboard_image_dedup_and_apply`、`tcp::` loopback）
- `cargo build -p m590-daemon`：通过
- 跨机截图实机：待用户验证（本环境未做 GUI 截图联调）

### 文档影响

- 已更新：protocol-draft、plan、本 task
- 无需更新：ui-spec 主体（无进度 UI）
- 待补：文件通道 task；UI 显示「图片已同步」可后续

### 风险

- 大截图（如 4K raw RGBA）会超 12 MiB 被 skip，暂无压缩/分片
- Wayland/部分应用对 image clipboard 支持不一致
- hub status 用 `[image WxH NB]` 文本摘要，非结构化字段

### 下一步

- task-015 候选：文件元数据 + 按需传输；或图片 PNG 压缩以提升大图成功率
