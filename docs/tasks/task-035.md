# task-035 · Hub 与文件通道发布硬化

## 状态

`completed`

## 目标

修复 task-033/034 验收中确认的发布阻断项：保护 localhost Hub 控制 API、让 SHA-256 线协议变更具备明确版本边界，并补齐浏览器文件上传、失败队列和接收落盘的正确性。

本 task 只做安全与正确性硬化；不做吞吐架构重构、独立数据连接、Linux 登录自启或 Windows 安装包。

## 背景

- Hub API 当前无鉴权且响应 `Access-Control-Allow-Origin: *`，网页可跨域读取状态并调用配对、推送和本机路径发送接口。
- `FileOffer` / `FileComplete` 已增加 `sha256_hex`，但 `PROTOCOL_VERSION` 仍为 1，新旧构建无法在握手阶段明确拒绝。
- UI 标称浏览器回退支持 4 MiB，Hub HTTP body 却限制为 1 MiB。
- push API 在检查连接状态前写入全局 pending 槽，失败请求可能在后续连接中发送。
- 接收完成忽略 writer flush 错误，最终保存采用“先 exists 再 rename”，存在覆盖竞争窗口。

## 允许修改

- `crates/m590-core/src/{protocol,session}.rs`：协议版本、接收完成错误处理与测试。
- `crates/m590-net/src/frame.rs`：版本拒绝测试。
- `crates/m590-daemon/{Cargo.toml,src/hub.rs,src/file_save.rs,src/main.rs}`：临时鉴权令牌、Origin、HTTP body 上限、pending 顺序、防覆盖落盘与测试。
- `ui/src-tauri/src/lib.rs`、`ui/src-tauri/{capabilities,permissions}/**`：向内嵌 Hub 注入临时令牌，原生请求携带令牌，并限制 token command 权限。
- `ui/src/lib/bridgeApi.ts`：Tauri/开发模式取得令牌并发送鉴权 header。
- `Cargo.lock`：记录本 task 新增 Rust 依赖的锁定版本。
- `docs/domain/protocol-draft.md`、`docs/discovery/{commands,project-map}.md`、`docs/plans/current.md`、`项目说明.md`、本 task：同步行为、使用方式和新增权限文件索引。

## 禁止修改

- 独立文件数据连接、chunk/缓冲/并发吞吐重构。
- Linux/Windows 自启、安装包、自动更新与签名。
- 配对状态机、mDNS 发现和剪贴板语义的大改。
- 文件夹、OS 文件剪贴板、断点续传、多 peer、Android。

## 实现要求

1. Hub 启动时使用进程生命周期内的随机令牌；除明确的预检外，API 请求必须携带令牌。
2. CORS 不再使用通配符；仅允许 Tauri origin，debug 构建可允许 localhost 开发 origin；无 Origin 的 CLI 请求仍需令牌。
3. Tauri WebView 自动取得内嵌 Hub 令牌；共享文档只写占位符，不记录真实令牌。
4. 协议版本升级，新旧帧在解码入口明确返回 unsupported version。
5. 浏览器源文件上限保持 4 MiB；Base64 JSON body 上限与之匹配，并在读取前检查 Content-Length。
6. 未连接请求不得写入 pending 槽；断开时清理遗留命令。
7. 接收完成不得忽略 flush 错误；最终保存不得覆盖竞争期间出现的同名文件。

## 验证命令

```bash
cargo test -p m590-core -p m590-net -p m590-daemon --lib
cargo check -p m590-ui
cargo clippy -p m590-core -p m590-net -p m590-daemon --all-targets -- -D warnings
cd ui && npm run build && npm run lint

# Hub smoke：无令牌 401/403；正确令牌 200；恶意 Origin 拒绝；
# >1MiB 且 <=4MiB 源文件的 Base64 JSON 不被 HTTP reader 提前拒绝。
```

## 完成标准

- [x] 无令牌或非允许 Origin 不能读取状态或调用控制接口。
- [x] Tauri UI 能自动访问内嵌 Hub，CLI 有明确的令牌使用方式。
- [x] 协议版本与 SHA-256 payload 格式一致，旧版本在帧头处被拒绝。
- [x] 浏览器 4 MiB 上限与 Hub 实际接受上限一致。
- [x] 未连接 push/send 请求不残留；断开会清理 pending。
- [x] flush/save 失败可见，最终保存不会覆盖同名文件。
- [x] Rust 测试、UI build/lint、Tauri check 通过；Clippy 无本 task 新增告警。

## 实施记录

