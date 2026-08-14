# task-047 · Linux 关闭到托盘与桌面图标统一

## 状态

`in_progress`（代码与 Linux 启动验证完成，待 Linux/Windows 真机交互验收）

## 背景

Linux GNOME/Wayland 实机有两个桌面壳问题：

- 点击窗口右上角关闭后，主窗口只是最小化并设置 `skip_taskbar`，Ubuntu Dock 仍可能保留任务栏项。
- `npm run desktop:standalone` 直接运行 `target/release/m590-ui`，没有已安装的
  `.desktop` 应用身份；GNOME 按 Wayland `app_id` 查找图标失败后显示通用齿轮图标，
  即使 Tauri 窗口和托盘已经使用同一份位图图标。

task-028 曾将 `hide()` 改为最小化，以规避托盘恢复后标题栏按钮第一次点击不生效；
本 task 恢复真正隐藏，同时保留并验证现有的 `present()` 与聚焦补偿。

## 目标

- Linux 点击右上角关闭后真正隐藏主窗口和任务栏项，仅保留托盘。
- 从托盘恢复后窗口正常显示，右上角关闭按钮第一次点击即可再次隐藏。
- 主窗口与托盘显式复用同一份默认应用图标。
- Linux `desktop:standalone` 为 GNOME/Wayland 准备与 `m590-ui` app ID 匹配的
  用户级隐藏桌面身份和图标，避免任务栏显示通用齿轮图标。
- Windows 关闭到托盘、窗口图标和打包行为不回归。

## 允许修改

- `ui/src-tauri/src/lib.rs`：窗口关闭/恢复顺序与窗口图标设置。
- `ui/scripts/prepare-standalone.mjs`、`ui/package.json`：仅为 Linux standalone
  准备用户级 GNOME 应用身份；其它平台明确 no-op。
- `ui/README.md`：记录 standalone 的 Linux 桌面身份行为和清理方式。
- `docs/discovery/project-map.md`、`docs/discovery/commands.md`：新增脚本与命令副作用索引。
- 本 task、`docs/plans/current.md` 与 `AGENTS.md`：任务状态和后续验收入口。

## 禁止修改

- task-046 文件 offer/按需传输生命周期。
- task-042 Windows 登录自启、NSIS 安装器身份和卸载逻辑。
- Tauri 产品 `identifier`、Windows 安装身份或协议/API。
- 前端布局、多文件、文件夹、Linux FUSE 和断点续传。

## 完成标准

- [ ] Linux 点击 X 后窗口与 Ubuntu Dock 项消失，只保留托盘。
- [ ] 托盘“打开主面板”可恢复窗口，恢复后 X 第一次点击有效。
- [ ] Linux standalone 主窗口任务栏图标与托盘 M590Bridge 图标一致。
- [ ] Windows 原有关闭到托盘和图标行为不回归。
- [x] Rust 测试/检查/Clippy、前端 lint/build 与 Linux release 构建通过。

## 验证命令

```bash
cargo test -p m590-ui --lib
cargo check -p m590-ui --features custom-protocol
cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings
cd ui && npm run lint
cd ui && npm run build
cargo build -p m590-ui --release --features custom-protocol
```

Linux 真机复测：

1. `cd ui && npm run desktop:standalone`，确认 Ubuntu Dock 图标与托盘图标一致。
2. 点击 X，确认窗口和 Dock 项均消失，只剩托盘。
3. 从托盘选择“打开主面板”，确认窗口恢复，再点击一次 X 即可隐藏。

Windows 真机回归：运行 standalone 或安装版，重复关闭、托盘恢复和再次关闭，确认
窗口与图标行为保持正常。

## 实施记录

- Linux 真机验收发现：从托盘恢复后，第一次点击窗口右上角关闭按钮无效。首次返工将 GTK
  `present()`/聚焦补偿延迟到下一次主循环，但用户复测仍失败，证明问题不是单纯的窗口映射或
  激活时序。
- 进一步诊断与 Tao 0.35.3 上游缺陷 `tauri-apps/tao#1299`、`tauri-apps/tauri#15460`
  完全一致：Wayland 窗口 `hide()`/`show()` 后，Tao 的 CSD 标题栏保留过期指针状态；普通
  `present()`、聚焦和客户端 resize 均不能恢复，用户双击标题栏触发最大化后才恢复，是因为
  compositor configure 清除了该状态。Linux 恢复现仅在 Wayland 隐藏窗口上自动执行一次透明的
  最大化/还原 configure 往返，保留原最大化状态并在完成后显示；X11 与 Windows 恢复路径不变。
- 用户确认自动 configure 往返后，恢复窗口的关闭按钮第一次点击已生效，但窗口会移动到左上角。
  原因是上一版在 `show_all()` 后立即最大化，Wayland 尚未完成原几何位置的首次 remap configure，
  还原时 compositor 没有可恢复的位置。恢复流程现改为三阶段：先透明映射并等待 200ms，再执行
  最大化/还原 configure，最后显示窗口；不使用 Wayland 明确不可靠的手工 `set_position`。
- Linux `CloseRequested` 在 `prevent_close` 与 `skip_taskbar` 后改用真正的 `hide()`，
  避免最小化窗口仍出现在 Ubuntu Dock；Windows 等非 Linux 平台继续沿用已验证的
  `minimize()`，不改变现有关闭行为。
