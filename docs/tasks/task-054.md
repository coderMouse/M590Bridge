# task-054 · Linux / Windows 一键打包脚本

## 状态

`in_progress`

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
- [x] Windows 脚本使用仓库相对位置、失败时返回非零状态；Windows 产物生成待构建机执行确认。
- [x] 两个脚本均实现成功后打印实际安装包路径、未找到产物时明确失败；Linux 已实测输出。
- [x] 命令文档与 UI README 把日常打包入口缩减为每个平台一条命令。
- [x] 当前环境无法覆盖的 Windows 执行验证已如实记录。

## 实施记录

- 2026-08-17：建立独立任务；确认 Tauri `beforeBuildCommand` 已包含 `npm run build`，一键脚本无需重复执行前端构建命令。
- 2026-08-17：新增 `package-linux.sh` 和 `package-windows.ps1`。脚本均按自身位置解析仓库路径，执行锁定依赖安装、现有 Tauri 打包入口，并在找不到产物时以非零状态失败。
- 2026-08-17：Linux 脚本增加 GTK、WebKitGTK、Ayatana AppIndicator 的 `pkg-config` 预检；缺少开发库时直接打印 Ubuntu 安装提示，不自动修改系统环境。

## 修改文件

- `ui/scripts/package-linux.sh`：Linux 环境预检、`npm ci`、`.deb` 构建和产物输出。
- `ui/scripts/package-windows.ps1`：Windows/MSVC 环境预检、`npm ci`、NSIS 构建和产物输出。
- `ui/README.md`：增加两端脚本入口及脚本职责说明。
- `docs/discovery/commands.md`：将日常打包命令收敛为每个平台一条，并保留可选检查/安装命令。
- `docs/discovery/project-map.md`：登记两个打包脚本。
- `docs/plans/current.md`、`AGENTS.md`：记录 task-054 状态与 Windows 验证边界。
- `docs/tasks/task-054.md`：本任务的边界、实施和验证记录。

## 验证结果

- `bash -n ui/scripts/package-linux.sh`：通过。
- `./ui/scripts/package-linux.sh`：通过；脚本完成 `npm ci`（50 个包，0 vulnerabilities）、Tauri 前端 production build、Rust release 构建和 `.deb` bundle，生成 `target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb` 并打印绝对路径。
- `dpkg-deb --info target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包名 `m590-bridge`、版本 `0.1.0`、架构 `amd64`、section `utils`，运行时依赖可见。
- `dpkg-deb --contents target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包含 `/usr/bin/m590-ui`、`M590Bridge.desktop` 和多尺寸图标。
- `npm run lint`：通过。
- `git diff --check`：通过。
- `package-windows.ps1`：当前 Linux 环境未执行；没有 PowerShell、Windows MSVC 或 NSIS，不能替代 Windows 真机验证。

## 文档影响检查

- 已更新：`ui/README.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`docs/plans/current.md`、`AGENTS.md`。
- 无需更新：协议、Hub API、UI 规格、Tauri bundle 配置和 `项目说明.md` 均未改变。
- 待补：在 Windows 构建机执行脚本后补充实际 NSIS 文件名与退出结果。

## 风险 / blocker

- 当前环境为 Linux，不能将 Windows PowerShell / MSVC / NSIS 构建记为真机通过；需记录可复现命令和实际未覆盖项。

## 下一步

- 在 Windows 10 构建机从仓库根目录执行 `.\ui\scripts\package-windows.ps1`，确认 NSIS `.exe` 路径输出；在此之前保持 task-042 的自启/卸载/跨机验收暂停状态。
