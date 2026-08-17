# task-052 · Linux FUSE 单文件网络按需粘贴

## 状态

`completed`（2026-08-17，Linux↔Windows 真机验收通过）

## 背景

task-051 已在 GNOME Wayland + Nautilus 真机证明：Linux 后台进程可以把单文件 FUSE URI
发布到文件剪贴板，Nautilus 在实际粘贴时才读取内容，并显示系统原生复制进度。
当前原型的内容来自本机确定性数据，还没有接入跨机 `FileRequest` / `FileChunk` 通道。

task-044 已为 Windows OLE 虚拟文件实现 `VirtualFileBridge` 有界管道、惰性
`BridgeEvent::Request` 和 `FileCancel` 生命周期。本 task 复用同一协议和桥，不建立 Linux
专用网络状态机，也不先下载到保存目录或永久中间文件。

## 目标

- Linux 收到远端单文件 `FileOffer` 后，发布指向只读 FUSE 文件的 URI，不立即请求内容。
- Nautilus 实际读取 FUSE 文件时才发送一次 `FileRequest`。
- 网络 `FileChunk` 经过现有有界管道直接供 FUSE reader 消费；大小、offset 和 SHA-256
  继续由 `m590-core` 校验。
- 完成、取消、剪贴板替换和断线时唤醒读写端并清理挂载、管道和 Hub 状态。
- 抑制本机发布的 FUSE URI 被剪贴板轮询再次识别为新的本机文件 offer。
- 保留 Nautilus 系统原生复制进度，不在保存目录创建 `.part` 或第二份中间文件。

## 允许修改

- `crates/m590-daemon/src/virtual_file_bridge.rs`：将惰性 reader 工厂复用于 Linux。
- `crates/m590-daemon/src/linux_virtual_file.rs`：网络 reader 所需的小幅通用化。
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`、`src/lib.rs`：Linux 挂载、URI 发布和生命周期管理。
- `crates/m590-daemon/src/hub.rs`：Linux 单文件 offer/request/chunk/complete/cancel 接线与回环抑制。
- `crates/m590-clipboard/src/lib.rs`、`src/linux.rs`：仅当 Hub 无法可靠抑制自发布 URI 时，增加最小剪贴板基线 API。
- 本 task、`docs/plans/current.md`、`AGENTS.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`、`项目说明.md`。
- `.agent/local-environment.md`：仅记录本机真机环境与结果，不提交。

## 禁止修改

- 多文件、文件夹、断点续传、独立数据连接和并行 peer。
- Windows OLE 虚拟文件行为与 task-042 安装/自启代码。
- Tauri UI、Hub HTTP API、协议消息结构和协议版本。
- 自动保存到接收目录的非 Linux/非虚拟文件既有行为。
- Android、macOS 和已取消 task-019A。

## 验证命令

```bash
cargo test -p m590-daemon virtual_file_bridge
cargo test -p m590-daemon linux_virtual_file
cargo test -p m590-daemon linux_virtual_file_manager
cargo test -p m590-daemon
cargo check --workspace
cargo clippy -p m590-daemon --lib --examples --no-deps -- -D warnings
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples
```

Linux↔Windows 真机由用户完成：两端运行 `cd ui && npm run desktop:standalone`；Windows
复制单个普通文件后，Linux 剪贴板应立即可粘贴但未传内容；Nautilus `Ctrl+V` 后才开始
跨机传输并显示系统进度，完成文件内容一致。另测试取消系统复制、粘贴前替换剪贴板、
传输中替换剪贴板、断开连接和同一文件再次复制。

## 完成标准

- [x] Linux 收到 offer 后只发布 FUSE URI，首次读取前不发 `FileRequest`、不落中间文件。
- [x] 首次 FUSE 读取只产生一次请求，网络分片受有界管道背压并由 core 完成完整性校验。
- [x] 成功、取消、替换和断线均无挂死；挂载与活动状态可回收。
- [x] 自发布 FUSE URI 不回环成新的本机 `FileOffer`，同一远端文件后续可再次复制/粘贴。
- [x] Linux 测试、workspace check、严格 Clippy 和 Windows 交叉检查通过。
- [x] GNOME Wayland + Nautilus ↔ Windows 真机按需、系统进度、内容和取消验收通过。

## 实施记录

- 2026-08-14：task-051 真机验收后建立本任务；限定为单文件 Linux FUSE 与既有网络桥接，
  不扩展协议、多文件、文件夹或 Windows 行为。
- 2026-08-14：扩展 `VirtualFileBridge` 提供 Linux reader 工厂，并增加消费者已读完、
  FUSE 句柄释放和提前关闭取消事件；网络完成不会单独触发卸载。
- 2026-08-14：新增 Linux manager，创建唯一临时挂载目录、发布文件 URI、条件替换当前
  offer，并在 manager 清理时卸载和删除空目录。
- 2026-08-14：Hub Linux 分支收到远端 offer 后仅发布 FUSE URI；首次 FUSE read 才调用
  `request_file_stream`，`FileChunk` 经有界管道进入 FUSE，失败/取消/剪贴板替换/延迟
  offer/断线路径复用 `FileCancel` 和生命周期清理。
- 2026-08-17：用户确认 Linux↔Windows 真机测试通过，task-052 完成。

## 修改文件

- `docs/tasks/task-052.md`：定义目标、边界、验证和真机验收步骤。
- `docs/plans/current.md`：将唯一下一步切换为 task-052 进行中。
- `crates/m590-daemon/src/virtual_file_bridge.rs`：Linux reader 工厂、消费者完成/释放
  事件和提前关闭取消。
- `crates/m590-daemon/src/linux_virtual_file.rs`：网络 reader release 回调、唯一 FUSE
  文件句柄和可并行处理 release 的 FUSE worker 配置。
- `crates/m590-daemon/src/linux_virtual_file_manager.rs`：Linux FUSE 挂载与剪贴板 URI
  manager。
- `crates/m590-daemon/src/lib.rs`：导出 Linux manager 模块。
- `crates/m590-daemon/src/hub.rs`：Linux 远端 offer/request/chunk/complete/cancel 接线、
  回环抑制、状态和延迟 offer 管理；Windows `Consumed`/`Released` 事件只做兼容忽略。
- `AGENTS.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、
  `项目说明.md`：同步 task-052 开发状态、命令、模块和能力边界。

