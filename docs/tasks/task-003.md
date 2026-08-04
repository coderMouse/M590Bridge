# task-003 · 核心协议与配对/会话草案

## 状态

`completed`

## 目标

在 `m590-core` / `m590-net` 中落地 **局域网 1 对 1** 的消息类型、帧/会话草案与单测：支持后续文本剪贴板同步挂接。  
本任务 **不实现** 真实 TCP 监听、真实剪贴板读写、UI、文件传输完整通道。

## 背景

- task-001 已提供可编译 workspace 与占位类型
- task-002 已完成 UI 设计参考壳
- 产品约束：预留 `DeviceId` 等多设备字段，首期只跑 1 对 1；Android 不做

## 允许修改

- `crates/m590-core/**`（消息枚举、错误类型、会话状态机草案等）
- `crates/m590-net/**`（帧编解码草案、配对码/握手状态、心跳占位；可用内存 mock，不必真听端口）
- 可选：`crates/m590-daemon/**` 仅打印草案状态，不接真实 I/O
- `docs/discovery/*`、`docs/domain/*`（若新建协议说明）
- `docs/plans/current.md`、本 task

## 禁止修改

- 真实剪贴板监听/写入业务（属后续 Linux/Windows clipboard task）
- 完整文件分片传输实现（V2+）
- Tauri / `ui/` 大改
- Android
- 无关依赖大升级
- git commit（除非用户明确要求）

## 验证命令

```bash
cargo test
cargo build
cargo run -p m590-daemon
```

## 完成标准

- [x] 定义至少：配对/握手、心跳、文本剪贴板同步相关消息类型（可 `enum`）
- [x] 帧或序列化草案有单测（编码→解码 roundtrip 或等价）
- [x] 会话状态迁移有单测（Disconnected → Pairing → Connected 等）
- [x] 1 对 1 实现但类型上保留 `DeviceId`（或等价）字段
- [x] 无真实 socket bind 要求；若做 demo listener 须可关且不阻塞测试
- [x] discovery / 可选 domain 文档已反映协议入口
- [x] 本 task 与 `docs/plans/current.md` 已更新

## 实施记录

### 修改文件

- `crates/m590-core/src/lib.rs`
- `crates/m590-core/src/error.rs`
- `crates/m590-core/src/protocol.rs`
- `crates/m590-core/src/session.rs`
- `crates/m590-net/src/lib.rs`
- `crates/m590-net/src/frame.rs`
- `crates/m590-net/src/pipe.rs`
- `crates/m590-daemon/src/main.rs`
- `docs/domain/protocol-draft.md`（新建）
- `docs/discovery/project-map.md`
- `docs/discovery/commands.md`
- `docs/plans/current.md`
- `docs/tasks/task-003.md`
- `docs/tasks/task-004.md`（下一任务，pending）

### 验证结果

- 命令：`cargo test`
  - 结果：**通过**（core 9 + net 6 + clipboard 3 + daemon 1；含帧 roundtrip、配对状态机、MemoryPipe 文本同步）
- 命令：`cargo build`
  - 结果：**通过**
- 命令：`cargo run -p m590-daemon`
  - 结果：**通过**，输出含 `protocol_version=1`、`demo_pairing=ok host=Connected joiner=Connected`、`demo_clipboard_content_id=demo-1`
- 无真实 socket bind

### 文档影响

- 已更新：本 task、`docs/plans/current.md`、`docs/discovery/project-map.md`、`docs/discovery/commands.md`、`docs/domain/protocol-draft.md`、`docs/tasks/task-004.md`
- 无需更新：`ui/`、`docs/ui-spec.md`
- 待补：加密套件、正式端口、TCP 传输 task 文档

### 风险 / blocker

- 帧格式与配对状态机仍为 draft，未做互操作兼容承诺
- 配对码明文占位，无加密（Q7 open）
- 默认端口仍为占位 `5901`（Q3 open）
- 未实现 OS 剪贴板与真实网络

### 下一步

- 执行 **task-004**：Linux 文本剪贴板读/写/监听抽象落地（仍可不接双机网络）
