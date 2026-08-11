# task-036 · 桌面文件传输吞吐调优（第一刀）

## 状态

`completed`

## 目标

针对 Linux↔Windows 桌面端实测文件传输仅约 3–5 MB/s、同网络 LocalSend 约 10 MB/s 的问题，移除现有单连接发送循环中与网络状态无关的固定 50 ms 批次停顿，并减少路径流式读取的重复系统调用。

本 task 保留现有协议、256 KiB 分片、每轮 4 分片和单 TCP 连接，只做低风险的第一轮吞吐调优。

## 背景

- `Session` 每次 pump 最多产生 `4 × 256 KiB = 1 MiB` 文件数据。
- 桌面 Hub 每轮无条件休眠 50 ms；即使网络可持续写入，也会在每个约 1 MiB 批次后主动停顿。
- 路径发送已持有打开的 `File`，但每个分片仍按当前 offset 重复 `seek` 后再读取。
- task-033 的 100 MiB 测试只覆盖同进程 Session 数据路径，不包含桌面 Hub 的固定休眠和真实 LAN。

## 允许修改

- `crates/m590-core/src/session.rs`：暴露文件传输活跃状态、路径文件改为连续读取，并补充回归测试。
- `crates/m590-net/src/tcp.rs`：修复高吞吐下多帧累计缓冲被误判为单帧超限，并补充 TCP 回归测试。
- `crates/m590-daemon/src/hub.rs`：桌面 Hub 连接循环改为工作感知调度，并补充调度单测。
- `docs/plans/current.md`、本 task：同步计划、实施和验证记录。

## 禁止修改

- 独立文件数据连接、协议字段/版本、chunk 大小或多文件并行。
- 文件夹、断点续传、OS 文件剪贴板、配对/mDNS 行为。
- UI 布局、文案、Hub API、鉴权和落盘语义。
- Linux/Windows 安装、自启、更新或签名。
- 为基准成绩降低/移除 SHA-256 校验。

## 实现要求

1. 桌面 Hub 空闲时仍以 50 ms 低频轮询，避免空闲忙等。
2. 文件批次确实发送或接收时只让出线程，不再固定睡眠 50 ms。
3. 文件仍活跃但本轮无数据时使用短休眠，避免断流期间占满 CPU。
4. 发送/接收仍每轮回到心跳、入站消息和剪贴板处理，不能用无限文件发送循环饿死控制消息。
5. 路径文件使用已打开句柄连续读取，不再为每个分片重复定位。
6. `MAX_PAYLOAD_LEN` 仍约束单帧；非阻塞读取必须在每批 socket 数据后尝试解帧，不能用多帧累计缓冲长度冒充单帧 payload 长度。

## 验证命令

```bash
cargo test -p m590-core -p m590-net -p m590-daemon --lib
cargo test --release -p m590-core file_path_streams_100mib_with_sha256 -- --nocapture
cargo build -p m590-ui
cargo clippy -p m590-core -p m590-net -p m590-daemon --lib --no-deps -- -D warnings

# 本机双 Hub/TCP 大文件 smoke：记录文件大小、耗时、吞吐、SHA-256/落盘结果。
# Linux↔Windows 相同文件、相同网络复测由实机完成，记录 blocker，不用本机结果冒充 LAN 结果。
```

## 完成标准

- [x] 文件活跃时不再每约 1 MiB 固定休眠 50 ms。
- [x] 空闲与暂时无数据场景不会持续忙等。
- [x] 路径流式发送保持分片、进度和 SHA-256 正确。
- [x] 连续到达且累计超过 16 MiB 的多个合法帧不会被误判为单帧超限。
- [x] core/net/daemon 单测、桌面构建和范围内 Clippy 通过。
- [x] 记录本机性能 smoke；明确 Linux↔Windows 实机复测状态（2026-08-11 用户确认两边可复制文件）。

## 实施记录

