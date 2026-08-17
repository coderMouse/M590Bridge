# task-053 · Linux 托盘菜单文字回归

## 状态

`completed`（2026-08-17，Linux GNOME/Wayland 真机复测通过）

## 背景

task-027 曾通过 `TrayState` 持有 `Menu`、`MenuItem` 和 `TrayIcon`，修复 Linux
AppIndicator 菜单两项无文字。task-052 真机验收完成后，用户再次发现本机托盘菜单有
两行但文字不显示。

现有源码仍保留 task-027 的对象保活和中文标签。2026-08-17 在同一 GNOME/Wayland
会话启动当前 release 后，DBusMenu `GetLayout` 与 GNOME Shell 可访问性树均能读取
“打开主面板 / 退出”，说明标签数据和对象生命周期在该次启动中正常；问题更可能发生在
AppIndicator 菜单首次挂接与 GNOME Shell 读取布局之间。

## 目标

- Linux 托盘菜单稳定显示“打开主面板 / 退出”。
- 菜单标签在托盘挂接完成后产生明确的属性更新，避免 GNOME Shell 偶发保留初始空标签。
- 保持菜单点击行为、托盘恢复、关闭到托盘和 Windows 行为不变。

## 允许修改

- `ui/src-tauri/src/lib.rs`：仅调整托盘菜单创建、挂接和标签刷新时序，增加相关小测试。
- 本 task、`docs/plans/current.md`、`AGENTS.md` 与必要的命令/项目说明状态记录。
- `.agent/local-environment.md`：仅记录本机真机环境与结果，不提交。

## 禁止修改

- task-052 FUSE、文件协议、Hub、剪贴板和网络传输实现。
- task-047 Wayland 标题栏、隐藏/恢复与 standalone 桌面身份方案。
- Tauri 前端布局、Windows 安装/自启、多文件、文件夹和断点续传。
- 依赖版本和协议/API。

## 验证命令

```bash
rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs
cargo test -p m590-ui --lib
cargo check -p m590-ui --features custom-protocol
cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings
cargo build -p m590-ui --release --features custom-protocol
```

Linux 运行时验证：启动 release，检查 DBusMenu `GetLayout` 与 GNOME Shell 可访问性树均
包含“打开主面板 / 退出”；用户真机点击托盘确认两项可见且可用。

## 完成标准

- [x] 标签在托盘菜单挂接后刷新且 Linux GNOME 不再显示空白菜单项。
- [x] “打开主面板 / 退出”行为、关闭到托盘和恢复行为未改动。
- [x] Rust 测试、检查、严格 Clippy 与 release 构建通过。
- [x] Linux 真机视觉复测通过。

## 实施记录

- 2026-08-17：建立独立回归任务；确认 task-027 保活代码仍在，当前 release 的 DBusMenu
  和 GNOME Shell 可访问性树均包含正确中文标签，将修复限定为挂接后的标签刷新时序。
- 2026-08-17：Linux 创建菜单时先使用空标签，`TrayIconBuilder::build` 完成 AppIndicator
  挂接后再调用 `MenuItem::set_text` 写入“打开主面板 / 退出”，强制发出 DBusMenu
  属性更新；Windows 初始标签路径保持不变。
- 2026-08-17：用户运行新 release 后确认 Linux 托盘菜单文字与交互真机复测通过，
  task-053 完成。

## 修改文件

- `docs/tasks/task-053.md`：定义回归范围、诊断事实和验证标准。
- `ui/src-tauri/src/lib.rs`：托盘挂接后的 Linux 菜单标签刷新时序。

## 验证结果

- 当前 release 启动：成功，内嵌 Hub 到达 `ready`。
- DBusMenu `GetLayout`：包含“打开主面板 / 退出”。
- GNOME Shell 可访问性树：包含并布局“打开主面板 / 退出”。
- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`：通过。
- `cargo test -p m590-ui --lib`：通过，8 passed、0 failed。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- 用户原始真机结果：托盘菜单行存在但文字不可见，不通过。
- 用户最终真机结果：新 release 托盘菜单文字可见且测试通过。

## 文档影响检查

- 已更新：task-052 完成事实、当前计划、`AGENTS.md`、命令文档与`项目说明.md`；本 task
  的实施记录与验证结果已同步。
- 无需更新：协议、Hub API、模块结构和 UI 规格未变化。

## 风险 / blocker

- 无 blocker。AppIndicator/GNOME Shell 菜单挂接仍属于平台兼容路径，后续升级
  Tauri、`tray-icon` 或 GNOME Shell 时需复查挂接后标签刷新是否仍必要。

## 下一步

- task-053 已完成；task-042 继续暂停，等待用户决定是否恢复 Windows 登录自启与卸载回归验收。
