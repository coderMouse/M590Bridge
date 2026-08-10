# task-038 · Linux 用户级登录自启

## 状态

`completed`

## 目标

让 `m590-ui` 在 Linux 用户登录时自动启动(托盘常驻),并可在设置页显式开启/关闭。使用 XDG autostart(`~/.config/autostart/M590Bridge.desktop`),不要求 root、不写系统级 systemd unit,卸载随用户配置自然消失。

本 task 只做 Linux 用户级自启;不做 Windows 自启、不做系统级 service、不做开机前/网络就绪延迟策略。

## 背景

- `docs/plans/current.md`「下一步」第 2 项即为此 task。
- task-032 已建立 `.deb` 基线,包内含 `/usr/share/applications/M590Bridge.desktop`(菜单入口),但**不含** autostart 入口,且 commands.md 明确写「不含开机自启」。
- 现有托盘已常驻、关闭即最小化到托盘(task-027),适合登录后后台常驻。
- `SettingsScreen.tsx` 现有 toggle 均为本地 `useState`(mock,不持久化);本 task 为首个真实持久化的设置开关。

## 允许修改

- `ui/src-tauri/src/lib.rs`:新增 `autostart_enabled` / `set_autostart` 命令(XDG autostart desktop 文件读写)。
- `ui/src-tauri/capabilities/default.json`、`ui/src-tauri/permissions/autostart.toml`:登记新命令许可。
- `ui/src/app/OperableApp.tsx`:在真实可操作设置页新增「开机自启」toggle 并接通 invoke。
- `ui/src/screens/SettingsScreen.tsx`:如需同步设计画廊中的设置页展示。
- `ui/src/lib/bridgeApi.ts`:如需 autostart invoke 封装。
- `docs/plans/current.md`、`docs/discovery/{commands,project-map}.md`、`docs/ui-spec.md`、`项目说明.md`、本 task:同步状态、命令、结构与 UI 行为。

## 禁止修改

- 协议、hub、剪贴板与文件传输业务逻辑。
- Windows 安装包、Windows 自启、注册表/计划任务。
- 系统级 systemd unit、`/etc/xdg/autostart` 全局入口、包安装期 hook。
- 文件夹、OS 文件剪贴板、断点续传、多 peer、会话加密、配对码随机性。
- 全仓格式化(沿用既有 blocker 约定)。

## 验证命令

```bash
# 编译
cargo check -p m590-ui
cd ui && npm run build
# 前端类型/构建

# 运行期验证(本机 Linux)
cargo run -p m590-ui &
# 在设置页开启自启 → 检查文件
ls -l ~/.config/autostart/M590Bridge.desktop
cat ~/.config/autostart/M590Bridge.desktop
# 关闭自启 → 检查文件移除
ls ~/.config/autostart/M590Bridge.desktop  # 应 not found
```

## 完成标准

- [x] 设置页有「开机自启」开关;开启时在 `~/.config/autostart/M590Bridge.desktop` 创建合法 XDG autostart 入口(`Type=Application`、`Exec` 指向 `m590-ui`、`X-GNOME-Autostart-enabled=true`)。
- [x] 关闭时移除该文件;再次打开恢复已存在状态(命令返回当前 bool)。
- [x] 非 Linux 平台命令返回 `false`/不可用,不创建文件;UI 开关隐藏或禁用。
- [x] `cargo check -p m590-ui` 与 `npm run build` 通过;记录真实验证结果。
- [x] 文档写明开启方式、入口路径与移除/卸载行为。

## 实施记录

