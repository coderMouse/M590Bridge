# task-028 · 桌面图标复制文件名解析 + 托盘恢复关闭可点

## 状态

`done`

## 问题

1. 文件管理器打开桌面目录复制 txt 成功；直接在桌面图标上复制无反应  
2. 关到托盘后再打开，右上角关闭按钮仍难点

## 根因与修复

1. **桌面图标复制**常只产生裸文件名文本（`12.txt`），原先只认绝对路径/`file://`  
   → `regular_file_from_text` / `first_regular_file` 在桌面目录（含 `user-dirs.dirs`）解析裸名  
2. **关闭用 hide()** 在 GNOME Wayland 上易导致标题栏按钮不接收点击  
   → 改为 `skip_taskbar` + `minimize`；恢复时 `present()` + focus

## 验证

```bash
cargo test -p m590-clipboard
cargo build -p m590-ui
```

通过。

## 用户复测

1. 直接在桌面选中 txt，Ctrl+C，看对端是否收文件  
2. 点关闭（进托盘）→ 托盘「打开主面板」→ 再点右上角关闭，应一次可点
