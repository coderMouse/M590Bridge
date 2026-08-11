# task-040 · 修复 Linux 内嵌 Hub 持续离线提示

## 状态

`completed`

## 目标

修复 Linux 桌面端启动后持续显示「内嵌 Hub 正在启动；若持续离线，请退出重复进程后重新打开 M590Bridge」的问题，让内嵌 Hub 更快就绪，并在失败时给出可操作的真实原因。

## 背景

- 用户反馈 Linux 启动后看到上述横幅。
- 现有 `run_hub_with_token` 在 `TcpListener::bind(5910)` 之前同步初始化 mDNS；登录/网络未就绪时可能拖住甚至卡住 API 监听。
- UI `getHubAuthToken` 首次失败会把 `null` 永久缓存，后续 health 永远失败。
- Hub bind 失败只 `eprintln`，WebView 无法区分“还在启动 / 端口占用 / 启动失败”。

## 允许修改

- `crates/m590-daemon/src/hub.rs`：先绑定控制 API，再启动 mDNS；必要时补充回归。
- `ui/src-tauri/src/lib.rs`：记录 Hub 就绪/错误状态，向 WebView 暴露只读查询。
- `ui/src-tauri/capabilities/default.json`、`ui/src-tauri/permissions/**`：登记新 command 许可（若新增）。
- `ui/src/lib/bridgeApi.ts`：token 失败可重试；health/离线原因更明确。
- `ui/src/app/OperableApp.tsx`：按真实原因展示离线提示。
- `docs/plans/current.md`、本 task、必要 UI/说明文档：同步状态。

## 禁止修改

- Windows 安装包/自启。
- 配对协议、文件传输协议、剪贴板语义。
- 端口默认值变更（仍为 `127.0.0.1:5910`），除非 task 明确扩展。
- 大范围 UI 重设计。

## 验证命令

```bash
cargo test -p m590-daemon --lib hub
cargo test -p m590-ui --lib
cargo check -p m590-ui --features custom-protocol
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
cd ui && npm run build
```

## 完成标准

- [x] 控制 API 在 mDNS 之前完成 bind；mDNS 失败不阻止 `/api/health`。
- [x] Hub token 首次获取失败后可重试，不再永久缓存 null。
- [x] 持续离线时 UI 能区分：启动中 / 端口占用 / 启动失败 / 鉴权失败。
- [x] 相关测试与 clippy 通过；文档已更新。

## 实施记录

- `run_hub_with_token` 先 `TcpListener::bind`，再通过 `on_ready` 回调通知桌面壳；mDNS 改为后台线程初始化，结果写入 `Mutex<Option<DiscoveryHandle>>`。
- 新增 `run_hub_with_token_on_ready`；桌面壳用 `HubRuntimeState` 记录 ready/error，并暴露 `hub_runtime_info` command。
- `bridgeApi` 仅在成功拿到 token 后缓存；失败可重试。`resolveHubOfflineReason` 结合 health 与 runtime 错误区分原因。
- 桌面离线横幅改为按原因显示可操作文案，不再只给笼统“正在启动/退出重复进程”。

## 修改文件

- `crates/m590-daemon/src/hub.rs`
- `ui/src-tauri/src/lib.rs`
- `ui/src-tauri/permissions/hub-runtime-info.toml`
- `ui/src-tauri/capabilities/default.json`
- `ui/src/lib/bridgeApi.ts`
- `ui/src/app/OperableApp.tsx`
- `docs/plans/current.md`、`docs/ui-spec.md`、本 task

## 验证结果

- `cargo test -p m590-daemon --lib hub`：10 passed（含 `hub_control_api_ready_before_mdns_init_completes`）。
- `cargo test -p m590-ui --lib`：5 passed。
- `cargo check -p m590-ui --features custom-protocol`：通过。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cd ui && npm run build`：`tsc -b && vite build` 通过。
- GUI 实机复测受当前沙箱限制（tray/dconf 只读会让本环境桌面进程崩溃）；需用户在本机 release/standalone 再确认横幅消失或原因文案正确。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`docs/ui-spec.md`（离线提示语义）。
- 无需更新：协议草案、discovery commands（无新用户命令）。

## 风险 / blocker

- 未在本环境完成真实 GUI 启动观察；若用户仍持续离线，优先检查是否有第二个 M590Bridge/`m590-daemon hub` 占用 5910。
- mDNS 仍可能在后台失败；发现列表会显示 unavailable，但不应再挡住控制 API。

## 下一步

- 用户本机验证 Linux 启动横幅。
- 然后创建 Windows 安装包/自启 task。
