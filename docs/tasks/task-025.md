# task-025 · 修复 file_list/文本路径发文件与发送方进度

## 状态

`done`

## 问题

1. Linux 复制 txt → Windows 无文件反应（常只有路径文本同步，或 file_list 后文本又冲掉）
2. Windows 复制 txt → Linux 已完成，Windows 仍「发送中」进度 0%（发送方送完 FileComplete 未更新 status）

## 目标

- 发送方在发出成功 `FileComplete` 后标记 `done` 与满进度
- 剪贴板文本若是本地非图片文件路径 → 走 file offer，不发纯路径文本
- file_list offer 成功后同步文本 baseline，避免紧接着再推路径文本
- 路径规范化（file:// 等）再 `is_file`

## 允许修改

- `crates/m590-daemon/**`、`crates/m590-clipboard/**`、docs 本 task / `plans/current.md`

## 实施记录

### 根因

1. hub 处理 `FileRequest` 并发送 `FileComplete` 后，sender status 仍停在 `sending` / `file_bytes_received=0`
2. Linux 复制文件常只出路径文本；`file_list` offer 后文本 poll 仍可能再推路径字符串

### 改动

- `m590-clipboard`：`regular_file_from_text`（规范化 path/URI，跳过图片扩展名）；`ClipboardService::adopt_text_baseline`（Linux/Windows/Null/Platform）
- `hub`：`note_outbound_file_completes` — outbox 含成功 `FileComplete` 时 phase=`done`、进度满
- `hub`/`main`：文本 poll 对本地非图片路径走 `offer_file`；`file_list`/path offer 成功后 `adopt_text_baseline`

### 修改文件

- `crates/m590-clipboard/src/file_paths.rs`
- `crates/m590-clipboard/src/image_file.rs`
- `crates/m590-clipboard/src/lib.rs`
- `crates/m590-clipboard/src/linux.rs`
- `crates/m590-clipboard/src/windows.rs`
- `crates/m590-daemon/src/hub.rs`
- `crates/m590-daemon/src/main.rs`
- `docs/tasks/task-025.md`
- `docs/plans/current.md`

## 验证

```bash
cargo test -p m590-clipboard -p m590-daemon -p m590-core -- --skip tcp::
cargo build -p m590-daemon -p m590-ui
```

结果（本机 2026-08-05）：

- clipboard 14 passed
- core 18 passed
- daemon lib 6 + main 1 passed
- `m590-daemon` / `m590-ui` build OK

跨机实机：需两端 pull/build 后测「复制 txt 文件」双向。

## 文档影响

- 已更新：本 task、`docs/plans/current.md`
- 无需更新：`项目说明.md`、`ui-spec.md`（行为修复，无新产品边界/UI 约定）
- 待补：无

## 验收建议（用户）

两端：

```bash
git pull && cargo build -p m590-ui && cargo run -p m590-ui
```

1. Linux 复制小 txt 文件 → Windows 应收文件并落盘，进度到完成  
2. Windows 复制小 txt 文件 → Linux 完成；Windows 也应显示完成而非卡在发送中 0%
