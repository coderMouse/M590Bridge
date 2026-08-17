# task-057 · Windows Explorer 多文件剪贴板粘贴

## 状态

`pending`

## 背景

task-043/044 已验证 Windows 单文件 OLE 虚拟剪贴板与按需网络读取。此任务将同一能力
扩展为 Explorer 可识别的多文件 `IDataObject`，目录树语义以 task-055 清单为准。

## 目标

- Windows 发布批次清单对应的虚拟文件集合，Explorer `Ctrl+V` 时按需读取每个文件。
- 文件名、相对目录、大小和取消/替换生命周期与网络批次状态一致。
- 保留系统原生复制进度，不提前把全部文件落盘到接收目录。

## 允许修改

- `crates/m590-daemon/src/windows_virtual_file.rs`
- `crates/m590-daemon/src/hub.rs` 的 Windows 批次接线
- 必要的 Windows 构建/测试辅助代码
- 本 task 及必要文档

## 禁止修改

- Linux FUSE 虚拟目录树（task-058）。
- 现有单文件 OLE 行为、安装器、自启和断点续传。

## 验证命令

```bash
cargo test -p m590-daemon virtual_file_bridge
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples
```

Windows 10 真机：Explorer 多文件、嵌套文件夹、取消、替换、断线和重复粘贴。

## 完成标准

- [ ] Explorer 能粘贴多文件及嵌套文件夹，内容和相对路径一致。
- [ ] 系统进度、取消、替换和断线均无死锁或残留状态。
- [ ] 单文件回归保持通过；Windows 真机结果已记录。

## 下一步

- 依赖 task-056 的批次发送状态机完成后实现 Windows OLE 集合对象。
