# task-050 · GNOME Wayland 单文件 URI 剪贴板可行性验证

## 状态

`completed`

## 背景

Windows 已通过 OLE `IDataObject/IStream` 实现单文件 Explorer 按需粘贴；Linux 当前收到
`FileOffer` 后仍会立即发送 `FileRequest`，下载到 `.part` 并移动到接收目录，没有把远端文件
发布为 Linux 文件剪贴板对象。

Linux 文件管理器通常从 `text/uri-list` 和 `x-special/gnome-copied-files` 读取本地文件 URI。
后续若要做到“未粘贴不传输、Nautilus `Ctrl+V` 后才传输”，计划让剪贴板 URI 指向 FUSE
虚拟文件，由文件管理器首次读取触发网络请求。不过 task-026 已确认当前 GNOME Wayland
compositor 没有 `ext-data-control` / `wlr-data-control`，现有 `arboard` 可能回退 X11，因此必须
先验证后台进程发布的文件 URI 能否被 Wayland Nautilus 实际粘贴。

## 目标

- 提供一个 Linux 独立探针，命令行接收任意本地单文件路径并持续持有文件剪贴板所有权；
  不再使用仓库现有 example 中的个人硬编码路径。
- 明确输出实际剪贴板后端和发布结果，非法路径、目录及发布失败必须给出可诊断错误。
- 在当前 GNOME Wayland 真机上验证 Nautilus 能否将该 URI 粘贴到另一个目录，并校验文件内容。
- 根据真机结果形成明确结论：可以进入 FUSE 单文件原型，或必须先解决 Wayland 剪贴板发布。

## 允许修改

- `crates/m590-clipboard/examples/`：新增或替换 Linux 文件剪贴板发布探针。
- `crates/m590-clipboard/src/linux.rs`、`src/lib.rs`、`src/error.rs`：仅限探针所需的最小文件列表
  发布 API 与错误处理。
- `crates/m590-clipboard/Cargo.toml`、`Cargo.lock`：仅限现有 Linux 剪贴板依赖或 example 配置。
- `docs/tasks/task-050.md`、`docs/plans/current.md`、`AGENTS.md` 及必要的命令/项目结构文档。
- `.agent/local-environment.md`：仅记录本机 Wayland/X11、桌面环境和探针结果，不提交。

## 禁止修改

- `m590-core`、`m590-net`、Hub/Session 文件状态机和当前 Linux 自动下载行为。
- Linux FUSE、GVfs backend、Nautilus 扩展或 GNOME Shell 扩展。
- Windows OLE 虚拟文件、task-042 安装/自启代码。
- 多文件、文件夹、断点续传、并行数据连接和 UI 功能。

## 完成标准

- [x] 探针接受一个本地普通文件路径，发布文件剪贴板并持续运行到用户主动退出。
- [x] 探针不包含个人绝对路径；空路径、目录和不存在路径均明确失败。
- [x] Linux 测试、检查和限定范围 Clippy 通过，Windows GNU 目标不因 Linux-only example 回归。
- [x] GNOME Wayland + Nautilus 真机步骤已执行并记录真实结果，而非仅验证进程自读剪贴板。
- [x] Nautilus 接受 `x11-fallback` 发布的 `text/uri-list`，粘贴后文件内容校验一致。
- [x] 可行性结论已明确：下一任务可进入 Linux FUSE 单文件原型，暂不扩展多文件/文件夹。

## 验证命令

```bash
cargo test -p m590-clipboard
cargo check -p m590-clipboard --examples
cargo clippy -p m590-clipboard --lib --examples --no-deps -- -D warnings
cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples
```

GNOME Wayland 真机：

1. 创建一个内容可校验的本地小文件，并准备另一个空目录作为 Nautilus 粘贴目标。
2. 先退出正在运行的 M590Bridge，再运行下面的探针并传入该文件路径，保持进程运行：

   ```bash
   cargo run -p m590-clipboard --example set_file_and_read -- /path/to/test-file
   ```

3. 切换到 Nautilus 目标目录，确认“粘贴”可用并按 `Ctrl+V`。
4. 校验目标文件名、大小和内容；记录探针报告的后端及粘贴是否成功。
5. 退出探针，确认没有修改 Hub 的接收目录或产生网络传输。

## 实施记录

