# task-030 · 配对卡在「正在配对…」修复

## 状态

`done`

## 问题

用户反馈 UI 一直显示「正在配对…」，无法进入已连接/同步。

## 根因

1. **配对阶段无超时 / reject 不退出**：`run_session_loop` 在 `Connected` 前循环；收 `PairReject` 时 Session 只 `Ok` + Disconnected，hub 继续等 → 永久「正在配对…」  
2. **错码可被 auto_reconnect 掩盖**（若将来有其他失败路径）  
3. **本机配置被 task-029 smoke 污染**：`device_id=smoke-host`、`listen_port=15901`

## 修复

1. `Session`：`PairReject` → `Err(SessionError::PairRejected(reason))`  
2. hub 配对循环：  
   - 先 flush outbox 再传播 handle 错误  
   - Disconnected → 退出  
   - **30s pairing timeout**  
3. 错码/超时/device_id 冲突：**不** auto-reconnect，phase=`error` + 可读 `last_error`  
4. 默认 `device_id`：hostname sanitize + 4hex  
5. UI：pairing/error 展示 last_error  
6. 本机 config 重置为 `listen_port=5901` + 新 device_id（不入库）

## 修改文件

- `crates/m590-core/src/error.rs`、`session.rs`  
- `crates/m590-daemon/src/hub.rs`、`config.rs`  
- `ui/src/app/OperableApp.tsx`  
- docs：本 task、`plans/current.md`  
- 本机 `~/.config/m590bridge/config.cfg`（仅本机）

## 验证结果

```text
cargo test -p m590-core -p m590-daemon --lib
  core 19 passed；daemon 9 passed

双 hub：
  错码 111111 vs 222222 → joiner ~0.5s phase=error
    last_error=配对码错误或已过期…
  同码 333333 → 两端 connected
```

## 文档影响检查

- 已更新：本 task、plans/current  
- 无需更新：protocol 帧表（仅错误语义）  

## 风险

- 配对超时 30s 对极慢网络可能偏短（可后续可配置）  
- Windows 端需同步升级本修复  

## 用户操作

两端 `git pull && cargo build -p m590-ui` 后**完全退出重开**；确认设置里端口 5901、两端配对码一致。

## 下一步

安装包/自启；或跨机再验 pairing 提示文案。