## 验证结果

- `cargo test -p m590-daemon virtual_file_bridge`：通过，5 passed。
- `cargo test -p m590-daemon linux_virtual_file`：通过，6 passed（包含 Linux manager 过滤匹配的测试）。
- `cargo test -p m590-daemon linux_virtual_file_manager`：通过，2 passed。
- `cargo test -p m590-daemon`：通过，daemon library 50 passed、binary 1 passed，0 failed。
- `cargo check -p m590-daemon --lib`：通过。
- `cargo check --workspace`：通过。
- `cargo clippy -p m590-daemon --lib --examples --no-deps -- -D warnings`：通过。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：
  通过；Linux-only manager/FUSE 未进入 Windows target。
- GNOME Wayland + Nautilus ↔ Windows 真机：用户于 2026-08-17 确认测试通过。

## 文档影响检查

- 已更新：`docs/plans/current.md`、`AGENTS.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`、`项目说明.md`；真机结果同步到本 task、当前计划、
  命令文档、`AGENTS.md` 与`项目说明.md`。
- 无需更新：协议草案、`m590-core`、`m590-net`、UI/Tauri API 和 Windows OLE 行为未改变。

## 风险 / blocker

- 网络流只支持顺序读取；如 Nautilus 对同一文件发出无法满足的回退 seek，本 task 需记录
  真实 offset 序列并在单文件边界内处理，不能假定与 task-051 本地可 seek 源完全一致。
- 当前执行沙箱仍没有 `/dev/fuse`，但用户真机验收已补足该覆盖，不再构成 task blocker。

## 下一步

- task-052 已完成；转入 task-053，修复 Linux 托盘菜单文字回归。
