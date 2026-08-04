# task-016 · 文件管理器复制图片：file_list + Wayland

## 状态

`completed`

## 问题

文件管理器复制 png 时，剪贴板往往是 `text/uri-list`（文件列表），不是纯文本路径、也不是位图。  
仅读 text/image 时：主面板无变化、对端粘贴不出图。  
另：配对期间打开的剪贴板基线会吞掉「连接前已复制」的内容。

## 修改

- arboard 启用 `wayland-data-control`
- `read_file_list` / `poll_file_list_change`
- hub/daemon：文件列表中的图片 → `ClipboardImage`
- `prime_poll_to_emit_current`：Connected 后重新武装 poll

## 验证

- `cargo test -p m590-clipboard -p m590-daemon`
- 本机 example：`set_file_and_read` 可 file_list + 解码 514x1194
- 实机：两端重启新构建后，连接成功再复制 png 文件

## 风险

- 部分 GNOME 对 data-control 权限仍可能限制后台读；若仍失败，用「图片查看器复制图像」或截图到剪贴板