- 2026-08-14：建立任务，先验证 GNOME Wayland 文件 URI 发布链路；尚未修改业务代码。
- 在 `ClipboardService` 增加默认不支持的 `write_file_list`；仅 Linux
  `PlatformClipboard` 转发到 `arboard.set().file_list(...)`，Windows OLE 路径不变。
- 将旧 `set_file_and_read` example 改为参数化 Linux 探针：解析并校验单一普通文件，
  发布 `text/uri-list`，立即自读校验，然后持有剪贴板到用户按 Enter 退出。
- 探针分别输出会话后端、data-control 可用性和 `arboard` 发布后端；本机实测为
  `session_backend=Wayland` / `data_control_available=false` / `publisher_backend=x11-fallback`。
- 用户在 GNOME Wayland Nautilus 目标目录执行 `Ctrl+V`，成功生成 `Cargo.toml`；
  `cmp` 校验输出“内容一致”，证明 Mutter 已将 X11 剪贴板 URI 交给 Wayland Nautilus。

## 修改文件

- `docs/tasks/task-050.md`：定义可行性验证范围、命令和真机步骤。
- `docs/plans/current.md`、`AGENTS.md`：将 task-050 登记为唯一下一任务。
- `crates/m590-clipboard/src/lib.rs`：新增平台文件列表写入边界、Linux-only
  转发和 Null 回归测试。
- `crates/m590-clipboard/src/linux.rs`：用 `arboard` 发布本地文件 URI 并同步 poll 基线。
- `crates/m590-clipboard/examples/set_file_and_read.rs`：参数化 Linux/Nautilus 真机探针。
- `docs/discovery/commands.md`、`docs/discovery/project-map.md`：登记探针命令与文件职责。
- `项目说明.md`：记录 Linux URI 剪贴板入口真机可行，并明确 FUSE 仍未实现。
- `.agent/local-environment.md`：记录本机发布后端和 Nautilus 真机验收结果（gitignore）。

## 验证结果

- `cargo test -p m590-clipboard`：通过，21 passed、0 failed。
- `cargo check -p m590-clipboard --examples`：通过。
- `cargo clippy -p m590-clipboard --lib --examples --no-deps -- -D warnings`：未通过；被本 task
  范围外、task-043 已记录的 `src/image_file.rs` `clippy::doc_lazy_continuation` 拦截。
- 上述 Clippy 命令加 `-A clippy::doc_lazy_continuation`：通过，本 task 库和 examples
  无新增告警。
- `CARGO_HOME=/tmp/<shared-cache> cargo check -p m590-clipboard --target x86_64-pc-windows-gnu --examples`：
  通过；主 Cargo cache 只读，新空临时 cache 受环境 quota 限制，改用已有可写共享缓存完成检查。
- `cargo run -p m590-clipboard --example set_file_and_read -- Cargo.toml </dev/null`：通过；
  本机输出 `Wayland` + `data_control_available=false` + `x11-fallback`，
  `published_mime=text/uri-list`、`readback_matches=true`。
- 直接运行已构建探针的空参数、目录和不存在路径：均以 exit 2 返回明确错误。
- GNOME Wayland Nautilus `Ctrl+V`：用户真机验收通过；探针输出
  `session_backend=Wayland`、`data_control_available=false`、`publisher_backend=x11-fallback`、
  `published_mime=text/uri-list`、`readback_matches=true`。
- `cmp ../Cargo.toml /tmp/m590bridge-paste-test/Cargo.toml && echo "内容一致"`：用户实机输出
  `内容一致`，文件名、粘贴链路和内容校验通过。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`AGENTS.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`、`项目说明.md`。
- 无需更新：协议、Hub API、UI 和产品能力均未改变。

## 风险 / blocker

- 当前 GNOME Wayland 没有 data-control 协议，但实机确认 Mutter 能将 X11 侧
  `text/uri-list` 交给 Wayland Nautilus；其它 compositor/文件管理器尚未验证。
- Nautilus 在本次环境不要求同时发布 `x-special/gnome-copied-files`；后续不应无故增加
  GTK/GDK 剪贴板依赖。
- 本 task 只证明 Linux 文件 URI 剪贴板入口可行；Hub 仍会立即下载，FUSE 按需读取尚未实现。

## 下一步

- task-050 已完成；后续先建立新 task，再实现 Linux FUSE 单文件按需粘贴原型。
