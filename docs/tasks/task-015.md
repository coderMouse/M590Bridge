# task-015 · 复制本地图片文件时按图片同步

## 状态

`completed`

## 目标

修复「文件管理器复制 `.png/.jpg` 只同步路径文本、对端无法粘贴图片」：若剪贴板文本是本地图片路径/`file://` URI，则读文件解码为 RGBA 并走 `ClipboardImage`。

## 完成标准

- [x] 文本为本地 png 路径时可得到 ImageClipboard
- [x] hub/daemon 对该类变更发 image 而非裸路径
- [x] 单元测试通过（含用户截图路径可选测）

## 修改文件

- `crates/m590-clipboard/src/image_file.rs`（新）
- `crates/m590-clipboard/src/lib.rs`、`Cargo.toml`（`image`）
- `crates/m590-daemon/src/hub.rs`、`main.rs`
- docs：本 task、plan

## 验证结果

- `cargo test -p m590-clipboard -p m590-daemon` 通过
- 本机截图 `514x1194` 可解码（可选测试）

## 风险

- 仅识别「文本里是本地图片路径」；纯二进制位图仍靠 arboard `get_image`
- 超大图仍受 12MiB RGBA 限制
- 多文件复制只取第一个可识别图片路径

## 下一步

- 用户双机复测：文件管理器复制 png → 对端粘贴图片
- 后续：通用文件传输
