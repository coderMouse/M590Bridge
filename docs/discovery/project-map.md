# 项目结构图 · M590Bridge

> 更新日期：2026-08-18
> 状态：文本+图片+单文件按需流+mDNS+Linux/Windows 发布；task-058 Linux FUSE tree 待 Nautilus 跨机验收

```text
crates/
  m590-core/       # 协议 v3、Message File*/FileBatchOffer、批次路径校验、Session 批次条目串行收发/SHA-256
  m590-clipboard/  # 文本/图片/file_list；Linux URI 探针；Windows 单/多文件 OLE 虚拟剪贴板
    examples/set_file_and_read.rs   # Linux text/uri-list 发布与 Nautilus 真机探针
    src/virtual_file.rs          # 安全文件名/相对路径、虚拟文件集合、惰性内容工厂
    src/windows_virtual_file.rs  # 多项 IDataObject / FILEGROUPDESCRIPTORW / 按索引延迟 IStream
    examples/windows_virtual_file.rs # Windows Explorer 单文件真机原型
    examples/windows_virtual_file_collection.rs # Windows Explorer 多文件/目录 OLE 探针
  m590-net/        # 帧 1..16、版本拒绝、批次清单字段校验、TCP
  m590-daemon/     # hub：send_file/send_batch、批次暂存或 Windows OLE/Linux FUSE tree 串行惰性流、status、mDNS
    src/linux_virtual_file.rs             # Linux-only 单文件/tree FUSE 元数据、路径安全、逐文件惰性读取与句柄回收
    src/linux_virtual_file_manager.rs     # Linux 单文件/tree 挂载、顶层文件 URI 列表发布、条件替换与清理
    src/virtual_file_bridge.rs          # 有界网络字节管道、非阻塞背压、请求/消费/释放、取消/超时
    src/windows_virtual_file_manager.rs # Windows STA/OLE 单文件或集合 guard 生命周期（Windows-only）
    examples/linux_virtual_file.rs      # Linux FUSE/Nautilus 按需读取与进度探针
ui/
  src/             # React：OperableApp 配对/发现/多文件与目录批次发送/双层进度/登录自启
  scripts/
    prepare-standalone.mjs # Linux standalone 的隐藏 GNOME 应用身份与图标
    package-linux.sh       # Linux .deb 依赖检查、锁定依赖安装、Tauri 打包与产物定位
    package-windows.ps1    # Windows NSIS 依赖检查、锁定依赖安装、Tauri 打包与产物定位
  src-tauri/       # Tauri 2 m590-ui；托盘、多文件/目录选择与拖放、Linux XDG / Windows HKCU autostart
    permissions/
      autostart.toml       # Linux/Windows 用户登录自启读写
      hub-auth-token.toml  # 主窗口读取进程临时 Hub 令牌
    windows/
      installer-hooks.nsh # NSIS 卸载清理当前用户 Run 值
docs/
  plans/current.md
  tasks/           # ..058
  domain/          # 协议草案
```
