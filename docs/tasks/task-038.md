# task-038 · Linux 用户级登录自启

## 状态

`pending`

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
- `ui/src-tauri/capabilities/default.json`:如需登记新命令许可。
- `ui/src/screens/SettingsScreen.tsx`:新增「开机自启」toggle 并接通 invoke。
- `ui/src/lib/bridgeApi.ts`:如需 autostart invoke 封装。
- `docs/plans/current.md`、`docs/discovery/commands.md`、`docs/ui-spec.md`、`项目说明.md`、本 task:同步状态、命令与 UI 行为。

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

- 设置页有「开机自启」开关;开启时在 `~/.config/autostart/M590Bridge.desktop` 创建合法 XDG autostart 入口(`Type=Application`、`Exec` 指向 `m590-ui`、`X-GNOME-Autostart-enabled=true`)。
- 关闭时移除该文件;再次打开恢复已存在状态(命令返回当前 bool)。
- 非 Linux 平台命令返回 `false`/不可用,不创建文件;UI 开关隐藏或禁用。
- `cargo check -p m590-ui` 与 `npm run build` 通过;记录真实验证结果。
- 文档写明开启方式、入口路径与移除/卸载行为。

## 实施记录

(待开始开发后填写)

## 修改文件

(待填写)

## 验证结果

(待填写)

## 文档影响检查

(待填写)

## 风险 / blocker

(待填写)

## 下一步

(待填写)
