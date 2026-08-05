# task-026 · Linux 桌面文件复制（GNOME Wayland）可用路径

## 状态

`done`

## 问题

Linux 桌面/文件管理器复制 txt 等文件，对端无反应。

## 根因（本机 GNOME Shell 50 + Wayland）

- `arboard` 的 wayland-data-control 需要 `ext-data-control` / `wlr-data-control`
- 本机 compositor **未提供**该协议 → arboard **回退 X11**
- Nautilus 等 Wayland 原生「复制文件」只进 Wayland 剪贴板，X11 侧读不到 `text/uri-list`
- 亦通常无 `text/plain` 路径 → path-text offer 也触发不了
- 文本同步仍可能工作（多数应用会往 X11 桥文本）

## 目标与结果

1. 路径/uri-list 解析硬化（CRLF/`\r`）— **已做**
2. UI 文件区 **拖入发送** — **已做**
3. 状态字段 `file_clipboard_watch_likely` + UI 琥珀提示 — **已做**
4. hub 启动日志 `file_clipboard_watch=limited` — **已做**

## 修改文件

- `crates/m590-clipboard/src/file_paths.rs`
- `crates/m590-clipboard/src/lib.rs`
- `crates/m590-clipboard/Cargo.toml`（linux `wl-clipboard-rs` 探测）
- `crates/m590-daemon/src/status.rs`
- `crates/m590-daemon/src/hub.rs`
- `ui/src/lib/bridgeApi.ts`
- `ui/src/app/OperableApp.tsx`
- `docs/tasks/task-026.md`
- `docs/plans/current.md`

## 验证

```bash
cargo test -p m590-clipboard -p m590-daemon -p m590-core -- --skip tcp::
cargo build -p m590-ui
```

- clipboard / core / daemon tests 通过；`resolve_trims_trailing_cr` 通过
- `m590-ui` build 通过

## 用户侧用法（GNOME Wayland）

不要依赖「在文件管理器 Ctrl+C 复制文件」自动同步。请：

1. 主面板 **选择并发送文件**，或  
2. 把桌面上的 txt **拖进**「文件传输」区域  

## 文档影响

- 已更新：本 task、`plans/current.md`
- 无需更新：协议文档
- 待补：无
