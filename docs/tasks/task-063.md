# task-063 · Windows 粘贴含文件夹的批次时不显示系统复制进度窗口（优化项）

## 状态

`pending`（2026-09-01 用户 Windows 真机报告，标为优化项；机制已在代码中定位）

## 背景

用户真机：Windows 粘贴时，如果批次里**含文件夹**，就不会弹出 Windows 自带的
复制进度窗口。纯文件批次会弹（task-043/057 已验收「Explorer 按粘贴取流 +
系统复制进度」）。

## 机制（已定位，非推测）

`crates/m590-clipboard/src/windows_virtual_file.rs:327-338` 构造
`FILEDESCRIPTORW` 时：

```rust
dwFlags: (FD_ATTRIBUTES.0 | FD_UNICODE.0) as u32,
dwFileAttributes: if entry.is_directory() { FILE_ATTRIBUTE_DIRECTORY.0 } else { ... },
...
if !entry.is_directory() {
    descriptor.dwFlags |= (FD_FILESIZE.0 | FD_PROGRESSUI.0) as u32;
    descriptor.nFileSizeLow = entry.size() as u32;
}
```

**`FD_PROGRESSUI` 只加在非目录条目上**，目录描述符没有它。`FD_PROGRESSUI` 是
告诉 shell「请显示进度 UI」的标志，目录条目缺了它，Explorer 在含目录的批次上
就不显示进度窗口。

`FD_FILESIZE` 不加在目录上是**正确**的（目录没有流，`windows_virtual_file.rs:154`
的注释已说明不为 `FILE_ATTRIBUTE_DIRECTORY` 描述符提供流）；但 `FD_PROGRESSUI`
与是否有流无关，是纯 UI 提示。

## 目标

含文件夹的批次在 Windows 粘贴时也显示系统复制进度窗口，且不破坏 task-058 已验收
的目录树粘贴行为。

## 允许修改范围

- `crates/m590-clipboard/src/windows_virtual_file.rs`：目录描述符的 `dwFlags`

## 禁止修改

- 目录条目**不得**添加 `FD_FILESIZE` 或声明大小（目录无流，会破坏 task-058）
- 不改流提供逻辑、协议 wire、Hub HTTP API、UI
- 不改文件条目现有描述符（已验收）

## 实施要点（待实现时确认）

- 最小改动：把 `FD_PROGRESSUI` 移出 `if !entry.is_directory()`，对所有条目都加，
  `FD_FILESIZE` 与 `nFileSize*` 仍只给文件。
- **需真机验证是否真的生效**：`FD_PROGRESSUI` 的实际效果由 shell 决定，不能只凭
  标志推断。若无效，记录并考虑其它方向（如批次总大小的表达方式），不要盲目试探。
- 注意这是**优化项**，风险高于收益时应停手：task-058 的目录树粘贴已真机验收通过，
  任何回归都比缺一个进度窗口严重。

## 验证

- 代码级：`cargo test -p m590-clipboard --lib`、`cargo test -p m590-daemon --lib`、
  Windows 交叉 clippy（带与不带 `task-057-diagnostics`）`-D warnings`、
  native clippy `-D warnings`、`cargo fmt --check`、`git diff --check`
- 真机（Windows）：
  1. Linux 复制含文件夹的批次 → Windows 粘贴 → 出现系统复制进度窗口
  2. **回归（重于第 1 项）**：task-058 的目录树粘贴仍正确 —— 嵌套目录、空目录、
     空文件、大文件批次、取消、替换均如常
  3. 纯文件批次的进度窗口行为不变

## 文档影响（待实现后填写）

- 预计需更新：本 task、`docs/plans/current.md`
- 预计无需更新：协议 wire、Hub HTTP API、UI spec、安装器

## 实施记录

**2026-09-01**：按最小改动方案实现 —— 把 `FD_PROGRESSUI` 从 `if !entry.is_directory()` 块移出，对所有条目（文件 + 目录）都加入初始 `dwFlags`。`FD_FILESIZE` 与 `nFileSize*` 仍只给文件（目录无流）。

改动点：`crates/m590-clipboard/src/windows_virtual_file.rs:327`（初始 `dwFlags`）和 `:336`（文件专属标志）

```rust
// 改动前
dwFlags: (FD_ATTRIBUTES.0 | FD_UNICODE.0) as u32,
...
if !entry.is_directory() {
    descriptor.dwFlags |= (FD_FILESIZE.0 | FD_PROGRESSUI.0) as u32;

// 改动后
dwFlags: (FD_ATTRIBUTES.0 | FD_UNICODE.0 | FD_PROGRESSUI.0) as u32,
...
if !entry.is_directory() {
    descriptor.dwFlags |= FD_FILESIZE.0 as u32;
```

## 修改文件

- `crates/m590-clipboard/src/windows_virtual_file.rs`：`file_descriptor()` 函数，行 327 和 336 —— 把 `FD_PROGRESSUI` 提升到所有条目的初始标志，`FD_FILESIZE` 仍只给文件

## 验证结果（代码级）

```bash
cargo test -p m590-clipboard --lib  # 27 passed
cargo test -p m590-daemon --lib     # 75 passed; 2 ignored
cargo clippy -p m590-clipboard -p m590-daemon --lib --no-deps \
  --target x86_64-pc-windows-gnu -- -D warnings  # passed
cargo clippy -p m590-clipboard -p m590-daemon --lib --no-deps -- -D warnings  # passed
cargo fmt --check  # passed
git diff --check   # passed
```

**代码级验证全通过**。差异：2 行，+1 标志 / -1 标志的对称移动。

## 验证结果（真机）

**待真机验证**（Windows 环境）：

1. **主目标**：Linux 复制含文件夹的批次 → Windows 粘贴 → 是否出现系统复制进度窗口
2. **回归优先**（必须先做，任何一项失败立即回滚）：task-058 的目录树粘贴清单
   - 嵌套目录结构正确
   - 空目录、空文件
   - 大文件批次流式读取
   - 取消批次
   - 替换已存在的同名文件/目录
3. **次要回归**：纯文件批次的进度窗口行为不变

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`（状态同步）
- 无需更新：协议 wire（`FD_PROGRESSUI` 是 Windows OLE 内部描述符标志，不过网络）、Hub HTTP API（不涉及）、UI spec（进度窗口是 OS 行为，UI 无关）、安装器（零配置改动）

## 风险

1. **shell 决定论不确定**：`FD_PROGRESSUI` 只是「提示 shell 显示进度」，最终是否显示由 Windows Explorer 决定。可能存在其他隐藏条件（批次总大小、文件数量、Windows 版本差异），导致改动无效。若真机验证「仍无进度窗口」，记录此结论，不要继续试探其他方案（成本 > 收益）。

2. **task-058 回归风险**（优先级高于主目标）：虽然改动只碰描述符标志，不碰流逻辑（`windows_virtual_file.rs:154` 注释已明确目录条目不提供流），但 Windows COM `IDataObject` 的隐式耦合很难完全预测。**真机验证顺序：先跑 task-058 回归清单，全通过后再测主目标；任何回归立即回滚此改动。**

## 下一步

等待 Windows 真机验证（优先跑 task-058 回归清单）。
