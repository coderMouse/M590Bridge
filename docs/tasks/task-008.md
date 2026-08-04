# task-008 · 可操作 UI 壳 + 本机 Hub API

## 状态

`completed`

## 目标

把 `ui/` 从「设计画廊」升级为 **可操作配对/主面板**，并通过本机 **hub 控制 API** 驱动 `m590-daemon` 的 listen/connect/同步（为后续 Tauri 托盘预留同一 API）。  
**不做** 完整 Tauri 打包与系统托盘实装（可后置）。

## 背景

- 用户希望有界面后再做跨机联调
- task-006/007 已有 TCP 文本同步与硬化
- 现有 `ui/` 仅为 Figma 对照画廊

## 允许修改

- `ui/**`
- `crates/m590-daemon/**`（hub API）
- `docs/**`、本 task、plan、discovery

## 禁止修改

- 文件/图片通道、Android、公网中继
- 无关大重构
- git commit（除非用户要求）

## 验证命令

```bash
cargo test
cargo build -p m590-daemon
cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
# 另一终端：
cd ui && npm run build
curl -s http://127.0.0.1:5910/api/health
```

## 完成标准

- [x] UI 默认进入可操作壳（配对 / 主面板 / 设置）
- [x] 设计画廊可切换进入，不删除
- [x] daemon `hub` 提供 status/listen/connect/push/disconnect
- [x] UI 能探测 hub、发起配对、展示状态与最近同步文本
- [x] 构建与 API 真实验证记录
- [x] 文档更新；跨机仍可等用户用界面测

## 实施记录

### 修改文件

- `crates/m590-daemon/src/hub.rs`、`status.rs`、`main.rs`（`hub` 子命令）
- `ui/src/app/OperableApp.tsx`、`ui/src/lib/bridgeApi.ts`、`ui/src/App.tsx`
- `docs/tasks/task-008.md`、`docs/plans/current.md`、`docs/discovery/*`、`ui/README.md`

### 验证结果

- `cargo build -p m590-daemon`：通过
- `cargo test`：通过
- `npm run build`（ui）：通过
- hub：`GET /api/health` → `{"ok":true}`
- hub `POST /api/listen` + CLI `connect --push-text` → status 出现 `last_sync_text`（随后对端退出导致 `peer disconnected` 为预期）

### 文档影响

- 已更新：本 task、plan、commands、project-map、ui README
- 待补：Tauri 托盘壳；跨机界面联调记录

### 风险

- hub 为最小 HTTP，非生产级；仅建议本机 loopback
- 断线后需在 UI 点断开/重新配对
- 尚无系统托盘

### 下一步

- 用户用「可操作壳 + hub」做本机/跨机联调
- 或新建 task：Tauri 2 托盘包装同一前端
