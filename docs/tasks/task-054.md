# task-054 · Linux / Windows 一键打包脚本

## 状态

`completed`

## 背景

- Linux `.deb` 与 Windows NSIS `.exe` 已分别在 task-032、task-042 建立并完成过真实打包。
- `docs/discovery/commands.md` 当前把依赖安装、前端验证、Tauri 打包和产物检查都展开为多条命令，日常重新打包输入较多。
- 本 task 只封装已有构建链，不改变安装器、自启或业务功能。

## 目标

- Linux 从仓库根目录执行一个 Bash 脚本即可生成 `.deb`。
- Windows 从仓库根目录执行一个 PowerShell 脚本即可生成 NSIS `.exe`。
- 两个脚本都检查基础构建环境、安装锁定的 Node 依赖、调用现有 Tauri 构建入口，并在成功后打印安装包路径。
- 文档区分“一键打包”和可选的安装包检查/安装/验收命令。

## 允许修改

- `ui/scripts/package-linux.sh`
- `ui/scripts/package-windows.ps1`
- `ui/package-lock.json`
- `ui/README.md`
- `docs/discovery/commands.md`
- `docs/discovery/project-map.md`
- `docs/plans/current.md`
- `AGENTS.md`
- 本 task

## 禁止修改

- Tauri bundle / NSIS 安装器配置与登录自启实现。
- 协议、Hub、剪贴板、文件传输和 React 页面。
- task-042 状态及其尚未完成的 Windows 真机验收项。
- 代码签名、自动更新、CI/CD、MSI 或其它平台打包。

## 验证命令

```bash
bash -n ui/scripts/package-linux.sh
./ui/scripts/package-linux.sh
dpkg-deb --info target/release/bundle/deb/M590Bridge_*_amd64.deb
git diff --check
```

Windows 10 真机或具备 PowerShell 的环境：

```powershell
.\ui\scripts\package-windows.ps1
Get-ChildItem .\target\release\bundle\nsis\*.exe
```

## 完成标准

- [x] Linux 脚本能在当前受支持的 Linux 构建机生成 `.deb`，失败时返回非零状态。
- [x] Windows 脚本使用仓库相对位置、失败时返回非零状态；用户已确认 Windows 构建机正常生成产物。
- [x] 两个脚本均实现成功后打印实际安装包路径、未找到产物时明确失败；两端均已有对应构建机确认。
- [x] 命令文档与 UI README 把日常打包入口缩减为每个平台一条命令。
- [x] 当前环境无法覆盖的 Windows 执行验证已如实记录。

## 实施记录

- 2026-08-17：建立独立任务；确认 Tauri `beforeBuildCommand` 已包含 `npm run build`，一键脚本无需重复执行前端构建命令。
- 2026-08-17：新增 `package-linux.sh` 和 `package-windows.ps1`。脚本均按自身位置解析仓库路径，执行锁定依赖安装、现有 Tauri 打包入口，并在找不到产物时以非零状态失败。
- 2026-08-17：Linux 脚本增加 GTK、WebKitGTK、Ayatana AppIndicator 的 `pkg-config` 预检；缺少开发库时直接打印 Ubuntu 安装提示，不自动修改系统环境。
- 2026-08-17：修复 Linux 通过 `sudo` 启动时因 `secure_path` 隐藏用户级 Node.js 的误报；脚本会切回 `SUDO_USER` 的登录环境执行，避免 root 所有的构建产物。
- 2026-08-17：Windows 脚本成功打印 NSIS 产物或捕获异常后，均等待任意按键再退出，便于双击脚本时查看路径和错误。
- 2026-08-17：用户在 Windows 10 构建机完成成功与异常两条路径复测；产物路径可见，两种结果都会等待按键后按原退出码结束。
- 2026-08-17：提交前在远端 lockfile 复现出 `nanoid <3.3.18` 导致的 5 个 high audit 告警；仅将传递依赖 `nanoid` 由 3.3.17 升到 3.3.18，不改 `package.json`，复验归零。

## 修改文件

- `ui/scripts/package-linux.sh`：Linux 环境预检、`sudo` 原始用户切回、`npm ci`、`.deb` 构建和产物输出。
- `ui/scripts/package-windows.ps1`：Windows/MSVC 环境预检、`npm ci`、NSIS 构建、产物输出及成功/异常按键暂停。
- `ui/package-lock.json`：锁定已修复 high audit 告警的 `nanoid 3.3.18` 传递依赖。
- `ui/README.md`：增加两端脚本入口及脚本职责说明。
- `docs/discovery/commands.md`：将日常打包命令收敛为每个平台一条，并保留可选检查/安装命令。
- `docs/discovery/project-map.md`：登记两个打包脚本。
- `docs/plans/current.md`、`AGENTS.md`：记录 task-054 状态与 Windows 验证边界。
- `docs/tasks/task-054.md`：本任务的边界、实施和验证记录。

## 验证结果

- `bash -n ui/scripts/package-linux.sh`：通过（含 `sudo` 用户切回逻辑的 shell 语法检查）。
- `./ui/scripts/package-linux.sh`：通过；脚本完成 `npm ci`（50 个包，0 vulnerabilities）、Tauri 前端 production build、Rust release 构建和 `.deb` bundle，生成 `target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb` 并打印绝对路径。
- `(cd ui/scripts && ./package-linux.sh)`：通过；从用户反馈中的脚本目录直接执行同样生成上述 `.deb`，无需 `sudo`。
- `dpkg-deb --info target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包名 `m590-bridge`、版本 `0.1.0`、架构 `amd64`、section `utils`，运行时依赖可见。
- `dpkg-deb --contents target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包含 `/usr/bin/m590-ui`、`M590Bridge.desktop` 和多尺寸图标。
- `npm run lint`：通过。
- `npm run build`：通过；TypeScript 与 Vite production build 成功，输出 `dist/` 产物。
- `npm audit --json`：通过；`info/low/moderate/high/critical/total` 均为 0。
- `cargo test --workspace`：通过；130 个单元测试全部通过，doc-tests 无失败。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，无 warning。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。
- `.\ui\scripts\package-windows.ps1`：通过；用户在 Windows 10 构建机确认 NSIS 产物路径正常显示，成功和异常后均等待按键，并分别保留成功/失败退出状态。
- `sudo -n true`：当前受限环境因 no-new-privileges 被拒绝，无法在本机模拟真实 `sudo` 用户切回；普通用户路径已通过完整 Linux 打包验证。

## 文档影响检查

- 已更新：`ui/README.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`docs/plans/current.md`、`AGENTS.md`、`docs/tasks/task-054.md`。
- 无需更新：协议、Hub API、UI 规格、Tauri bundle 配置和 `项目说明.md` 均未改变。

## 风险 / blocker

- 当前执行环境禁止提权，Linux 的 `sudo` → `SUDO_USER` 切回分支需由用户在真实 Ubuntu 终端复测；普通用户一键打包已真实通过。

## 下一步

- task-054 已完成。task-042 继续按用户决定暂停；若不恢复该验收，下一个独立任务建议先对文件数据与控制消息共用单连接的性能影响做定量基线，再决定是否实现独立文件数据连接。