- Hub 每次启动生成 256 位进程临时令牌；独立 daemon 支持 `M590_HUB_TOKEN`，Tauri 通过受限 command 向主 WebView 提供内嵌 Hub 令牌。
- 所有非预检 API 校验 `X-M590-Token`；CORS 仅允许 Tauri origin，debug 额外允许 localhost/loopback 开发 origin；前端拒绝非 loopback API 地址，开发令牌不再允许放入 URL。
- HTTP reader 按路径设置 body 上限，在读取 body 前校验 `Content-Length`；`send_file_bytes` 的 Base64 JSON 空间与 4 MiB 浏览器源文件上限一致。
- pending 命令合并为一个互斥状态；连接检查、拒绝覆盖、断开/worker 清理共用同一同步边界，消费后再进入会话处理。
- 线协议升级到 v2，`FileOffer` / `FileComplete` 的 SHA-256 字段与版本边界一致，旧版本在帧头拒绝。
- 文件接收完成显式处理 `flush()` 错误；最终文件用 `create_new` / hard-link 安全占名，跨文件系统 fallback 也不覆盖并发出现的同名文件。
- Hub JSON 请求字段改用结构化解析，原生拖放使用 `serde_json` 序列化，保留带转义字符路径的正确性。

## 修改文件

- `crates/m590-core/src/{protocol,session}.rs`、`crates/m590-net/src/frame.rs`：协议 v2、SHA-256 payload、flush/断线清理和版本拒绝测试。
- `crates/m590-daemon/{Cargo.toml,src/hub.rs,src/file_save.rs,src/main.rs}`、`Cargo.lock`：令牌/CORS、HTTP 上限、pending 同步、结构化 JSON、安全落盘、CLI 帮助及测试。
- `ui/src-tauri/src/lib.rs`、`ui/src-tauri/capabilities/default.json`、`ui/src-tauri/permissions/hub-auth-token.toml`：令牌注入、command 权限、原生请求鉴权与状态检查。
- `ui/src/lib/bridgeApi.ts`：loopback 限制、Tauri/debug 令牌获取和鉴权 header。
- `docs/domain/protocol-draft.md`、`docs/discovery/{commands,project-map}.md`、`docs/plans/current.md`、`docs/tasks/task-035.md`、`项目说明.md`：协议、命令、结构、计划、任务记录和安全边界同步。

## 验证结果

- `cargo test -p m590-core -p m590-net -p m590-daemon --lib`：通过；core 22、daemon 20、net 14，均 0 failed。
- `cargo check -p m590-ui`：通过。
- `cd ui && npm run build && npm run lint`：通过；Vite 1804 modules transformed，oxlint 无错误。
- `cargo clippy -p m590-core -p m590-net -p m590-daemon --lib --no-deps -- -D warnings`：通过，无本 task 新增告警。
- task 原定 all-targets Clippy：未全绿；在进入本 task 代码前被 `m590-clipboard` 7 个既有告警阻断，见 blocker。
- Hub curl smoke：无令牌 `401`；正确令牌 `200`；恶意 Origin `403`；Tauri Origin `200` 且仅回显该 Origin；约 1.1 MiB 源文件的 Base64 JSON 到达业务层并因未连接返回 `400 not connected`，未被 HTTP reader 提前拒绝。
- `git diff --check`：本 task 文件无新增空白错误；全工作区仍报告 `AGENTS.md:27` 的 task-033 既有行尾空格。

## 文档影响检查

- 已更新：协议版本/Hub 鉴权、独立 Hub 与 curl 命令、项目结构、新增 Tauri permission、当前计划、产品安全边界。
- 无需更新：`docs/ui-spec.md`，本 task 未改变页面布局、视觉或文案交互。
- 敏感信息检查：共享文档只使用 `[REDACTED]`；smoke 临时令牌未写入仓库。

## 风险 / blocker

- 当前 Linux 环境无法实机验证 Windows WebView origin、原生拖放和 Windows 文件系统落盘；代码覆盖 `http://tauri.localhost` / `https://tauri.localhost`，仍需后续 Windows 安装包任务做实机 smoke。
- all-targets Clippy 被范围外既有告警阻断：`m590-clipboard` 7 个；daemon 测试 target 另有 `main.rs` 的 `single_match`，以及 `config.rs` / `status.rs` 的 `field_reassign_with_default`，共 3 个既有告警（均不属于 task-035）。本 task 的 `--lib --no-deps -D warnings` 已通过。
- 工作区在 task-035 前已包含 task-033/034 等未提交改动；本 task 未回退或整理这些改动。

## 下一步

- 新建并执行一个 Linux 用户级登录自启 task（需可显式启停）；不要在本 task 继续扩展。
