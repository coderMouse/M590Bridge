# task-005 · Windows 文本剪贴板（cfg + 实装或验证缺口）

## 状态

`completed`

## 目标

为 `m590-clipboard` 提供 **Windows** 文本读/写/轮询监听能力：有 Win 环境则真实验证；无 Win 环境则 `cfg` 代码 + 清晰 blocker/复现步骤，不伪造“已在 Windows 验证”。

## 背景

- task-004 已在 Linux（Wayland）用 `arboard` 打通文本路径
- 产品首期包含 Windows 10；开发机可能只有 Linux

## 允许修改

- `crates/m590-clipboard/**`（Windows 模块 / 共享 trait 微调）
- 可选 workspace 依赖（Windows 剪贴板）
- 可选 `crates/m590-daemon/**` 日志
- `docs/discovery/*`、`docs/plans/current.md`、本 task

## 禁止修改

- 完整双机 TCP 同步（可另建 task-006）
- UI / Tauri 大改
- Android
- 伪造 Windows 验证结果
- git commit（除非用户明确要求）

## 验证命令

```bash
cargo test -p m590-clipboard
cargo build
# 若在 Windows：
cargo test -p m590-clipboard
cargo run -p m590-daemon -- --clipboard-demo
```

## 完成标准

- [x] Windows `cfg` 路径存在：read/write/poll 或明确 `Unsupported` 升级为实装
- [x] 有 Win 环境：真实 roundtrip 记录；无 Win：blocker + 复现步骤
- [x] Linux 回归：`cargo test -p m590-clipboard` 仍通过
- [x] discovery / plan / 本 task 已更新

## 实施记录

### 修改文件

- `crates/m590-clipboard/Cargo.toml`（`arboard` 同时用于 linux/windows）
- `crates/m590-clipboard/src/arboard_text.rs`（共享读写）
- `crates/m590-clipboard/src/linux.rs`（改用共享 helper）
- `crates/m590-clipboard/src/windows.rs`（新建：open/read/write/poll）
- `crates/m590-clipboard/src/lib.rs`（PlatformClipboard 接 Windows）
- `docs/discovery/*`、`docs/plans/current.md`、本 task
- `docs/tasks/task-006.md`（下一任务）

### 验证结果

- 命令：`cargo test -p m590-clipboard`（Linux host）
  - 结果：**通过**（6 tests，含 Linux 真实剪贴板集成）
- 命令：`cargo test`（全 workspace，Linux）
  - 结果：**通过**
- 命令：`cargo run -p m590-daemon -- --clipboard-demo`（Linux）
  - 结果：`clipboard_demo=ok backend=Wayland roundtrip=ok poll=ok`
- 命令：`cargo check -p m590-clipboard --target x86_64-pc-windows-gnu`
  - 结果：**类型检查通过**（产物含 `WindowsClipboard` 符号）
- 命令：Windows 实机 `cargo test` / `cargo test -p m590-clipboard`
  - 结果：**通过（用户反馈：测试全绿）**（2026-08-04，Windows 实机）
- 命令：Windows 实机 `cargo run -p m590-daemon -- --clipboard-demo`
  - 结果：**通过**，真实输出摘要：
    ```text
    clipboard_backend=Unspecified available=[Unspecified, Windows]
    demo_pairing=ok host=Connected joiner=Connected
    clipboard_demo=ok backend=Windows roundtrip=ok poll=ok
    ```

### Windows 复现步骤（已在实机跑通测试 + clipboard-demo）

```bat
cd <repo>
cargo test -p m590-clipboard
cargo test
cargo run -p m590-daemon -- --clipboard-demo
```

可选人工确认：demo 后记事本 Ctrl+V 应出现 `m590-daemon-clipboard-...` 文本。

### 文档影响

- 已更新：本 task、plan、discovery（写入 Win 实机测试通过）
- 无需更新：协议草案、UI
- 待补：无（Win 测试 + clipboard-demo 均已有记录）

### 风险 / blocker

- ~~无 Windows 实机测试~~ **已解除**
- ~~无 Windows clipboard-demo~~ **已解除**（`backend=Windows roundtrip=ok poll=ok`）
- Linux 开发机仍无 mingw，不能在 Linux 上链接 Win 二进制（不影响 Win 本机验证）
- 监听仍为轮询；极端 Win32 剪贴板锁场景未专项压测

### 下一步

- 执行 **task-006**：双端 daemon 文本同步最小路径（可先本机双进程 + TCP）