- `Session::has_active_file_transfer` 同时覆盖正在发送和已经开始落盘的接收流，供桌面 Hub 判断调度状态。
- 路径发送继续复用打开的 `File`，按文件游标连续 `read_exact`；去掉每个 256 KiB 分片前重复的 `seek`。
- 桌面 Hub 每轮仍最多发送 4 个分片（约 1 MiB），随后返回心跳、入站消息和剪贴板处理：本轮有文件进展时 `yield_now`，文件仍活跃但本轮无数据时休眠 1 ms，完全空闲时才休眠 50 ms。
- 首次取消固定节流后的双 Hub smoke 暴露 TCP 接收错误：`try_recv` 会持续读取多帧后才解第一帧，累计缓冲超过 16 MiB 时被误判为单帧超限。
- TCP 非阻塞接收改为使用 64 KiB 复用读缓冲，每读一批立即尝试解出一帧；`MAX_PAYLOAD_LEN = 16 MiB` 的单帧限制保持不变，累计缓冲只允许一个最大帧加一个读批次。
- 新增连续发送约 18 MiB、由多个 256 KiB 合法帧组成的 TCP 回归，锁定高吞吐下不再误报 `payload too large`。

## 修改文件

- `crates/m590-core/src/session.rs`：文件活跃状态、路径顺序读取和发送/接收状态回归。
- `crates/m590-net/src/tcp.rs`：分批读取后立即解帧、复用 64 KiB 读缓冲和累计多帧回归。
- `crates/m590-daemon/src/hub.rs`：工作感知的桌面连接循环调度及单测。
- `docs/plans/current.md`、本 task：任务优先级、完成状态、验证结果和后续顺序。

## 验证结果

- `cargo test -p m590-core -p m590-net -p m590-daemon --lib`：通过；core 22、net 15、daemon 21，均 0 failed。
- `cargo test -p m590-net tcp_nonblocking_decodes_many_frames_exceeding_single_frame_limit -- --nocapture`：通过；约 18 MiB 连续多帧全部解码。
- `cargo test --release -p m590-core file_path_streams_100mib_with_sha256 -- --nocapture`：通过；100 MiB 用时 0.223 s，约 447.6 MiB/s，SHA-256 正确（同进程 release 数据路径，非 LAN）。
- `cargo build -p m590-ui`：通过；桌面端 debug 构建成功。
- `cargo build --release -p m590-daemon`：通过；用于双 Hub smoke 的优化构建成功。
- `cargo clippy -p m590-core -p m590-net -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- 本机双 Hub/TCP 512 MiB：从 `/api/send_file` 到接收状态 `done` 用时约 1.261 s，约 406.0 MiB/s；接收 536870912 bytes，双方保持 `connected`、`last_error=null`。
- `sha256sum`：源文件与最终落盘文件均为 `9acca8e8c22201155389f65abbf6bc9723edc7384ead80503839f49dcc56d767`；接收目录只有最终文件，无遗留 `.part`。
- `git diff --check -- crates/m590-core/src/session.rs crates/m590-net/src/tcp.rs crates/m590-daemon/src/hub.rs docs/plans/current.md docs/tasks/task-036.md`：通过。
- 2026-08-11 用户实机复测：Linux↔Windows 两边可复制文件，功能通过。未提供具体吞吐数字；本记录只确认跨机文件通道可用，不冒充 LocalSend 对比结果。

## 文档影响检查

- 已更新：本 task 与 `docs/plans/current.md`。
- 无需更新：`docs/domain/protocol-draft.md`，本 task 未改变协议字段、版本、单帧 16 MiB 上限或 256 KiB 文件分片。
- 无需更新：`docs/ui-spec.md`、`项目说明.md`、discovery 文档；未改变 UI、产品能力边界、API、命令或模块职责。

## 风险 / blocker

- 2026-08-11 用户已确认 Linux↔Windows 两边可复制文件；跨机功能 blocker 关闭。
- 用户未提供同文件吞吐数字，因此不能据此宣称已超过 LocalSend 或达到本机回环水平。
- 全文件 `rustfmt --check` 仍会报告 `session.rs`、`hub.rs`、`tcp.rs` 中 task-036 前已有的格式差异；本 task 未做全文件格式化，以免混入无关改动，本次新增行已按 rustfmt 输出修正。

## 下一步

- 跨机文件复测已通过；后续若仍关心吞吐对比，另开任务记录同文件/同网络 MB/s。
- 产品下一优先：Windows 安装包 / 开机自启。
