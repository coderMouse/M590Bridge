# task-010 · 配置持久化 + 断线自动重连

## 状态

`completed`

## 目标

把上次配对参数与同步偏好落到本机配置文件；断线后可按开关自动重连（host 重新监听 / joiner 重连对端）。

## 背景

- task-008/009：UI + hub 可配对与同步，但重启后参数丢失
- 跨机已验证文本同步；手动重配成本高
- 计划下一步：配置持久化、断线自动重连

## 允许修改

- `crates/m590-daemon/**`（config、hub、status、lib）
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`
- `docs/tasks/task-010.md`、`docs/plans/current.md`、`docs/discovery/*`

## 禁止修改

- 文件/图片通道、mDNS、安装包
- 协议帧格式大改
- git commit（除非用户要求）

## 验证命令

```bash
cargo test -p m590-daemon
cargo test
# 手工：
M590_CONFIG=/tmp/m590-test.cfg cargo run -p m590-daemon -- hub --api 127.0.0.1:5917
curl -s http://127.0.0.1:5917/api/config
curl -s -X POST http://127.0.0.1:5917/api/config \
  -H 'Content-Type: application/json' \
  -d '{"auto_reconnect":false,"listen_port":5902}'
```

## 完成标准

- [x] 本机配置可读写（路径可 `M590_CONFIG` 覆盖）
- [x] 持久化：device_id、role、code、port/addr、auto_sync、auto_reconnect
- [x] `GET/POST /api/config`；status 含 `auto_reconnect`
- [x] 非手动断开时，开启 auto_reconnect 会退避重试
- [x] 手动 `/api/disconnect` 不自动重连
- [x] UI 可开关并预填上次参数
- [x] 测试通过；文档更新

## 实施记录

### 修改文件

- `crates/m590-daemon/src/config.rs`（新建：读写 cfg、JSON patch）
- `crates/m590-daemon/src/status.rs`（auto_reconnect / listen_port / connect_addr 等）
- `crates/m590-daemon/src/hub.rs`（config API + reconnect 循环）
- `crates/m590-daemon/src/lib.rs`
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`
- docs：本 task、plan、commands

### 验证结果

- `cargo test -p m590-daemon`：通过（含 config roundtrip / json patch）
- `cargo test`：全绿（TCP 用例需非沙箱）
- hub 冒烟：`GET/POST /api/config` 写回文件；重启 hub 可 reload
- `cd ui && npm run build`：通过

### 文档影响

- 已更新 plan / commands
- 配置默认路径：Linux `~/.config/m590bridge/config.cfg`；Windows `%APPDATA%\M590Bridge\config.cfg`；可用 `M590_CONFIG` 覆盖

### 风险

- 自动重连默认开启；错误对端/错误码会周期性重试（可关）
- 配对码落盘仅本机文件，非安全边界
- 未在 Windows 实机验证 m590-ui 配置路径

### 下一步

- Windows 构建/运行 `m590-ui`
- 或图片/文件通道、mDNS、安装包
