# task-004 · Linux 文本剪贴板读/写/监听

## 状态

`completed`

## 目标

在 `m590-clipboard` 中实现 **Linux** 文本剪贴板的最小可用能力：读取当前文本、写入文本、变更监听（或可工作的轮询回退），并提供可测 API。  
本任务 **不要求** 完成双机网络串联；**不实现** Windows 实装（可保留 `cfg` 桩）；**不改** UI。

## 背景

- task-003 已有 `ClipboardText` 消息与会话挂接点
- Linux 需考虑 X11 / Wayland 差异（见 open-questions Q4）
- 无显示服务器或权限不足时允许记录 blocker 与降级策略

## 允许修改

- `crates/m590-clipboard/**`
- 可选：`crates/m590-daemon/**` 增加 Linux 下的演示子命令/日志（不得强制失败 CI 无显示环境）
- `Cargo.toml` / workspace 依赖（仅剪贴板相关，最小引入）
- `docs/discovery/*`、`docs/plans/current.md`、本 task
- 可选：`.agent/local-environment.md` 记录本机显示服务器（不进共享敏感细节）

## 禁止修改

- 完整双机 TCP 同步业务（可后续 task）
- Windows 剪贴板完整实现（除非顺带空 `cfg` 桩）
- 文件/图片剪贴板
- Tauri / `ui/` 大改
- Android
- git commit（除非用户明确要求）

## 验证命令

```bash
cargo test -p m590-clipboard
cargo build
# 若环境有显示服务器，可增加手动/集成检查并记录真实输出
```

## 完成标准

- [x] Linux 文本 read / write API 可用或明确降级错误类型
- [x] 监听或轮询能观察到本进程写入引起的变更（测试或手动记录）
- [x] X11/Wayland 策略写清（实现 + discovery/open-questions 更新）
- [x] 无显示环境时测试不谎报成功；记录 blocker
- [x] 本 task 与 plan / discovery 已更新

## 实施记录

### 修改文件

- `crates/m590-clipboard/Cargo.toml`（Linux 依赖 `arboard`，`default-features = false`）
- `crates/m590-clipboard/src/lib.rs`
- `crates/m590-clipboard/src/error.rs`
- `crates/m590-clipboard/src/linux.rs`
- `crates/m590-daemon/src/main.rs`（探测日志 + `--clipboard-demo`）
- `Cargo.lock`
- `docs/discovery/project-map.md`
- `docs/discovery/commands.md`
- `docs/discovery/open-questions.md`（Q4 结论）
- `docs/plans/current.md`
- `docs/tasks/task-004.md`
- `docs/tasks/task-005.md`（下一任务）
- `.agent/local-environment.md`（本机，不提交）

### 验证结果

- 命令：`cargo test -p m590-clipboard`
  - 结果：**通过**（6 tests，含 `linux_text_write_read_poll_if_clipboard_available`）
- 命令：`cargo build`
  - 结果：**通过**
- 命令：`cargo run -p m590-daemon`
  - 结果：`clipboard_detect=Wayland`，`clipboard_open=ok backend=Wayland`
- 命令：`cargo run -p m590-daemon -- --clipboard-demo`
  - 结果：`clipboard_demo=ok backend=Wayland roundtrip=ok poll=ok`
- 命令：`cargo test`
  - 结果：**通过**（全 workspace）
- 无显示环境策略：`PlatformClipboard::open` 返回 `ClipboardError`；集成测试 open 失败时 skip 并 `eprintln`，不伪造成功

### 文档影响

- 已更新：本 task、plan、discovery、open-questions Q4、新建 task-005
- 无需更新：`docs/ui-spec.md`、协议帧格式（未改）
- 待补：Windows 剪贴板实装；将 poll 事件挂到 Session/TCP

### 风险 / blocker

- 监听为 **轮询**（`poll_text_change`），非合成器事件订阅；高频场景需后续优化
- Wayland 下依赖会话 data-control 能力；部分锁定桌面可能限制
- 本机无 `wl-clipboard` CLI，但 `arboard` 库路径可用
- Windows 仍为未实现平台错误

### 下一步

- 执行 **task-005**：Windows 文本剪贴板（或明确 Win 验证缺口） / 或优先本地双进程同步（见 plan）