- 托盘恢复先撤销 `skip_taskbar`，再执行 `show()`、`unminimize()`；Linux 继续调用
  GTK `present()`，并保留 always-on-top/focus 脉冲以覆盖 task-028 的标题栏失焦问题。
- setup 阶段将同一份 `default_window_icon` 显式设置给主窗口与托盘。
- 新增 Linux standalone 预处理脚本：以原子替换方式写入用户级隐藏
  `m590-ui.desktop` 和 512×512 应用图标，桌面 ID/`StartupWMClass` 与实测 Wayland
  `app_id=m590-ui` 对齐；非 Linux 平台立即 no-op。
- `desktop:standalone` 通过 npm `pre` 生命周期自动运行预处理，正式 `.deb`/NSIS
  打包配置、产品 identifier 和 Windows 安装身份均未修改。

## 修改文件

- `ui/src-tauri/src/lib.rs`：Linux 关闭真正隐藏、恢复顺序和主窗口显式图标。
- `ui/scripts/prepare-standalone.mjs`：Linux standalone 的 GNOME 应用身份与图标准备。
- `ui/package.json`：接入 `predesktop:standalone`。
- `ui/README.md`：说明 Linux standalone 桌面身份和清理路径。
- `docs/discovery/project-map.md`、`docs/discovery/commands.md`：登记新增脚本与命令行为。
- 本 task、`docs/plans/current.md`、`AGENTS.md`：记录状态、验证和统一真机验收步骤。

## 验证结果

- `cargo test -p m590-ui --lib`：通过，8 项测试全部成功。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run lint`：通过。
- `cd ui && npm run build`：通过，Vite 构建 1804 个模块。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`：通过。
- `node --check ui/scripts/prepare-standalone.mjs`：通过。
- 临时 `XDG_DATA_HOME` 运行预处理并执行 `desktop-file-validate`：通过；生成
  `m590-ui.desktop` 与 512×512 PNG。
- 临时可写 XDG 运行目录启动 release 桌面壳 8 秒：成功提交 Wayland
  `app_id="m590-ui"`，内嵌 Hub 到达 `ready`，无 setup/tray panic；到时主动结束进程。
- `cargo fmt --check -p m590-ui`：被本 task 外既有 `ui/src-tauri/build.rs` 两空格
  缩进阻塞；未修改该无关文件，改为对本 task Rust 文件单独检查并通过。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-ui --target x86_64-pc-windows-gnu --features custom-protocol`：
  未完成；Tauri Windows 资源构建需要本机缺失的 `x86_64-w64-mingw32-windres`。
- 关闭、托盘恢复、Dock 最终图标与 Windows 桌面行为：待用户真机交互验收。

### 返工验证（恢复时序）

- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`：通过（补充延迟恢复代码格式修正后）。
- `cargo test -p m590-ui --lib`：通过，8 项测试全部成功。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run lint && npm run build`：通过，Vite 构建 1804 个模块。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- Linux 托盘恢复后的首次关闭点击：待用户真机复测。

### 第二次返工验证（Wayland CSD configure）

- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`：通过。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo test -p m590-ui --lib`：通过，8 项测试全部成功。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run lint && npm run build`：通过，Vite 构建 1804 个模块。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- Linux Wayland 隐藏/恢复后的标题栏交互：待用户真机复测；本机编译无法代替真实指针交互。

### 第三次返工验证（保留 Wayland 窗口位置）

- 用户真机结果：第二次返工后标题栏关闭第一次点击已生效；窗口位置移动到左上角，不通过位置保持。
- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`：通过。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo test -p m590-ui --lib`：通过，8 项测试全部成功。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run lint && npm run build`：通过，Vite 构建 1804 个模块。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- Linux Wayland 恢复位置与标题栏交互：待用户真机复测。

## 文档影响检查

- 已更新：本 task、当前计划、`AGENTS.md`、`ui/README.md`、项目结构图和命令索引。
- 无需更新：协议、Hub API、前端字段、打包命令和 UI 设计目标未变化，因此
  `docs/domain/*`、`docs/ui-spec.md` 与 `项目说明.md` 无需修改。

## 风险 / blocker

- GNOME/Wayland 的 Dock 图标依赖 `.desktop` 应用身份，纯 `set_icon` 不能替代该匹配；
  最终外观仍需当前 Linux 桌面真机确认。
- Tao 0.35.3 的 Wayland CSD 问题尚无直接上游修复；当前三阶段 configure 往返会让托盘恢复
  延迟约 500ms，并以透明窗口避免最大化闪烁，位置保持与最终效果仍需 Linux 真机确认。
- standalone 生成的桌面身份为 `NoDisplay=true`，不会增加应用菜单入口；停用源码
  standalone 后可按 `ui/README.md` 删除这两个用户级文件。
- 当前 Linux 环境不能运行 Windows 桌面壳，Windows 行为待与 task-046 一并真机验收。
- 当前环境缺少 MinGW `windres`，无法用 Windows GNU 交叉检查覆盖 Tauri 资源构建。

## 下一步

- 用户统一验收 task-046 的 3 组文件生命周期场景，以及本 task 的 Linux 3 步桌面
  交互和 Windows 关闭到托盘回归。
- 第三次返工后复测：先把窗口移动到明显不是左上角的位置 → 点击 X → 托盘打开主面板（允许约
  500ms 恢复）→ 确认窗口仍在原位置且第一次点击右上角 X 立即生效；同时观察是否有最大化闪烁。
