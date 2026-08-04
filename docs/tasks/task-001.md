# task-001 · 初始化 Rust workspace 骨架

## 状态

`completed`

## 目标

在仓库中建立 **Rust workspace** 与基础 crate 划分，使后续协议/剪贴板/网络/daemon 有可编译入口。  
本任务 **不实现** 真实剪贴板同步、配对逻辑、UI。

## 背景

- 项目已定：Rust、Linux + Windows、双机剪贴板桥、Android 暂缓
- 当前仅有文档，无 `Cargo.toml` / 源码
- UI 规格已存在，但不在本 task 范围

## 允许修改

- 新建 workspace 根 `Cargo.toml`
- 新建规划中的 crates（名称可微调，但需在 discovery 中反映），例如：
  - `crates/m590-core`
  - `crates/m590-clipboard`
  - `crates/m590-net`
  - `crates/m590-daemon`
- 各 crate 最小 `lib.rs` / `main.rs`、占位模块与 `README`（可选）
- `.gitignore` 中与 Rust 相关的必要补充
- `docs/discovery/project-map.md`、`docs/discovery/commands.md`
- 本 task 文件与 `docs/plans/current.md` 状态

## 禁止修改

- 实现剪贴板读写/监听业务
- 实现网络协议、配对、文件传输
- 新增 Tauri/UI 工程（除非仅空目录且计划未要求——本 task **不要**加）
- Android 相关代码或文档扩写为进行中工作
- 无关大重构、拷贝第三方大段业务代码
- 提交 git commit（除非用户明确要求提交）

## 验证命令

在仓库根目录执行（根据实际 package 名调整）：

```bash
cargo build
cargo test
```

若环境无 Rust toolchain：

1. 记录 blocker  
2. 不写「构建通过」  
3. 可在 `.agent/local-environment.md` 记录本机 rustc 缺失情况（不要写进共享 docs 的敏感路径细节）

## 完成标准

- [x] workspace 可 `cargo build` 通过（或已记录 toolchain blocker）
- [x] 至少包含 core / clipboard / net / daemon 四类占位划分（命名允许合理差异）
- [x] daemon 为可运行 bin 占位（打印版本或 “ok” 即可）
- [x] `docs/discovery/project-map.md` 改为反映真实目录
- [x] `docs/discovery/commands.md` 更新为可执行命令
- [x] 本 task 实施记录与验证结果已填写
- [x] `docs/plans/current.md` 已勾选本步并指向下一任务（下一 task 文件可在本任务结束时创建或标「待建」）

## 实施记录

### 修改文件

- `Cargo.toml`（workspace 根）
- `Cargo.lock`（首次 `cargo build` 生成）
- `crates/README.md`
- `crates/m590-core/Cargo.toml`、`crates/m590-core/src/lib.rs`
- `crates/m590-clipboard/Cargo.toml`、`crates/m590-clipboard/src/lib.rs`
- `crates/m590-net/Cargo.toml`、`crates/m590-net/src/lib.rs`
- `crates/m590-daemon/Cargo.toml`、`crates/m590-daemon/src/main.rs`
- `docs/discovery/project-map.md`
- `docs/discovery/commands.md`
- `docs/plans/current.md`
- `docs/tasks/task-001.md`
- `docs/tasks/task-003.md`（下一任务草案，pending）

### 验证结果

- 命令：`cargo build`
  - 结果：**通过**（编译 `m590-core` / `m590-clipboard` / `m590-net` / `m590-daemon`）
- 命令：`cargo test`
  - 结果：**通过**（9 个单元测试全部 ok）
- 命令：`cargo run -p m590-daemon`
  - 结果：**通过**，输出含 `M590Bridge daemon 0.1.0` 与 `status=ok`
- toolchain：`rustc 1.97.1` / `cargo 1.97.1`（Linux）

### 文档影响

- 已更新：`docs/plans/current.md`、`docs/discovery/project-map.md`、`docs/discovery/commands.md`、本 task、新建 `docs/tasks/task-003.md`
- 无需更新：`docs/ui-spec.md`、`ui/`、协议领域文档（尚无正式协议稿）
- 待补：协议消息与帧格式写入 discovery/domain（由 task-003 负责）

### 风险 / blocker

- 无 toolchain blocker
- 占位默认端口 `5901` 仅作骨架常量，生产端口仍见 open-question Q3，未锁定
- Windows `cfg` 分支已预留枚举，本机未做交叉编译验证
- 工作区 `.git` 异常时可能无法用 git 记录变更；未执行 commit（符合禁止项）

### 下一步

- 执行 **task-003**：核心协议与配对/会话草案 + 单测（仍不接真实剪贴板 I/O）
