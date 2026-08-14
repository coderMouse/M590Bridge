# task-046 · 文件 offer 与按需传输生命周期修复

## 状态

`completed`（2026-08-14，Windows↔Linux 真机验收通过）

## 背景

Windows Explorer 的虚拟文件流只能消费一次。实机发现两个生命周期问题：

- 一次粘贴完成或由 Explorer 取消后，Linux 再复制同一路径时，剪贴板内容比较认为文件列表未变化，Windows 因而收不到新的虚拟文件流。
- Windows 正在按需接收文件时，Linux 再复制其它文件会立即替换当前 OLE offer，并以 `replaced by a newer file offer` 取消正在使用的流；两端进度随后可能停在发送中/接收中。

## 目标

- 已被 Explorer 请求且尚未完成的虚拟文件继续传输，不被后续 `FileOffer` 中断。
- 活跃传输期间收到的新 offer 延后发布；多个延后 offer 只保留最新一个，并取消更旧的未请求 offer。
- 当前流完成或由 Explorer 取消后，发布延后的最新 offer；没有延后 offer 时，为仍在 Linux 剪贴板中的本地文件建立新的 transfer，使同一文件可再次粘贴。
- UI 状态在活跃传输期间继续指向当前 transfer，不被排队 offer 或旧 transfer 的结束事件覆盖。

## 允许修改

- `crates/m590-daemon/src/hub.rs`：Windows 虚拟文件 offer 排队/发布状态、发送端状态与同文件重新发布，并增加回归测试。
- `crates/m590-clipboard/src/lib.rs`、`linux.rs`、`windows.rs`：增加仅针对文件 offer 的轮询重新布防能力与测试。
- 本 task 与 `docs/plans/current.md`：记录任务边界、实施、验证和后续真机步骤。

## 禁止修改

- `m590-core` / `m590-net` 协议、消息格式、分片、SHA-256 和传输上限。
- 多文件、文件夹、Linux FUSE、断点续传或并行数据连接。
- task-042 的 Windows 自启验收。
- Linux 窗口关闭到托盘与任务栏图标；这两项留给后续 task-047。

## 完成标准

- [x] Windows 正在接收文件 A 时，Linux 复制文件 B 不会取消 A；A 完成后 B 成为可粘贴文件。
- [x] 活跃传输期间连续复制 B、C 时，只保留 C，未请求的 B 被取消。
- [x] 文件粘贴完成或 Explorer 取消后，当前本地文件获得新的 transfer，Windows 可再次粘贴同一文件。
- [x] 活跃 transfer 的发送/接收状态不被排队 offer 或旧 transfer 的完成/取消覆盖。
- [x] Linux 测试、Clippy 与 Windows GNU 类型检查/Clippy 通过；Windows 运行行为给出明确真机复测步骤。

## 验证命令

```bash
cargo test -p m590-clipboard
cargo test -p m590-daemon
cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu
CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings
```

Windows↔Linux 真机复测：

1. Linux 复制文件 A，Windows 粘贴；传输中 Linux 再复制文件 B，确认 A 完成且 B 随后可粘贴。
2. 传输中连续复制 B、C，确认 A 不被中断，完成后 Windows 粘贴得到 C。
3. A 粘贴完成后再次粘贴 A；再取消一次 Explorer 复制并重新粘贴，确认都有可用的新流。

## 实施记录

- Windows 新增单个 deferred offer：当前虚拟文件已经被 Explorer 请求且未完成时，新 `FileOffer` 不再替换当前 OLE 流；连续收到多个新 offer 时取消旧 deferred，只保存最新一个。
- 当前流完成、Explorer 关闭读取流或 OLE 发布失败后发布 deferred；Windows 本地剪贴板主动替换时取消当前和 deferred，避免旧远端文件抢回剪贴板。
- 发送端新增排队状态：活跃 transfer 的进度继续显示，新 offer 只记录为下一项；当前完成/失败后才晋升，旧完成事件和已取消 deferred 不会覆盖当前状态。
- 剪贴板抽象新增 `rearm_file_offer_poll`，只重置文件列表和路径文本基线，不重置位图。最新本地文件成功发送或 Explorer 流取消后自动建立新 transfer，使同一文件可再次粘贴。
- 重新布防按 transfer ID 限定，且不响应 `clipboard replaced` / `replaced by a newer file offer`，避免旧文件覆盖用户更新的剪贴板。
- 增加状态判定、排队晋升/清理、旧完成保护、取消原因和同文件重新布防测试。

## 修改文件

- `crates/m590-daemon/src/hub.rs`：Windows deferred offer 生命周期、发送端排队状态、同文件重新发布和回归测试。
- `crates/m590-clipboard/src/lib.rs`：文件 offer 重新布防接口、平台转发、NullClipboard 行为测试。
- `crates/m590-clipboard/src/linux.rs`、`windows.rs`：重置文件列表/路径文本轮询基线。
- 本 task、`docs/plans/current.md`：实施、验证、风险和后续任务顺序。

## 验证结果

- `cargo test -p m590-clipboard -p m590-daemon`：通过；clipboard 21 项、daemon lib 32 项、daemon bin 1 项，doc tests 无失败。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings`：被本 task 外的既有 `image_file.rs` 文档注释触发 Rust 1.97 `doc_lazy_continuation` 阻塞；未修改该无关文件。
- `cargo clippy -p m590-clipboard --lib --no-deps -- -D warnings -A clippy::doc-lazy-continuation`：通过，除上述既有新 lint 外无警告。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu`：通过。
- `CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings`：通过。
- `rustfmt --edition 2021 --check --config skip_children=true <本 task 的 4 个 Rust 文件>`：通过；避免递归检查本 task 外的既有子模块格式。
- Windows↔Linux Explorer 运行行为：用户按 3 组步骤完成真机验收；活跃传输不被 Linux 后续
  文件 offer 中断、连续更新只保留最新文件，完成或取消后同一文件可再次粘贴，均通过。

## 文档影响检查

- 已更新：本 task、当前计划。
- 无需更新：协议、Hub API、UI 字段、运行/打包命令和产品边界均未变化，因此 `docs/domain/protocol-draft.md`、`docs/discovery/*`、`docs/ui-spec.md`、`项目说明.md` 无需更新。

## 风险 / blocker

- Windows OLE/Explorer 最终行为已由 Windows 10 真机覆盖；当前 Linux 环境仍不能独立复现该平台交互。
- deferred offer 仍受现有单会话总在途文件预留上限约束；本 task 未扩大文件上限或增加并行数据连接。
- 同文件可再次粘贴通过传输结束后自动重新发布当前剪贴板文件实现；`clipboard replaced` 不自动抢回旧文件。

## 下一步

- task-046 已完成；建立独立后续任务，使 Windows Explorer 已开始的远端文件粘贴与 Windows
  本机随后复制其它文件的剪贴板替换解耦。
