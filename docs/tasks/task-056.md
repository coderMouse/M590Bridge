# task-056 · 多文件选择与批次顺序传输

## 状态

`in_progress`（实现与本地自动化验证完成，待 Linux/Windows 桌面交互和跨机复测）

## 背景

task-055 固定了批次清单和路径安全边界。本任务把 UI 的多选/拖放输入转换为一个批次，
并沿用现有单文件请求与分片通道按条目串行传输。

## 目标

- UI 支持一次选择多个文件和包含文件的文件夹，生成稳定的批次顺序与相对路径。
- 发送批次清单后，按条目顺序复用单文件 `FileRequest → FileChunk → FileComplete`，
  同时暴露整体进度和当前条目进度。
- 接收端在清单确认前不创建越界路径；失败、取消和替换时清理整个批次状态。

## 允许修改

- `crates/m590-core/src/session.rs`
- `crates/m590-daemon/src/hub.rs`
- `crates/m590-daemon/src/status.rs`：批次整体/当前条目进度字段
- `ui/src/` 相关文件选择、拖放和进度组件
- `ui/src-tauri/src/lib.rs`、对应 permission/capability：原生多选、文件夹选择和多路径拖放
- 本 task 及必要的计划/发现文档

## 禁止修改

- Windows OLE 多文件 IDataObject（task-057）和 Linux FUSE 目录树（task-058）。
- 断点续传、并行条目传输和独立数据连接。

## 验证命令

```bash
cargo test -p m590-core -p m590-daemon
npm run lint
npm run build
```

## 完成标准

- [x] 多选/文件夹输入能生成合法批次，并由 Core loopback 验证按序完成传输。
- [x] 取消、替换、断线和非法清单均有确定状态；未提交批次的 `.part`/暂存树有自动清理测试。
- [ ] Linux/Windows 至少完成各自本地 UI 测试；跨机验收另行记录。

## 下一步

- 在 Linux 与 Windows 桌面端完成多选、文件夹、拖放、取消、替换和断线复测；确认后将
  本 task 标记 completed，再进入 task-057。

## 实施记录

- 2026-08-17：task-055 完成后启动本任务。复核现有入口发现原生对话框和窗口拖放由
  `ui/src-tauri` 负责，批次进度由 `HubStatus` 输出，因此将这两处补入允许范围；仍不
  修改 Windows OLE 或 Linux FUSE 虚拟目录实现。
- 2026-08-17：Core 增加 `offer_file_batch_paths` 与 `BatchOffered`，批次文件条目沿用
  现有 `FileRequest → FileChunk → FileComplete`，loopback 验证两个文件严格顺序完成。
- 2026-08-17：Hub 新增 `/api/send_batch` 与 `/api/cancel_batch`。目录扫描按相对路径稳定
  排序，不跟随 symlink，并在文件变化、重复路径、超限或非普通文件时拒绝；批次 ID 使用
  随机 nonce 与递增序号，条目 ID 在活动会话内唯一。
- 2026-08-17：接收端一次只请求一个文件，完成条目先进入
  `.partial/<batch_id>.batch/` 暂存树，整批完成后再发布到保存目录；取消、替换、失败和
  断线会取消剩余条目并清理 `.part`/暂存树。单顶层保留原顶层名，多顶层放入一个安全
  批次容器，同名目标使用既有后缀避让策略。
- 2026-08-17：Tauri 增加原生多文件/文件夹对话框，窗口拖放一次提交全部文件与目录；
  React 增加两个选择按钮、批次整体/当前条目双层进度和“取消整个批次”。浏览器开发
  模式仍只保留 4MiB 单文件回退。
- 2026-08-17：提交前审查补齐对端返回批次级失败时的发送端运行时清理，避免状态已失败但
  未请求条目仍留在 Session 中；随后重新通过任务要求的 Rust 测试、前端 lint 与构建。

## 修改文件

- `crates/m590-core/src/session.rs`、`src/lib.rs`：批次路径源、清单收发与条目顺序复用。
- `crates/m590-daemon/src/hub.rs`：批次 HTTP API、安全扫描、发送/接收状态机、暂存发布、
  取消/替换/失败清理及测试。
- `crates/m590-daemon/src/status.rs`：批次 ID/名称、文件数、总字节和当前相对路径字段。
- `ui/src-tauri/src/lib.rs`、`permissions/pick-send-file.toml`：原生多选、目录选择与多路径拖放。
- `ui/src/app/OperableApp.tsx`、`ui/src/lib/bridgeApi.ts`：批次 API、双层进度与取消 UI。
- `docs/domain/protocol-draft.md`、`docs/ui-spec.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`：同步运行时、API、交互和模块职责。
- `docs/plans/current.md`、`AGENTS.md`、`项目说明.md`、本 task：同步能力与验收边界。

## 验证结果

- `cargo test -p m590-core -p m590-daemon`：通过；Core 37、daemon lib 54、daemon bin 1，
  doc-tests 无失败。
- `npm run lint`：通过。
- `npm run build`：通过；TypeScript 与 Vite production build 成功（1804 modules）。
- `cargo test --manifest-path ui/src-tauri/Cargo.toml`：通过；8 passed。
- `cargo check --workspace`：通过。
- `cargo test --workspace`：通过；141 个单元测试通过，doc-tests 无失败。
- `cargo clippy -p m590-core -p m590-daemon --lib --bins --no-deps -- -D warnings`：通过。
- `cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets --no-deps -- -D warnings`：通过。
- `cargo check -p m590-core -p m590-daemon --target x86_64-pc-windows-gnu`：通过。
- `cargo check --manifest-path ui/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu`：未完成；
  当前 Linux 环境缺少 `x86_64-w64-mingw32-windres`，Tauri Windows 资源 build script 中止。
- `cargo clippy --workspace --all-targets -- -D warnings`：未通过；task-055 已提交的
  `protocol.rs` 测试触发 Rust 1.97 `manual_repeat_n`，本 task 未越界修改该历史测试；上述
  当前改动涉及的 lib/bin 与 Tauri Clippy 均通过。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

## 文档影响检查

- 已更新：本 task、计划、协议草案、UI 规格、命令和项目结构图；新增 Hub API、status
  字段、桌面选择入口和批次运行时均有对应文档。
- 无需新增 API inventory：仓库当前没有该文档，Hub 控制 API 继续由
  `docs/discovery/commands.md` 与 `docs/domain/protocol-draft.md` 共同登记。

## 风险 / blocker

- 当前环境可以完成 Linux 本地构建和协议 loopback；Windows 原生对话框与跨机批次传输
  仍需 Windows 真机复测，不会用本机结果代替。
- Linux 当前有图形会话，但自动化命令不能替代原生对话框/拖放人工交互；Linux 与
  Windows 均需真机确认按钮、多路径拖放、取消、替换、断线和最终目录内容。
- 本 task 的手动批次会自动保存到接收目录；Windows Explorer 多文件虚拟剪贴板和 Linux
  FUSE 目录树仍分别属于 task-057/task-058。
