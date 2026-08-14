# task-045 · 新文件 offer 替换旧 offer 时不误报失败

## 状态

`in_progress`（代码完成，待 Windows↔Linux 真机复测）

## 背景

Linux 复制一个文件时，剪贴板后端偶尔会先后产生两个文件 offer。Windows 会正确保留较新的虚拟文件并取消旧 offer，因此 Explorer 仍可正常粘贴；但 Linux 收到旧 transfer 的 `FileCancel("replaced by a newer file offer")` 后，会无条件把 Hub 当前状态改成 `failed`，导致界面误报 `file transfer failed`。

## 目标

保留旧 offer 的取消与资源清理语义，但旧 transfer 的迟到失败/取消事件不得覆盖当前较新 offer 的状态和错误提示。真正属于当前 transfer 的失败仍需正常展示。

## 允许修改

- `crates/m590-daemon/src/hub.rs`：按 transfer ID 保护最新文件状态，并增加回归测试。
- 本 task 与 `docs/plans/current.md`：记录实施和验证结果。

## 禁止修改

- `m590-core` / `m590-net` 协议和 `FileCancel` 消息格式。
- Windows OLE 虚拟文件、网络流、SHA-256、超时和取消行为。
- 多文件、文件夹、Linux FUSE、断点续传。
- task-042 安装包与登录自启。

## 完成标准

- [x] 旧 transfer 被较新 offer 替换时，旧 transfer 的失败事件不覆盖较新 offer 的状态或 `last_error`。
- [x] 当前 transfer 的真实失败仍进入 `failed` 并展示原因。
- [ ] Windows 仍取消旧 offer，较新虚拟文件仍可粘贴，且 Linux 不再误报失败（待真机复测）。
- [x] Linux/Windows daemon 类型检查与相关测试通过。

## 验证命令

```bash
cargo test -p m590-daemon
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu
CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings
```

Windows↔Linux 真机复测：Linux 复制出现重复 offer 的文件，确认 Linux 不再显示 `replaced by a newer file offer`，Windows Explorer 仍可粘贴。

## 实施记录

- 新增 `mark_file_failed_if_current`：Hub 只允许无当前 transfer 或 transfer ID 与当前状态一致的失败事件进入 `failed`。
- Linux 收到旧 offer 的 `FileCancel` 后，Session 仍会终止旧发送并清理资源；Hub 发现状态已指向较新 offer 时忽略这条迟到的 UI 失败更新。
- Windows 入站失败路径使用同一保护，避免旧 transfer 的迟到事件覆盖较新的 OLE offer；当前 transfer 的真实失败仍原样展示。
- 增加两项回归测试，分别验证旧 ID 不覆盖新状态、当前 ID 仍进入失败状态。

## 修改文件

- `crates/m590-daemon/src/hub.rs`：按当前 transfer ID 更新失败状态，并增加回归测试。
- `docs/tasks/task-045.md`、`docs/plans/current.md`：记录任务边界、实施和验证状态。

## 验证结果

- `cargo test -p m590-daemon`：通过；daemon lib 27、bin 1，包含新增的 2 项状态回归测试。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu`：通过。
- `CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings`：通过。
- `rustfmt --edition 2021 --check crates/m590-daemon/src/hub.rs`：通过。
- Windows↔Linux 偶发重复 offer：待用户使用最新提交复测。

## 文档影响检查

- 已更新：本 task 与当前计划。
- 无需更新：协议、Hub API、UI 字段、运行/打包命令和产品边界均未变化，因此 `docs/domain/protocol-draft.md`、`docs/ui-spec.md`、`docs/discovery/*`、`项目说明.md` 无需更新。

## 风险 / blocker

- Linux 本机不能替代 Windows Explorer 真机确认；代码验证后仍需用户复测偶发现象。
- 本修复不阻止合法的新 offer 替换旧 offer，只阻止旧 transfer 的状态回执覆盖较新状态。

## 下一步

- 提交并推送后，由用户在 Windows↔Linux 真机重复复制文件，确认不再显示 `replaced by a newer file offer` 且 Explorer 仍可粘贴；通过后将 task-045 标记为 completed。
