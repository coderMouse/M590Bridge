# 项目结构图 · M590Bridge

> 更新日期：2026-08-12
> 状态：文本+图片+流式文件+mDNS+Linux 发布；Windows 按粘贴网络流代码完成，待 Windows↔Linux 真机验收（task-020..044）

```text
crates/
  m590-core/       # 协议 v3、Message File*、FileCancel、Session 磁盘/流式收发/SHA-256
  m590-clipboard/  # 文本/图片/file_list；Windows 单文件 OLE 虚拟剪贴板原型
    src/virtual_file.rs          # 安全文件名、大小、惰性且可重复打开的内容工厂
    src/windows_virtual_file.rs  # IDataObject / FILEDESCRIPTORW / 延迟 IStream / STA guard
    examples/windows_virtual_file.rs # Windows Explorer 真机原型
  m590-net/        # 帧 1..15、版本拒绝、TCP
  m590-daemon/     # hub：临时令牌/CORS、send_file、无覆盖落盘、status、mDNS
    src/virtual_file_bridge.rs          # 有界网络字节管道、惰性请求、取消/超时
    src/windows_virtual_file_manager.rs # Windows STA/OLE guard 生命周期（Windows-only）
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
  tasks/           # ..044
  domain/          # 协议草案
```