- 在 Tauri 壳增加 `autostart_enabled` / `set_autostart` 命令。Linux 按 XDG 规则解析配置目录,`XDG_CONFIG_HOME` 仅接受绝对路径,否则回退到 `$HOME/.config`。
- autostart 入口使用当前运行的 `m590-ui` 绝对路径;`Exec` 对引号、反斜杠、美元符、反引号和 `%` 做 desktop entry 转义,并拒绝相对路径、非 UTF-8 路径和控制字符。
- 开启时先在同目录创建唯一临时文件、写入并同步,再原子替换 `M590Bridge.desktop`;关闭时只删除本应用入口,重复关闭保持幂等。
- Tauri command 通过独立 `allow-autostart` permission 暴露给主窗口。非 Linux command 返回 `false`,不执行文件写入。
- 可操作设置页仅在 Linux Tauri WebView 显示「登录时自动启动」;启动时读取磁盘状态,切换期间锁定控件,并以 command 返回值校准 UI。
- 顺带修正同一 Tauri 文件中两个既有等价 Clippy 告警(`into_iter_on_ref` / `needless_borrow`),不改变拖放发送行为。

## 修改文件

- `ui/src-tauri/src/lib.rs`:XDG 路径、desktop entry 安全生成、原子写入/删除、Tauri commands 和 Linux 单测。
- `ui/src-tauri/permissions/autostart.toml`:限制自启 commands 的 WebView 权限。
- `ui/src-tauri/capabilities/default.json`:主窗口启用 `allow-autostart`。
- `ui/src/lib/bridgeApi.ts`:封装自启状态读取/设置及 Linux Tauri 平台判断。
- `ui/src/app/OperableApp.tsx`:真实设置页自启状态、开关和错误/成功反馈。
- `docs/plans/current.md`、`docs/discovery/{commands,project-map}.md`、`docs/ui-spec.md`、`项目说明.md`:同步能力、使用方式、UI 与卸载边界。
- `docs/tasks/task-038.md`:任务状态、实施、验证、文档影响与风险记录。

## 验证结果

- `cargo test -p m590-ui --lib`:通过,3 个 autostart 测试全部通过;临时 XDG 目录中真实创建/读取/删除 desktop 文件,并覆盖 `Exec` 转义和非法路径拒绝。
- `cargo check -p m590-ui`:通过。
- `cargo clippy -p m590-ui --lib --no-deps -- -D warnings`:通过。
- `rustfmt --edition 2021 --check ui/src-tauri/src/lib.rs`:通过。
- `cd ui && npm run build`:通过;TypeScript 与 Vite production build 完成,1804 modules transformed。
- `cd ui && npm run lint`:通过;oxlint 无错误。
- `cd ui && npm run desktop:dev`:在 config/data/cache/state/runtime 全部隔离到临时 XDG 目录后成功启动 Tauri 桌面进程,Hub ready,命令权限清单加载成功;未触碰真实用户 autostart 文件。
- `cargo check --target x86_64-pc-windows-gnu -p m590-ui`:未完成;Tauri Windows resource build 在进入本 crate 编译前因环境缺少 `x86_64-w64-mingw32-windres` 退出。

## 文档影响检查

- 已更新:`docs/plans/current.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`docs/ui-spec.md`、`项目说明.md`。
- 无需更新:协议、Hub API 和 domain 文档;本 task 未改变网络协议、控制 API、配置 schema 或文件传输行为。
- 新增文件职责已登记:`ui/src-tauri/permissions/autostart.toml` 已加入项目结构图。

## 风险 / blocker

- 当前环境没有 MinGW `windres`,Windows 目标无法完成 Tauri 壳交叉检查;非 Linux command 已用 `cfg` 隔离并返回 `false`,仍需 Windows 构建环境复验。
- 当前验证覆盖开发二进制绝对路径;安装 `.deb` 后 `Exec=/usr/bin/m590-ui` 的最终入口需在安装包实机 smoke 中确认。
- `apt remove` 不会也不应遍历用户主目录删除 XDG autostart 文件;用户应在卸载前关闭开关,或卸载后手工删除入口。二进制不存在时该入口不会成功启动应用。

## 下一步

先完成 task-036 的 Linux↔Windows 同文件、同网络吞吐复测;后续开发单独创建 Windows 安装包/自启 task。
