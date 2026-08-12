# task-042 · Windows NSIS 安装包与用户登录自启

## 状态

`in_progress`（Windows 10 已成功打包并安装；用户暂缓登录自启与跨机回归验收）

## 目标

为 Windows 10 提供 Tauri NSIS 当前用户安装包，并让设置页「登录时自动启动」通过当前用户注册表显式启停。Linux 完成代码与可用验证，最终安装包和运行行为由 Windows 真机确认。

## 实现选择

- 安装包：Tauri 2 NSIS `.exe`，`currentUser` 模式，不要求管理员权限。
- 登录自启：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `M590Bridge` 值。
- 注册表值始终写当前正式版 EXE 的带引号绝对路径；开发壳拒绝开启。
- 卸载：NSIS hook 删除当前用户的 `M590Bridge` 自启值。
- 构建：用户在 Windows 真机本地构建；不引入 GitHub Actions。

## 允许修改

- `ui/src-tauri/src/lib.rs`：Windows 自启后端与无平台依赖的值校验测试。
- `ui/src-tauri/Cargo.toml`、`Cargo.lock`：Windows-only `winreg` 依赖。
- `ui/src-tauri/tauri.conf.json`、`ui/src-tauri/windows/**`：NSIS 当前用户安装与卸载清理。
- `ui/package.json`：Windows 打包命令。
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`：Windows 显示并调用现有自启开关。
- 本 task、计划、命令/UI/项目说明等必要文档。

## 禁止修改

- GitHub Actions 或其它远端构建流水线。
- MSI、代码签名、自动更新、Windows Service、系统级开机服务。
- 配对、剪贴板、文件协议与默认端口。
- Android/macOS 支持。

## 验证命令

Linux 当前环境：

```bash
cargo test -p m590-ui --lib
cargo check -p m590-ui --features custom-protocol
cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings
cd ui && npm run lint
cd ui && npm run build
cargo build -p m590-ui --release --features custom-protocol
```

Windows 10 真机：

```powershell
cd ui
npm ci
npm run build
cargo test -p m590-ui --lib
npm run desktop:build:windows

Get-ChildItem ..\target\release\bundle\nsis\*.exe
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v M590Bridge
```

## 完成标准

- [x] Windows Tauri 壳显示「登录时自动启动」开关。
- [x] Windows 后端可写、读、删当前用户 Run 值，并拒绝开发壳开启。
- [x] NSIS 配置为当前用户安装，卸载 hook 清理 Run 值。
- [x] Linux 可执行的 Rust/前端/release 验证通过。
- [x] Windows 10 真机成功生成并安装 NSIS `.exe`（用户于 2026-08-11 确认）。
- [ ] 开启后注销/重新登录只启动一个 M590Bridge，托盘和内嵌 Hub 正常。
- [ ] 关闭后 Run 值消失，重新登录不启动；卸载后 Run 值也不存在。
- [ ] Windows 安装版与 Linux 完成文本、图片、文件回归。

## 实施记录

- Windows 使用 `winreg` 管理 HKCU Run 值，不执行 `reg.exe` 子进程，也不请求管理员权限。
- 读取开关时同时校验注册表命令是否仍指向当前 EXE，旧安装路径不会被误报为已开启。
- NSIS 使用 WebView2 download bootstrapper 和当前用户安装模式。
- 未添加 GitHub Actions；Windows 真机按文档本地构建。

## 修改文件

- `ui/src-tauri/src/lib.rs`：Windows HKCU Run 后端、开发壳保护和路径值测试。
- `ui/src-tauri/Cargo.toml`、`Cargo.lock`：Windows-only `winreg` 直接依赖。
- `ui/src-tauri/tauri.conf.json`、`ui/src-tauri/windows/installer-hooks.nsh`：NSIS 当前用户安装、WebView2 引导与卸载清理。
- `ui/src-tauri/permissions/autostart.toml`：权限说明覆盖 Linux/Windows。
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`：Windows Tauri 壳显示自启开关。
- `ui/package.json`、`ui/README.md`：Windows 打包脚本与操作说明。
- `docs/plans/current.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`docs/ui-spec.md`、`项目说明.md`：状态、命令、结构和产品边界同步。
- `docs/tasks/task-041.md`、本 task：记录 task-041 用户实机通过与 task-042 结果。

## 验证结果

- `cargo test -p m590-ui --lib`：通过，8 passed（含 2 个 Windows Run 值校验测试）。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run lint`：通过。
- `cd ui && npm run build`：通过，Vite 生成 production dist。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- `cargo check -p m590-ui --target x86_64-pc-windows-gnu --features custom-protocol`：首次因全局 Cargo cache 只读失败；改用 `/tmp` 独立 cache 后已编译 Windows 依赖和 `winreg`，最终被缺少 `x86_64-w64-mingw32-windres` 的 Tauri 资源构建步骤阻塞。未将其记为 Windows 构建通过。
- Windows NSIS 打包与安装：用户于 2026-08-11 确认通过。
- Windows 登录自启、卸载清理与跨机回归：尚未反馈，保持待验收。

## 文档影响检查

- 已更新计划、常用命令、项目结构、UI 规格、UI README 与项目说明。
- 无协议、Hub API、配置字段变化，`docs/domain/protocol-draft.md` 无需更新。

## 风险 / blocker

- 当前 Linux 环境没有 Windows MSVC、Visual Studio Build Tools、NSIS 与可交互 Windows 会话，不能在本机声明 Windows 安装/自启通过。
- Windows GNU 静态检查被缺少 `x86_64-w64-mingw32-windres` 阻塞；正式目标仍应在 Windows 上使用 MSVC 构建。
- 安装包未签名，Windows SmartScreen 可能提示未知发布者；签名明确不在本 task。
- WebView2 使用下载引导器；缺少 WebView2 的机器安装时需要网络。

## Windows 真机验收步骤

1. 安装 Node.js 22 LTS、Rust stable MSVC、Visual Studio Build Tools 2022（Desktop development with C++ + Windows SDK）。
2. 执行上述 Windows 构建命令并安装 `target\release\bundle\nsis\*.exe`。
3. 启动后确认托盘存在、主界面显示「API 已连接」。
4. 开启「登录时自动启动」，用 `reg query` 确认值为安装目录下带引号的 `m590-ui.exe`。
5. 注销并重新登录，确认只启动一个进程且内嵌 Hub 正常。
6. 关闭开关并再次查询；随后卸载，确认 Run 值不存在。
7. 与 Linux 对端回归文本、图片、文件。

## 下一步

- 用户已明确暂缓本 task；后续恢复时从登录自启、卸载清理与跨机回归继续。
