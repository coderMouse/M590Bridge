# task-062：Windows 粘贴多文件时「取消整个批次」按钮无效

**状态**：已实现，待 Windows 真机验证  
**创建**：2026-09-01  
**优先级**：高（bug，功能预期存在但失效）

## 背景

task-061 真机复测时用户发现：Windows 粘贴多文件批次时，前端「取消整个批次」按钮无效，
传输继续进行且前端没有响应。Linux 侧同一按钮功能正常。

## 问题定位

`hub.rs:2078-2217` 的 `cancel_batch` 处理块：

- **手动批次取消**（2083-2089）先跑，处理 `outbound_batch` / `inbound_batch`（手动
  发送/接收批次），调用 `cancel_runtime_batch`
- **Linux 虚拟批次取消**存在（2091-2118）：处理 `virtual_batch_receive` 与
  `deferred_virtual_batch_offer`，调用 `cancel_linux_virtual_batch` /
  `cancel_deferred_linux_virtual_batch` 并清理 `fuse_manager`
- **Windows 虚拟批次取消完全缺失**：没有对应的 `#[cfg(target_os = "windows")]` 分支

结果：按钮能入队 `pending.cancel_batch = true`，主循环读到该标志时 Linux 走 runtime +
虚拟批次双重取消，Windows 只走 runtime 批次取消（发现无 runtime 批次于是什么都不做），
剪贴板虚拟批次从未被接入取消路径。

## 实施方案

在 Linux 虚拟批次分支后新增**结构镜像的 Windows 分支**（`hub.rs:2119-2164`）：

1. 检查并 take `virtual_batch_receive`，调用 `cancel_windows_virtual_batch`
2. 检查并 take `deferred_virtual_batch_offer`，调用
   `cancel_deferred_windows_virtual_batch`
3. 清理 `ole_manager`（对应 Linux 的 `fuse_manager.clear()`）
4. 发 `task_057_diagnostic` 事件（诊断日志）
5. 更新前端状态（`file_transfer_phase: "cancelled"`、清 current_path/bytes）

**设计要点**：

- 逻辑与 Linux 分支对齐 —— 「用户显式取消」优先于「系统是否已开始接收」，take 两者之一
  都发网络取消并清管理器
- `ole_manager.clear()` 把 OLE 对象从剪贴板摘下；它产生的任何事件会被图片时代 drain
  消化（此时无 receive 在活跃），且下一个 offer 发布前会再调 `discard_stale_ole_events`
  （task-061 引入的双重保障）

## 修改文件

- `crates/m590-daemon/src/hub.rs:2119-2164`：新增 `#[cfg(target_os = "windows")]` 虚拟
  批次取消块，46 行

## 实施记录（2026-09-01）

代码已实现并通过本地验证，待 Windows 真机确认取消功能恢复。

**改动**：新增 Windows 虚拟批次取消分支，结构镜像 Linux 分支，调用既有
`cancel_windows_virtual_batch` / `cancel_deferred_windows_virtual_batch` helper，清理
`ole_manager` 并更新前端状态。

**为什么这样做**：Linux 侧从 task-056/058 起就有虚拟批次取消路径，Windows 侧初次
引入虚拟批次（task-057）时只抄了正向接收路径，取消路径漏抄。本轮补齐缺口，复用
Windows helper 签名（6213/6228），保持与 Linux 对称。

## 验证结果

本地：

```bash
cargo fmt && cargo fmt --check
cargo test -p m590-daemon --lib
cargo clippy -p m590-daemon --lib --no-deps --target x86_64-pc-windows-gnu -- -D warnings
cargo clippy -p m590-core -p m590-daemon -p m590-clipboard --lib --no-deps -- -D warnings
cargo check --workspace
```

- 所有本地测试通过（75 passed daemon，41 passed core，27 passed clipboard）
- 现有测试覆盖 `queue_batch_cancel` → `pending.cancel_batch = true` 入队路径
  （`hub.rs:7861-7911`），虚拟批次取消逻辑需真机验证

真机（Windows）：

1. 复制多个文件（≥3 个，总大小 ≥5MB）
2. Windows 粘贴到真实文件夹，传输开始
3. 前端点击「取消整个批次」
4. **预期**：传输立刻停止，前端状态转 "cancelled"，远端收到取消帧
5. **回归**：重复一次，确认下一个批次能正常粘贴（无残留 offer/transfer_id 冲突）

## 风险

- **低**：Windows 虚拟批次取消路径之前零覆盖，新增分支不影响原有任何路径
- `ole_manager.clear()` 在 task-061 之前未被主动调用过（只有 Drop 隐式清），但 task-061
  引入的双重 drain 机制（图片时代自 drain + `discard_stale_ole_events`）已在真机通过，
  新增的这处 clear 复用同一套防护

## 文档影响检查

- `docs/plans/current.md`：已列为下一步第 2 项
- `AGENTS.md`：已提及 task-062
- 无 API/数据模型/命令/构建步骤变化
