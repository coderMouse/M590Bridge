# task-032 · Linux .deb 安装包基线

## 状态

`done`

## 目标

为现有 Tauri 2 桌面端建立可重复验证的 Ubuntu/Debian `.deb` 打包基线，使用户无需从源码启动即可安装 `m590-ui`。

## 背景

- `ui/src-tauri/tauri.conf.json` 已启用 `deb` bundle，但尚无任务记录证明安装包能在当前仓库状态下生成。
- 当前计划把“安装包 / 开机自启”列为第一优先；本 task 只完成其中 Linux 安装包这一刀。

## 允许修改

- `ui/src-tauri/tauri.conf.json`
- `ui/src-tauri/Cargo.toml`（仅打包元数据需要时）
- Linux bundle 所需的静态资源或打包配置
- `ui/README.md`
- `docs/discovery/commands.md`
- `docs/plans/current.md`
- `项目说明.md`
- 本 task

## 禁止修改

- 协议、hub、剪贴板与文件传输业务逻辑
- React 页面和交互
- Windows 安装器、签名、自动更新
- Linux / Windows 开机自启
- Android、macOS 或其它平台实现

## 验证命令

```bash
cd ui && npm run desktop:build -- --bundles deb
dpkg-deb --info <生成的.deb>
dpkg-deb --contents <生成的.deb>
```

## 完成标准

- 当前 Linux 环境能生成 `.deb` 安装包。
- 包内至少包含 `/usr/bin/m590-ui`、桌面入口与应用图标。
- 文档写明构建命令、产物位置和安装/卸载命令。
- task 记录真实验证结果及未覆盖的平台风险。

## 实施记录

- 保留现有 Tauri `deb` target，补充 `Utility` 分类、短/长描述与 Debian `utils` section。
- 首次真实构建定位到构建机缺少 `libayatana-appindicator3-dev` 的 `pkg-config` 元数据；运行时库本身已存在。
- 当前会话无法通过 `sudo` 认证安装系统包，因此只将所需开发包解压到临时目录，用 `PKG_CONFIG_PATH` 完成构建；共享文档记录标准安装依赖。
- 生成最终 `.deb` 后解包检查可执行文件、桌面入口和多尺寸图标，并用 APT 模拟安装验证依赖可满足。

## 修改文件

- `ui/src-tauri/tauri.conf.json`：补齐安装包分类、描述与 Debian section。
- `ui/README.md`：增加 Ubuntu 构建依赖、`.deb` 构建、检查、安装与卸载说明。
- `docs/discovery/commands.md`：登记 Linux 打包命令与当前边界。
- `docs/plans/current.md`：记录 task-032 完成并把下一步收敛到登录自启。
- `项目说明.md`：纠正 V3/mDNS 旧状态并记录 Linux `.deb` 第一刀。
- `docs/tasks/task-032.md`：任务边界、实施与真实验证记录。

## 验证结果

- `cd ui && npm run desktop:build -- --bundles deb`：首次失败；release 可执行文件已构建，但 Tauri bundler 报 `Can't detect any appindicator library`，根因是构建机缺少开发包的 `.pc` 文件。
- `PKG_CONFIG_PATH=<临时开发包目录> npm run desktop:build -- --bundles deb`：通过；生成 `M590Bridge_0.1.0_amd64.deb`，最终构建耗时约 43 秒（首次完整 release 编译约 5 分钟）。
- `dpkg-deb --info target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包名 `m590-bridge`、版本 `0.1.0`、架构 `amd64`、section `utils`，依赖包含 AppIndicator/WebKitGTK/GTK。
- `dpkg-deb --contents target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；包含 `/usr/bin/m590-ui`、`M590Bridge.desktop` 与 32/128/512 像素图标。
- 解包后的路径断言：通过；`m590-ui` 是可执行的 x86-64 ELF，桌面入口 `Exec=m590-ui`、`Icon=m590-ui`、`Categories=Utility;`。
- `apt-get install --simulate ./target/release/bundle/deb/M590Bridge_0.1.0_amd64.deb`：通过；APT 计划新增 `m590-bridge`，现有运行时依赖可满足。
- `lintian`：跳过；当前环境未安装该命令。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`docs/discovery/commands.md`、`ui/README.md`、`项目说明.md`。
- 无需更新：协议/API/UI 规格与项目结构图；未新增模块、接口或页面。

## 风险 / blocker

- 未执行真实系统安装/卸载：当前会话无 `sudo` 认证；已用解包检查和 APT 模拟安装替代。
- 仅在 Ubuntu 26.04 amd64 构建；尚未在 Ubuntu 22.04/24.04 实机安装验证。
- 安装包未签名；Windows 安装包与开机自启不在本 task 范围内。

## 下一步

- 新建 `task-033`：Linux 用户级登录自启，并提供显式启停方式。
