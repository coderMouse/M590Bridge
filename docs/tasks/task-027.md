# task-027 · Linux 桌面发文件 + 托盘文字 + 关闭按钮焦点

## 状态

`done`

## 问题

1. 本机桌面 `12.txt` 仍无法可靠发送
2. 窗口关闭按钮有时点不中
3. 托盘菜单两项无文字

## 实施

1. **原生选文件**：`pick_send_file`（rfd，默认桌面/桌面）→ `POST /api/send_file` path  
2. **窗口拖放**：`WindowEvent::DragDrop` → hub `send_file`  
3. **托盘**：`TrayState` manage 保活 Menu/MenuItem/TrayIcon；明确中文文案  
4. **焦点**：`show_main_window` = unminimize + show + always_on_top 脉冲 + set_focus；关闭仍 hide 到托盘

## 修改文件

- `ui/src-tauri/src/lib.rs`
- `ui/src-tauri/Cargo.toml`（rfd）
- `ui/src-tauri/capabilities/default.json`
- `ui/src-tauri/permissions/pick-send-file.toml`
- `ui/src-tauri/tauri.conf.json`
- `ui/src/lib/bridgeApi.ts`
- `ui/src/app/OperableApp.tsx`
- `docs/tasks/task-027.md`
- `docs/plans/current.md`
- `Cargo.lock`

## 验证

```bash
cargo build -p m590-ui
cd ui && npm run build
```

通过。

## 用户操作

1. 配对连接后点 **选择并发送文件**（原生对话框，默认桌面）选 `12.txt`  
2. 或把文件拖到 **应用窗口**  
3. 托盘应显示「打开主面板 / 退出」  
4. 点关闭应藏到托盘；再从托盘打开后标题栏关闭应可一次点中

## 文档影响

- 已更新：本 task、plans/current.md  
- 无需更新：协议  
