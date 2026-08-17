# task-048 · Windows 本机剪贴板替换不中断已开始的 Explorer 粘贴

## 状态

`completed`（Windows↔Linux 真机验收通过）

## 背景

Windows Explorer 已经通过 OLE `IStream` 开始粘贴远端文件时，如果用户在 Windows 本机
`Ctrl+C` 复制其它文件，系统剪贴板序号会变化。当前 OLE 管理线程发出
`ManagerEvent::ClipboardReplaced` 后，Hub 无条件关闭网络桥并向发送端发出 `FileCancel`，导致
已经开始的 Explorer 文件复制中断。

剪贴板中的数据对象与 Explorer 已经取得的 `IStream` 生命周期不同：新复制内容应替换后续粘贴
来源，但不应使已经打开的数据流失效。

## 目标

- Explorer 已打开远端文件流后，Windows 本机复制其它文件不取消当前网络传输，当前粘贴可继续完成。
- Windows 新复制内容继续保留在系统剪贴板；当前远端流完成或取消后，再由现有自动同步轮询处理。
- 本机剪贴板替换时丢弃尚未发布的远端 deferred offer，避免当前流结束后抢回用户的新剪贴板。
- Explorer 尚未请求远端文件时，本机剪贴板替换仍取消该未使用 offer；Explorer 主动取消流的行为不变。
- UI 在保留的流完成前继续显示当前 transfer 的接收进度，不误报 `clipboard replaced` 失败。

## 允许修改

- `crates/m590-daemon/src/hub.rs`：Windows 虚拟文件接收状态、剪贴板替换决策和回归测试。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`：deferred offer 仅在旧 OLE 剪贴板仍为当前内容时条件发布。
- 本 task、`docs/plans/current.md` 与 `AGENTS.md`：任务状态、验证和真机验收入口。

## 禁止修改

- `m590-core` / `m590-net` 协议、消息格式、分片、SHA-256、容量限制和连接模型。
- Windows OLE 虚拟文件格式、Explorer 系统进度 UI 或已验证的按需请求机制。
- 多文件、文件夹、Linux FUSE、断点续传或并行数据连接。
- task-042 Windows 登录自启剩余验收。

## 完成标准

- [x] Explorer 正在粘贴远端文件 A 时，Windows 本机复制文件 B，A 不被中断并可完整落盘。
- [x] 上述场景不发送针对 A 的 `FileCancel("clipboard replaced")`，两端不显示该失败。
- [x] A 完成后 Windows 剪贴板仍是 B；开启自动同步时，B 随后按现有单文件流程同步到对端。
- [x] 本机替换剪贴板时，未请求的当前远端 offer 和 deferred offer 仍会取消，不重新覆盖 B。
- [x] Explorer 主动取消、读取超时和网络失败行为不回归。
- [x] Rust 单测/Clippy 与 Windows GNU 类型检查/Clippy 通过；Windows 运行行为有明确真机步骤。

## 验证命令

```bash
cargo test -p m590-daemon
cargo clippy -p m590-daemon --lib --no-deps -- -D warnings
CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu
CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings
rustfmt --edition 2021 --check --config skip_children=true crates/m590-daemon/src/hub.rs crates/m590-daemon/src/windows_virtual_file_manager.rs
```

Windows↔Linux 真机复测：

1. Linux 复制一个足够观察进度的文件 A，在 Windows Explorer 粘贴；传输中在 Windows 本机复制文件 B，确认 A 继续完成且两端无 `clipboard replaced` 失败。
2. A 完成后在 Windows 另一目录粘贴，确认粘贴的是 B；若两端自动同步开启，再确认 Linux 随后可粘贴 B。
3. Linux 再复制文件 A，但不要在 Windows 粘贴，直接在 Windows 复制 B，确认未请求的 A 被替换且不会重新抢回剪贴板。
4. 再开始一次 A 的粘贴并在 Explorer 取消，确认取消仍会停止网络传输并正确结束进度。

## 实施记录

- Windows 虚拟接收状态新增 `clipboard_replaced`。OLE 管理线程报告系统剪贴板被替换时，Hub
  仅在当前 `IStream` 已请求且网络流尚未完成的情况下保留 bridge，不再调用 producer `fail()`，
  也不再向发送端发出针对当前 transfer 的 `FileCancel("clipboard replaced")`。
- 保留流期间继续消费网络 chunk 并更新当前接收进度；网络流完成后才释放 Hub 对该 receive 的
  控制句柄，使 Windows 本机新复制的内容重新进入现有剪贴板轮询。网络完成但 Explorer 尚未
  读完最后一段缓冲时只释放 offer，不向 producer 注入失败，已有 reader 可继续读完缓冲。
- 本机替换剪贴板仍立即取消 deferred remote offer；未请求或已完成的当前 offer 继续沿用原取消/
  清理行为，Explorer 主动关闭 reader 的 `BridgeEvent::Cancel` 路径未修改。
- deferred offer 的发布从收到 `StreamCompleted` 时立即覆盖，改为统一在 OLE 事件处理后晋升。
  OLE 管理线程新增条件替换命令：只有旧虚拟文件仍是当前系统剪贴板时才发布下一 offer；若用户
  已复制本地文件，则保留本地剪贴板并取消 deferred transfer，覆盖完成瞬间的竞争窗口。
- 增加剪贴板替换决策和“替换后的活动 receive 只能在完成后释放”回归测试。

## 修改文件

- `crates/m590-daemon/src/hub.rs`：活动虚拟流保留/释放、deferred 条件晋升、状态决策测试。
- `crates/m590-daemon/src/windows_virtual_file_manager.rs`：STA 线程 `ReplaceIfCurrent` 条件发布命令。
- `docs/tasks/task-048.md`、`docs/plans/current.md`、`AGENTS.md`：任务边界、状态、验证和真机入口。

## 验证结果

- `cargo test -p m590-daemon`：通过；daemon lib 34 项、daemon bin 1 项，doc tests 无失败。
- `cargo clippy -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu`：通过。
- `CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings`：通过。
- `rustfmt --edition 2021 --check --config skip_children=true crates/m590-daemon/src/hub.rs crates/m590-daemon/src/windows_virtual_file_manager.rs`：通过。
- `git diff --check`：通过。
- 首次 Linux Clippy 尝试在隔离克隆新建构建缓存时被 `/tmp` 磁盘配额阻塞；清理该隔离克隆自身
  可再生成的 `target` 后，复用项目既有构建缓存重新执行并通过，不是源码或 lint 失败。
- Windows Explorer 运行行为：用户后续确认 Windows↔Linux 真机验收通过；该事实已同步在
  `docs/plans/current.md` 与项目入口状态中。

## 文档影响检查

- 已更新：本 task、当前计划和 `AGENTS.md`。
- 无需更新：协议、Hub API、UI 字段、运行/打包命令和产品边界均未变化，因此
  `docs/domain/*`、`docs/discovery/*`、`docs/ui-spec.md`、`项目说明.md` 与 README 无需修改。

## 风险 / blocker

- 当前 Linux 环境不能运行 Windows OLE/Explorer，但用户真机验收已补足该覆盖，不再构成 blocker。
- OLE 条件发布使用 5 秒响应上限；正常路径只在线程内检查剪贴板所有权并发布对象。若 STA 线程
  异常卡住，会返回明确错误而不是无限阻塞会话循环。

## 下一步

- task-048 已完成；后续已转入并完成 task-049。
