# task-055 · 多文件批次清单与路径安全基础

## 状态

`completed`（2026-08-17，本地协议与 workspace 验证通过）

## 背景

当前文件协议只描述单个文件：`FileOffer` 的 `file_name` 只能是 basename，现有
`FileRequest → FileChunk → FileComplete` 生命周期也只以一个 `transfer_id` 为单位。
多文件/文件夹需要先固定目录树清单与跨平台路径安全边界，再接入桌面选择、Windows
OLE 和 Linux FUSE。此任务只建立可验证的协议基础，不宣称已经支持桌面多文件粘贴。

## 目标

- 增加批次清单模型，包含 `batch_id`、显示名称和条目列表。
- 每个条目包含稳定的 `entry_id`、相对路径、`file`/`directory` 类型、文件大小和可选
  SHA-256；目录条目不得携带文件字节大小或哈希。
- 增加跨平台路径安全校验：只接受相对路径，拒绝绝对路径、`..`、空路径分量、NUL、
  Windows 盘符和 UNC 路径，并限制条目数、路径深度、清单大小和总字节数。
- 为批次清单增加新的 wire 消息类型，完成 encode/decode round-trip 与恶意路径拒绝
  测试；不改变现有单文件消息语义。

## 允许修改

- `crates/m590-core/src/protocol.rs`
- `crates/m590-core/src/error.rs`
- `crates/m590-core/src/lib.rs`
- `crates/m590-core/src/session.rs`（仅增加新消息尚未启用的拒绝保护与测试）
- `crates/m590-net/src/frame.rs`
- `crates/m590-net/src/lib.rs`（仅在测试或导出确有需要时）
- `docs/domain/protocol-draft.md`
- `docs/discovery/project-map.md`、`docs/discovery/commands.md`（仅同步协议能力边界）
- `docs/tasks/task-056.md`、`docs/tasks/task-057.md`、`docs/tasks/task-058.md`（仅登记后续边界）
- 本 task、`docs/plans/current.md`、必要的 `AGENTS.md` 状态记录

## 禁止修改

- `Session` 的实际发送/接收状态机、Hub HTTP API 和文件落盘行为（仅允许增加对新消息的
  明确“尚未启用”拒绝分支）。
- UI、Windows OLE、多文件系统剪贴板和 Linux FUSE 虚拟目录树。
- 断点续传、并行传输、独立数据连接、协议版本升级和 Android。
- 现有单文件 `FileOffer`/`FileRequest`/`FileChunk`/`FileComplete` 的字段或语义。

## 验证命令

```bash
cargo fmt --all -- --check
cargo test -p m590-core -p m590-net
cargo clippy -p m590-core -p m590-net --lib --no-deps -- -D warnings
git diff --check
```

## 完成标准

- [x] 批次清单及条目类型公开、可构造，并执行全部边界校验。
- [x] 新批次消息能稳定 wire encode/decode round-trip；非法路径和超限清单被拒绝。
- [x] 现有单文件协议测试保持通过，未接入发送/接收/UI 行为。
- [x] task、计划和文档影响记录已更新；未写入本机路径或敏感信息。

## 实施记录

- 2026-08-17：建立 task-055，并将多文件后续拆分为 task-056（选择与顺序传输）、
  task-057（Windows Explorer 多文件粘贴）、task-058（Linux FUSE 虚拟目录树）。本轮
  只执行 task-055。
- 2026-08-17：新增 `FileBatchOffer`（type 16）及 `BatchEntry` 模型；清单仍复用后续的
  单文件取流链路设计，不改变 type 11 至 15 的字段与语义。
- 2026-08-17：以平台无关的 `/` 相对路径作为 wire 规范，拒绝绝对路径、反斜杠、盘符、
  UNC、NUL、空组件、`.` 和 `..`；同时限制重复 id/路径、条目数、深度、清单编码大小和
  文件总字节数。
- 2026-08-17：单文件 Session 对 type 16 明确返回“尚未启用”，确保 task-056 接入前不
  会把可编解码的清单误当成已经支持的运行时能力。

## 修改文件

- `crates/m590-core/src/protocol.rs`：批次条目/清单模型、路径和大小限制、type 16 消息。
- `crates/m590-core/src/error.rs`：批次 id 空值错误。
- `crates/m590-core/src/lib.rs`：公开批次模型、校验函数和限制常量。
- `crates/m590-core/src/session.rs`：单文件 Session 对 type 16 的明确拒绝保护，不接入批次
  传输状态机。
- `crates/m590-net/src/frame.rs`：type 16 wire encode/decode、字段上限和恶意路径测试。
- `crates/m590-net/src/lib.rs`：全消息 round-trip 样本。
- `docs/domain/protocol-draft.md`、`docs/discovery/project-map.md`、
  `docs/discovery/commands.md`：同步协议草案与当前能力边界。
- `docs/tasks/task-055.md` 至 `docs/tasks/task-058.md`：收口当前任务并登记三个后续实现任务。
- `docs/plans/current.md`、`AGENTS.md`：同步 task-055 完成状态和唯一下一步。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p m590-core -p m590-net`：通过；m590-core 37 passed，m590-net 21 passed，
  doc-tests 无失败。
- `cargo clippy -p m590-core -p m590-net --lib --no-deps -- -D warnings`：通过。
- `cargo check --workspace`：通过。
- `cargo test --workspace`：通过；137 个单元测试全部通过，doc-tests 无失败。
- `git diff --check`：通过。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`AGENTS.md`、`docs/domain/protocol-draft.md`、
  `docs/discovery/project-map.md`、`docs/discovery/commands.md`。
- 无需更新：`项目说明.md`、UI 规格和 Hub API；本任务没有把多文件能力接入产品运行时。

## 风险 / blocker

- 本任务不覆盖 Windows/Linux 真机；多文件桌面语义必须在 task-056 至 task-058 和后续
  跨平台验收任务中分别验证。
- type 16 沿用协议版本 3 的扩展消息策略；旧构建收到未知类型会明确报协议不匹配，因此
  后续跨机测试必须保证两端使用同一构建。

## 下一步

- task-055 完成后，进入 task-056 的 UI 多选/拖放与批次串行传输；task-057/058 仍按平台
  分开实现和验收。
