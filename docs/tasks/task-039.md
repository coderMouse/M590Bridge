# task-039 · Linux 自启拒绝开发壳并提供独立桌面运行

## 状态

`completed`

## 目标

修复 Linux 登录自启误指向 Tauri 开发二进制后，因登录会话没有 Vite 开发服务器而显示 `Could not connect to 127.0.0.1: Connection refused` 的问题。

提供不依赖浏览器或前端开发服务器的源码桌面启动命令；开发壳不得再创建必然在下次登录失效的 autostart 入口。

## 已确认根因

- 当前 XDG autostart 入口的 `Exec` 指向 `target/debug/m590-ui`。
- `tauri.conf.json` 的开发模式页面是 `http://127.0.0.1:5173`；`desktop:dev` 会同时启动 Vite，因此开发期间正常。
- 用户登录时只执行上述 debug 二进制，不会启动 Vite，WebView 在应用 UI 加载前即连接失败。
- 内嵌 Hub 地址是 `127.0.0.1:5910`；本次报错不是要求用户使用 Web 端，也不是先启动独立 Hub 的正常产品流程。

## 允许修改

- `ui/src-tauri/Cargo.toml`：登记 Tauri `custom-protocol` feature。
- `ui/package.json`：增加构建并运行内嵌前端资源的 standalone 桌面命令。
- `ui/src-tauri/src/lib.rs`：开发模式开启 autostart 时返回明确错误；关闭旧入口仍保持可用。
- `ui/src/app/OperableApp.tsx`：桌面壳与浏览器开发模式使用不同的 Hub 离线提示。
- `ui/README.md`、`docs/discovery/commands.md`、`docs/plans/current.md`、`docs/ui-spec.md`、`项目说明.md`、本 task：同步运行方式与限制。

## 禁止修改

- Windows 自启、安装器、注册表或计划任务。
- Hub 端口、网络协议、配对、剪贴板或文件传输逻辑。
- 系统级 service 或全局 autostart。
- 自动修改用户现有的真实 XDG autostart 文件。

## 验证命令

```bash
cargo test -p m590-ui --lib
cargo check -p m590-ui
cd ui && npm run build
cd ui && npm run lint
cd ui && npm run desktop:standalone
```

运行期验证使用隔离的 XDG 目录：确保 `127.0.0.1:5173` 未监听时 release 桌面进程仍启动，且内嵌 Hub 监听 `127.0.0.1:5910`。

## 完成标准

- [x] `desktop:standalone` 构建并运行内嵌前端资源的桌面程序，不依赖 Vite/浏览器。
- [x] Tauri 开发模式开启登录自启时明确拒绝，关闭已有错误入口仍可执行。
- [x] 正式/standalone 构建仍能创建指向当前 release 二进制的 XDG autostart 入口。
- [x] 桌面壳 Hub 离线提示不再要求用户启动独立 Hub 或刷新网页。
- [x] 验证结果、迁移旧入口的操作和文档影响已记录。

## 实施记录

- 为 `m590-ui` 登记 `custom-protocol` feature，并增加 `npm run desktop:standalone`：先构建前端，再用 release profile 和 Tauri custom protocol 运行，前端资源随二进制内嵌。
- Linux 开启自启前检查 `tauri::is_dev()`。依赖 Vite 的开发壳返回可操作错误；关闭操作不受该保护影响，因此仍可删除旧的错误入口。
- 增加两种 feature 模式的回归测试：默认开发模式必须拒绝，`custom-protocol` 模式必须接受；原有 XDG 创建/删除和 `Exec` 转义测试继续覆盖正式入口生成。
- 桌面 Tauri 壳检测不到 Hub 时只提示内嵌 Hub 启动/重复进程问题；只有浏览器开发模式才展示独立 Hub 和 URL 指引。
- 未修改真实用户 autostart 文件。迁移方式是先关闭旧开关或删除旧入口，再运行安装版或 `desktop:standalone`，从设置页重新开启。

## 修改文件

- `ui/src-tauri/Cargo.toml`：登记 `custom-protocol` feature。
- `ui/package.json`：增加 `desktop:standalone`。
- `ui/src-tauri/src/lib.rs`：开发构建保护与 feature 模式测试。
- `ui/src/app/OperableApp.tsx`：区分桌面/浏览器 Hub 离线提示。
- `ui/README.md`、`docs/discovery/commands.md`：将 standalone/安装版列为日常桌面用法，并标出开发壳自启限制。
- `docs/plans/current.md`、`docs/ui-spec.md`、`项目说明.md`：同步 task 状态、UI 行为和产品运行边界。
- `docs/tasks/task-039.md`：根因、实现、验证、迁移与风险记录。

## 验证结果

- `cargo test -p m590-ui --lib`：通过，5 tests passed；默认 Tauri 开发模式命中自启拒绝策略。
- `cargo test -p m590-ui --lib --features custom-protocol`：通过，5 tests passed；standalone/正式模式通过自启策略。
- `cargo check -p m590-ui`：通过。
- `cargo clippy -p m590-ui --lib --no-deps -- -D warnings`：通过。
- `rustfmt --edition 2021 ui/src-tauri/src/lib.rs`：完成；后续 `--check` 复核通过。
- `cd ui && npm run build`：通过，1804 modules transformed。
- `cd ui && npm run lint`：通过，oxlint 无错误。
- 隔离 XDG 目录运行 `cd ui && npm run desktop:standalone`：release 首次完整构建通过并启动 `target/release/m590-ui`，输出 `hub_status=ready`；Mesa/软件渲染警告不影响应用启动。
- standalone 运行期间 `127.0.0.1:5173` 无监听且连接明确失败，同时 `127.0.0.1:5910` 正常监听；未带令牌请求 `/api/health` 返回预期 `401`，证明内嵌 Hub 可达且鉴权生效。
- 首次隔离运行曾同时覆盖 `HOME`，导致 rustup 在进入应用前找不到默认 toolchain；改为只隔离 XDG 目录后重跑通过，该失败不属于产品代码。

## 文档影响检查

- 已更新：`ui/README.md`、`docs/discovery/commands.md`、`docs/plans/current.md`、`docs/ui-spec.md`、`项目说明.md`、本 task。
- 无需更新：协议、Hub API、项目结构图与 domain 文档；未改变端口、接口、网络协议或模块职责，也未新增源码文件。

## 风险 / blocker

- 当前真实 autostart 文件在仓库外，本 task 未自动修改；用户需关闭旧入口（或删除入口文件），再从正式/standalone 桌面程序重新开启。
- 尚未在修复后执行真实注销/重新登录 smoke；运行了等价的 release 入口启动、无 Vite 环境和内嵌 Hub 监听验证，最终登录会话仍需用户复测。
- standalone 二进制位于忽略的 `target/release/`；清理仓库构建产物后入口会失效。长期日常使用优先安装 `.deb`，使入口稳定指向 `/usr/bin/m590-ui`。
- 当前 Agent 沙箱无法写桌面会话运行目录，以真实用户配置启动时托盘初始化被只读文件系统拒绝；该限制不出现在隔离 XDG 的成功 smoke 中，迁移入口需用户在正常桌面终端执行。

## 下一步

- 用户迁移旧 debug 入口后执行一次注销/登录 smoke；然后回到 task-036 的 Linux↔Windows 同文件、同网络吞吐复测。
