# 项目结构图 · M590Bridge

> 更新日期：2026-08-05  
> 状态：文本+图片+文件+ mDNS 发现（task-020..029）

```text
crates/
  m590-core/       # Message File*、Session offer/request/chunk
  m590-clipboard/  # 文本/图片/file_list
  m590-net/        # 帧 1..14、TCP
  m590-daemon/     # hub：send_file、落盘、status、discovery(mDNS)
ui/
  src/             # React：OperableApp 配对/发现/文件发送
  src-tauri/       # Tauri 2 m590-ui
docs/
  plans/current.md
  tasks/           # ..029
  domain/          # 协议草案
```
