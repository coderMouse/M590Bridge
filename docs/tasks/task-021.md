# task-021 · V2 文件通道：hub 落盘 + 可配置目录 + status

## 状态

`completed`

## 目标

在 task-020 协议/会话之上，把文件通道接到 **hub**：

1. 配置项 `file_save_dir`（可持久化 / `POST /api/config`）
2. 对端 `FileOffer` 后自动 `request_file`，收齐后写入保存目录
3. `GET /api/status` 暴露文件相关字段与简单进度
4. `POST /api/send_file`：本机路径读入后 `offer_file`（≤ session 内存上限）

**不做**：UI 页面改版、文件夹递归、断点续传、019A、OS 文件剪贴板写回。

## 允许修改

- `crates/m590-daemon/**`
- `crates/m590-core/**`（仅当需暴露接收进度只读 API，小改）
- `docs/plans/current.md`、`docs/discovery/*`、`docs/domain/protocol-draft.md`（若行为说明变化）
- 本 task

## 禁止修改

- `ui/**` 大改（可不碰）
- 复活 019A
- 无关重构

## 验证命令

```bash
cargo test -p m590-daemon -p m590-core
```

## 实施记录

- `AppConfig.file_save_dir` 默认平台 data dir 下 `m590bridge/inbox`
- `HubStatus` 增加文件 phase / 进度 / 落盘路径字段并写入 `to_json`
- hub：offer 自动 request；Applied 后 `file_save::save_received_file`；`POST /api/send_file`
- core：`Session::inbound_file_progress`
- 未改 UI

## 修改文件

- `crates/m590-daemon/src/{config,status,hub,file_save,lib}.rs`
- `crates/m590-core/src/session.rs`（进度只读）
- `docs/tasks/task-021.md`、`docs/plans/current.md`、discovery 等

## 验证结果

```text
cargo test -p m590-daemon -p m590-core
# m590-core: 18 passed
# m590-daemon lib: 6 passed（config / file_save / status / session+save）
# m590-daemon bin: 1 passed
```

## 文档影响

- 已更新：本 task、plans/current、discovery、AGENTS/项目说明（能力边界）
- 无需更新：ui-spec（无 UI）
- 待补：设置页 file_save_dir 控件（后续 UI task）

## 风险

- 配对后自动接收任意 offer（信任局域网 peer）
- 单文件仍受 4MiB 内存上限
- `send_file` 读本机路径，仅本机 hub 调用场景
- 实机双端联调未在本 task 强制（无 Windows 本机）

## 下一步

UI 展示进度 / 选择发送文件；或剪贴板 file_list 触发 offer。
