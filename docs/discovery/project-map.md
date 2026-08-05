# 项目结构图 · M590Bridge

> 更新日期：2026-08-05  
> 状态：文本+图片可用；文件协议+hub+UI 发送/进度（task-020..022）

```text
crates/
  m590-core/       # Message File*、Session offer/request/chunk、inbound_file_progress
  m590-clipboard/  # 文本/图片/file_list
  m590-net/        # 帧 1..14、TCP
  m590-daemon/     # hub：send_file(_bytes)、自动落盘、file_save_dir、status
ui/
  src/             # React：OperableApp 文件发送/进度/保存目录
  src-tauri/       # Tauri 2 m590-ui
docs/
  plans/current.md
  tasks/           # ..021
  domain/          # 协议草案
```
