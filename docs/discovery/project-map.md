# 项目结构图 · M590Bridge

> 更新日期：2026-08-11
> 状态：文本+图片+流式文件+mDNS+Linux 发布+Windows NSIS/登录自启待真机验收（task-020..042）

```text
crates/
  m590-core/       # 协议 v2、Message File*、Session 流式收发/SHA-256
  m590-clipboard/  # 文本/图片/file_list
  m590-net/        # 帧 1..14、版本拒绝、TCP
  m590-daemon/     # hub：临时令牌/CORS、send_file、无覆盖落盘、status、mDNS
ui/
  src/             # React：OperableApp 配对/发现/文件发送/登录自启；bridgeApi 自动鉴权
  src-tauri/       # Tauri 2 m590-ui；托盘、Linux XDG / Windows HKCU autostart commands
    permissions/
      autostart.toml       # Linux/Windows 用户登录自启读写
      hub-auth-token.toml  # 主窗口读取进程临时 Hub 令牌
    windows/
      installer-hooks.nsh # NSIS 卸载清理当前用户 Run 值
docs/
  plans/current.md
  tasks/           # ..042
  domain/          # 协议草案
```
