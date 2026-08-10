# task-037 · 文件通道安全边界

## 状态

`completed`

## 目标

收紧 V2 文件与图片通道的输入及资源边界，避免远端字段影响接收目录之外的路径、压缩图片在解码时造成异常内存消耗，以及自动接收在多份 offer 或磁盘空间不足时无限消耗本机资源。

本 task 只处理输入验证、临时文件安全和接收资源上限；不处理 NACK/Cancel、会话加密、配对码随机性或独立数据连接。

## 允许修改

- `crates/m590-core/src/{protocol,session,lib}.rs`：transfer ID、图片像素、文件分片和接收资源边界；残留 `.part` 清理。
- `crates/m590-clipboard/src/lib.rs`：PNG 解码前后的尺寸/像素限制。
- `crates/m590-daemon/src/file_save.rs`：验证暴露的临时目录测试隔离问题（不改 hub 行为）。
- `crates/m590-core/Cargo.toml`、`Cargo.lock`：必要的跨平台磁盘空间查询依赖。
- `docs/plans/current.md`、本 task、必要的协议/能力文档：同步边界和验证结果。

## 禁止修改

- 文件失败反馈协议、取消/重试队列、会话加密、配对码生成和 mDNS。
- 独立文件数据连接、文件夹、OS 文件剪贴板、断点续传、多 peer。
- 与本 task 无关的 UI 布局、安装包、自启和全局格式化。

## 实现要求

1. `transfer_id` 只能是受限 ASCII 单路径标识；offer/request/chunk/complete 统一验证，不能通过 `..`、分隔符、控制字符或过长字段逃逸临时目录。
2. 临时文件使用不跟随已有路径的创建方式；同名 `.part` 不覆盖，内部接收目录只清理直接子项中的 `.part` 残留。
3. 图片声明尺寸和 PNG 实际解码尺寸都受像素上限约束；解码前先读取 PNG 元数据，不能只依赖压缩字节数。
4. 单个文件、所有待接收/接收中的文件总量和当前文件所在卷可用空间都必须通过检查后才创建临时文件。
5. 恶意输入被拒绝或转为本地可见的失败事件，不得 panic、覆盖接收目录外文件或无限增长内存/磁盘。

## 验证命令

```bash
cargo test -p m590-core -p m590-clipboard -p m590-net -p m590-daemon --lib
cargo check -p m590-core -p m590-clipboard -p m590-daemon
cargo clippy -p m590-core -p m590-clipboard -p m590-net -p m590-daemon --lib --no-deps -- -D warnings
cargo fmt --all -- --check -- crates/m590-core/src crates/m590-clipboard/src
```

## 完成标准

- [x] 恶意 `transfer_id` 在协议构造、网络解码和 session 落盘路径均不能逃逸 `.partial`。
- [x] 已有 `.part`、符号链接或同名临时文件不会被远端传输覆盖。
- [x] 超大声明尺寸、超大 PNG 解码尺寸和过大分片被拒绝。
- [x] 接收总量、单文件大小和可用磁盘不足时不会创建/继续写入临时文件。
- [x] 会话初始化会清理内部接收目录中的残留 `.part`，断开清理仍保持有效。
- [x] 现有小文件、空文件、路径流式和图片回归测试通过。

## 实施记录

- 在协议构造、网络帧解码和 `Session` 事件处理处统一校验 `transfer_id`：限制为安全 ASCII 单路径组件，拒绝空值、`.`、`..`、分隔符、控制字符和超长值；同时限制 `FileChunk` 与 inline 图片数据大小。
- 为图片声明尺寸增加 16M 像素上限；PNG 接收先读取元数据，再用 `image` 解码限制执行正式解码，并对最终 RGBA 尺寸再次校验。
- 将接收文件改为磁盘流式写入：单文件软上限 8 GiB、待接收/接收中总预留上限 8 GiB，并在创建临时文件前检查目标卷可用空间。
- 临时文件使用 `create_new`，避免覆盖已有文件或跟随符号链接；设置接收目录时只清理其直接子项中的残留 `.part`，断开时清理当前传输临时文件。
- 为协议、网络解码、路径穿越、残留文件、覆盖保护、接收配额、磁盘空间检查和 PNG 解码边界补充回归测试；修复 daemon 文件保存测试临时目录的并发重名问题。

## 修改文件

- `crates/m590-core/src/protocol.rs`：新增传输标识、图片、分片和 SHA-256 字段边界校验。
- `crates/m590-core/src/session.rs`：增加接收配额、卷空间检查、安全临时文件创建、残留清理和流式接收保护。
- `crates/m590-core/src/lib.rs`：导出安全边界相关协议常量/类型。
- `crates/m590-core/Cargo.toml`、`Cargo.lock`：加入跨平台磁盘空间查询与 SHA-256 依赖。
- `crates/m590-clipboard/src/lib.rs`：增加 PNG 元数据、解码分配和像素上限检查。
- `crates/m590-net/src/frame.rs`：在网络帧解码阶段限制图片与文件分片，并拒绝不安全传输标识。
- `crates/m590-daemon/src/file_save.rs`：保留最终文件的覆盖保护，并隔离测试临时目录。
- `docs/domain/protocol-draft.md`、`docs/plans/current.md`、`项目说明.md`：同步协议版本、文件/图片安全边界和当前计划。

## 验证结果

- `cargo test -p m590-core -p m590-clipboard -p m590-net -p m590-daemon --lib`：通过，clipboard 18、core 29、daemon 21、net 17，共 85 个测试。
- `cargo check -p m590-core -p m590-clipboard -p m590-daemon`：通过。
- `cargo clippy -p m590-core -p m590-clipboard -p m590-net -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cargo check --target x86_64-pc-windows-gnu -p m590-core -p m590-clipboard -p m590-daemon`：通过。
- `git diff --check -- <task-037 涉及文件>`：通过。
- `rustfmt --edition 2021 --check <task-037 涉及 Rust 文件>`：失败，原因是 `m590-clipboard` 模块递归检查包含仓库此前已有的格式差异；本 task 未执行全仓格式化。
- `cargo fmt --all -- --check -- crates/m590-core/src crates/m590-clipboard/src`：失败，命令参数包含目录且仓库存在多处既有格式差异；未执行全仓格式化。
- 验证期间默认 Cargo registry 缓存为只读，改用临时 Cargo 缓存目录完成上述真实验证；具体本机路径未写入共享文档。

## 文档影响检查

- 已更新：`docs/plans/current.md`、`docs/domain/protocol-draft.md`、`项目说明.md`，同步任务状态、协议版本和安全边界。
- 无需更新：`docs/discovery/*`、`docs/ui-spec.md`，本 task 未新增模块、命令或 UI 行为。
- 未更新：全局格式，仅记录为既有 blocker，避免扩大任务范围。

## 风险 / blocker

- `fs2::available_space` 已在 Linux 本机和 `x86_64-pc-windows-gnu` 目标完成编译检查；Windows 实机运行时的卷挂载、权限和空间错误文案仍需后续实机验证。
- 仓库仍存在 task-032..036 等未提交改动；本 task 未回退或整理这些既有改动。
- 全仓/模块递归 `rustfmt --check` 仍被既有格式差异阻塞；不影响本 task 的测试、编译和 Clippy 结果。

## 下一步

创建并单独处理 Linux 用户级登录自启 task；文件失败反馈、取消/重试、会话加密和配对码随机性继续留在后续 task。
