# task-058 · Linux FUSE 虚拟目录树

## 状态

`pending`

## 背景

task-052 已验证 Linux FUSE 单文件按需读取和 Nautilus 系统进度。此任务在不改变单文件
行为的前提下，把批次清单投影为只读虚拟目录树。

## 目标

- 收到批次清单后发布一个可浏览的只读 FUSE 目录树，不在首次发布时下载文件内容。
- Nautilus 读取具体文件时才为对应 entry 发起单文件请求，目录结构和路径严格来自已
  校验清单。
- 支持取消、替换、断线和挂载清理；保留 Nautilus 原生复制进度。

## 允许修改

- `crates/m590-daemon/src/linux_virtual_file.rs`
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`
- `crates/m590-daemon/src/hub.rs` 的 Linux 批次接线
- 必要的剪贴板 URI 发布辅助代码
- 本 task 及必要文档

## 禁止修改

- Windows OLE 多文件对象（task-057）。
- task-055 已定的 wire 字段和路径规则。
- 断点续传、并行读取和 Android/macOS 支持。

## 验证命令

```bash
cargo test -p m590-daemon virtual_file_bridge linux_virtual_file linux_virtual_file_manager
cargo check --workspace
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
```

Linux GNOME/Wayland + Nautilus 真机：浏览嵌套目录、粘贴多个文件、取消、替换、断线和
重复粘贴，并校验路径与内容。

## 完成标准

- [ ] 目录树可浏览，文件按需读取，内容和相对路径一致。
- [ ] 生命周期清理和单文件回归通过；Linux 真机结果已记录。
- [ ] 不因恶意清单创建绝对路径或路径穿越。

## 下一步

- 依赖 task-056 的批次状态与 task-055 的清单校验，完成 Linux FUSE 目录树投影。
