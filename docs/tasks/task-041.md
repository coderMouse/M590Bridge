# task-041 · 桌面壳经 IPC 访问内嵌 Hub（修复仍不可达）

## 状态

`completed`

## 目标

修复 release/standalone 桌面端持续显示「内嵌 Hub 仍不可达」的问题。根因是 WebView（常为 `https://tauri.localhost`）用 `fetch` 访问 `http://127.0.0.1:5910` 被混合内容/跨域拦截；改为经 Tauri command 在 Rust 侧访问本机 Hub。

## 背景

- task-040 已让 Hub 先 bind、token 可重试，并区分离线原因。
- 用户复测后仍看到 `unreachable` 文案，说明 token/前端新逻辑已生效，但 `fetch` 到 Hub 仍然失败。
- 开发模式 `http://127.0.0.1:5173` → `http://127.0.0.1:5910` 通常可用；正式 custom-protocol 页面为 https 源时会拦截 http 请求。

## 允许修改

- `ui/src-tauri/src/lib.rs`：通用 localhost Hub HTTP 代理 command。
- `ui/src-tauri/permissions/**`、`capabilities/default.json`：登记许可。
- `ui/src/lib/bridgeApi.ts`：Tauri 壳走 IPC；浏览器开发模式仍用 fetch。
- `ui/src/app/OperableApp.tsx`：仅在必要时微调离线文案。
- `docs/plans/current.md`、本 task、必要 UI 说明。

## 禁止修改

- Hub 协议字段、配对/文件语义。
- Windows 安装包。
- 默认端口变更。

## 验证命令

```bash
cargo test -p m590-ui --lib
cargo check -p m590-ui --features custom-protocol
cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings
cd ui && npm run build
cargo build -p m590-ui --release --features custom-protocol
```

## 完成标准

- [x] Tauri 壳不再依赖 WebView `fetch` 访问内嵌 Hub。
- [x] health/status 等 API 经 IPC 可达（构建与路径校验单测通过）。
- [x] 浏览器开发模式仍可用 fetch + 独立 hub。
- [x] 文档与 task 已更新。

## 实施记录

- 新增 `hub_http_exchange` + `hub_api_request`：仅允许 `/api/*`，由 Rust 连接 `127.0.0.1:5910` 并注入进程令牌。
- `bridgeApi.request` / `probeHubHealth` 在 Tauri 壳走 IPC；浏览器模式保持 fetch。
- 登记 `allow-hub-api-request` 权限。
- 已 release + custom-protocol 构建，供 autostart/`desktop:standalone` 使用。

## 修改文件

- `ui/src-tauri/src/lib.rs`
- `ui/src-tauri/permissions/hub-api-request.toml`
- `ui/src-tauri/capabilities/default.json`
- `ui/src/lib/bridgeApi.ts`
- `docs/plans/current.md`、`docs/ui-spec.md`、本 task

## 验证结果

- `cargo test -p m590-ui --lib`：6 passed。
- `cargo clippy -p m590-ui --lib --no-deps --features custom-protocol -- -D warnings`：通过。
- `cd ui && npm run build`：通过。
- `cargo build -p m590-ui --release --features custom-protocol`：通过。
- GUI 实机：用户于 2026-08-11 确认通过，「API 已连接」且离线横幅消失。

## 文档影响检查

- 已更新计划、task、UI 规格中桌面壳 Hub 访问方式说明。

## 风险 / blocker

- 若用户仍运行旧 deb/旧二进制，需重装或改用新 release 路径。
- 超大 `send_file_bytes` 仍受 hub 4MiB 上限约束；路径发送不受影响。

## 下一步

- task-042：Windows NSIS 安装包与用户登录自启。
