# 项目结构图 · M590Bridge

> 更新日期：2026-08-10
> 状态：文本+图片+流式文件+mDNS+桌面壳发布硬化+Linux 用户登录自启（task-020..038）

```text
crates/
  m590-core/       # 协议 v2、Message File*、Session 流式收发/SHA-256
  m590-clipboard/  # 文本/图片/file_list
  m590-net/        # 帧 1..14、版本拒绝、TCP
  m590-daemon/     # hub：临时令牌/CORS、send_file、无覆盖落盘、status、mDNS
ui/
  src/             # React：OperableApp 配对/发现/文件发送/登录自启；bridgeApi 自动鉴权
  src-tauri/       # Tauri 2 m590-ui；托盘与 XDG autostart commands
    permissions/
      autostart.toml       # Linux 用户登录自启读写
      hub-auth-token.toml  # 主窗口读取进程临时 Hub 令牌
docs/
  plans/current.md
  tasks/           # ..038
  domain/          # 协议草案
```
