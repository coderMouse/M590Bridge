# task-018 · Windows→Linux 图片：PNG 压缩 + 失败可见

## 状态

`completed`

## 问题

Linux→Windows 复制图片可用；Windows→Linux 时对端粘贴失败且主面板文本无变化。  
常见原因：Windows 截图 raw RGBA 超过 12MiB 被静默 skip；或 poll 错误被吞掉。

## 修改

- 线载图片支持 `ImageEncoding::Png`（优先）/ `RawRgba`
- `ImageClipboard::prepare_inline`：能塞进预算则发 PNG
- hub：过大/解码/poll 失败写入 `last_error`，成功摘要带 encoding
- Windows：剪贴板锁定时 reopen 重试

## 验证

- `cargo test -p m590-core -p m590-net -p m590-clipboard -p m590-daemon`
- 实机：两端同版本；Win 复制截图/图片后 Linux 面板应出现 `[image … Png]` 或明确错误

## 注意

- **两端必须同时升级**（协议多了 encoding 字节）
