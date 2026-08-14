# task-051 · Linux FUSE 单文件按需粘贴原型

## 状态

`completed`（2026-08-14：GNOME Wayland + Nautilus 真机验收通过）

## 背景

task-050 已在当前 GNOME Wayland + Nautilus 真机确认：后台进程通过 `arboard`
的 X11 fallback 发布 `text/uri-list` 后，Nautilus 可以粘贴本地文件 URI，且文件内容一致。
因此 Linux 按需文件粘贴剩余的首个技术风险，是能否把 URI 指向单文件 FUSE 挂载，
并让 Nautilus 只在实际粘贴读取时打开内容源和显示系统复制进度。

本 task 对应 Windows task-043，只做本机、确定性内容的独立原型。网络
`FileRequest` / `FileChunk` 桥接必须另建后续 task，避免把原型误报为跨机功能。

## 目标

- 提供 Linux-only 的只读单文件 FUSE 挂载边界，文件名和大小在挂载前已知。
- 元数据查询、URI 发布和剪贴板自读不得打开内容源；首次 FUSE `read` 才调用内容工厂。
- 独立 example 生成可调大小、可调延迟的确定性内容，不创建永久源文件或中间文件。
- 将 FUSE 虚拟文件 URI 发布到 Linux 文件剪贴板，在 Nautilus `Ctrl+V` 时观察内容打开、
  系统复制进度和最终文件内容。
- FUSE 依赖只在 Linux 目标启用，并使用不依赖系统 `fuse3.pc` 的构建方式。

## 实现选择

- 使用 Linux-only `fuser` 纯 Rust 挂载路径；不链接缺失的 `libfuse` / `fuse3.pc`。
- 挂载只暴露根目录和一个普通文件，使用只读、`nodev`、`nosuid`、`noexec` 选项。
- 内容工厂在首次 `read` 时打开一次；后续按 FUSE offset 执行 `Seek + Read`。
- 文件以 direct I/O 打开，避免页缓存掩盖按需读取时机，并用单 FUSE 工作线程保持读取顺序可观察。
- example 使用和 Windows 原型一致的 `offset % 251` 模式数据，可选校验粘贴结果。

## 允许修改

- `crates/m590-daemon/Cargo.toml`、`Cargo.lock`：增加 Linux-only `fuser` 依赖。
- `crates/m590-daemon/src/linux_virtual_file.rs`、`src/lib.rs`：单文件只读 FUSE 原型边界。
- `crates/m590-daemon/examples/linux_virtual_file.rs`：Linux/Nautilus 真机原型入口。
- 本 task、`docs/plans/current.md`、`AGENTS.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`、`项目说明.md`。
- `.agent/local-environment.md`：仅记录本机 FUSE 环境和真机结果，不提交。

## 禁止修改

- `m590-core`、`m590-net`、Hub/Session 文件状态机和当前 Linux 自动下载行为。
- 网络 `FileRequest` / `FileChunk` / `FileCancel` 接入和 UI 状态。
- Windows OLE 虚拟文件、task-042 安装/自启代码。
- 多文件、文件夹、断点续传、并行数据连接、GVfs/Nautilus 扩展。

## 验证命令

```bash
cargo test -p m590-daemon linux_virtual_file
cargo check -p m590-daemon --examples
cargo clippy -p m590-daemon --lib --examples --no-deps -- -D warnings
cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples
```

Linux GNOME Wayland 真机：

1. 确认 `/dev/fuse` 存在且当前用户可使用，退出正在运行的 M590Bridge。
2. 准备一个空的 Nautilus 粘贴目标目录，然后运行：

   ```bash
   mkdir -p ~/M590Bridge-paste-test
   cargo run -p m590-daemon --example linux_virtual_file -- 67108864 4 ~/M590Bridge-paste-test/M590Bridge-virtual.bin
   ```

3. example 报告 `virtual_file_ready` 后先等待数秒，确认尚无 `content_opened`。
4. 在目标目录按 `Ctrl+V`，确认此时才出现 `content_opened` / `content_first_read`，并观察
   Nautilus 系统复制进度。
5. 复制完成后回到终端按 Enter；example 校验文件大小与模式内容并报告
   `pasted_file_verified=true`，然后卸载并清理空挂载目录。

## 完成标准

- [x] Linux 单文件 FUSE 模块和确定性 example 完成，Linux 测试/检查/Clippy 通过。
- [x] Windows target 检查不引入 FUSE 依赖或 Linux API。
- [x] 单元测试证明构造、元数据路径和读取前不会调用内容工厂，首次读取只打开一次。
- [x] 单元测试覆盖 offset 读取、EOF、非法文件名和短源错误。
- [x] GNOME Wayland + Nautilus 真机确认粘贴前不打开、粘贴时读取、系统进度和内容一致。
- [x] 未产生永久源文件/中间文件，未改变网络与 Hub 行为。

## 实施记录

- 2026-08-14：建立任务并完成环境预检。`fusermount3` 可用，但当前执行环境没有
  `/dev/fuse`，所以代码验证可继续，挂载与 Nautilus 验收暂记环境 blocker。
- 2026-08-14：新增 Linux-only `LinuxVirtualFile` 和 `LinuxVirtualFileMount`，使用 `fuser`
  纯 Rust 挂载路径，仅暴露根目录和一个只读普通文件；`getattr/open` 不调用内容工厂，
  首次 `read` 才打开一次并按 offset `Seek + Read`。
- 2026-08-14：新增 `linux_virtual_file` example，生成可调大小/延迟的 `offset % 251`
  模式数据，发布 FUSE 文件 URI，并可在退出时校验粘贴文件内容。
- 2026-08-14：首次以 `/tmp` 作为 Nautilus 粘贴目标时，目标文件系统返回
  `Disk quota exceeded`；改用用户主目录并以 64 MiB / 4 ms 重测后复制完成。
- 2026-08-14：真机日志确认 URI 发布后没有提前打开内容；Nautilus 粘贴时依次出现
  `content_opened` 和 `content_first_read offset=0`，系统显示原生复制进度框，最终
  `pasted_file_verified=true`，task-051 验收完成。

## 修改文件

- `docs/tasks/task-051.md`：定义本机 FUSE 单文件原型边界与真机步骤。
- `docs/plans/current.md`：登记 task-051 完成状态、能力边界和唯一下一步。
- `crates/m590-daemon/Cargo.toml`、`Cargo.lock`：增加 Linux-only `fuser 0.18`（关闭默认特性）。
- `crates/m590-daemon/src/lib.rs`：导出 Linux-only FUSE 模块。
- `crates/m590-daemon/src/linux_virtual_file.rs`：单文件只读 FUSE 文件系统、挂载句柄和惰性读取测试。
- `crates/m590-daemon/examples/linux_virtual_file.rs`：发布 URI、等待 Nautilus 粘贴、模式内容校验探针。
- `AGENTS.md`、`docs/discovery/commands.md`、`docs/discovery/project-map.md`、`项目说明.md`：
  登记 task-051 当前边界、命令、模块和验收状态。

## 验证结果

- `fusermount3 --version`：通过，输出 `fusermount3 version: 3.18.2`。
- `pkg-config --modversion fuse3`：失败，本机没有 `fuse3.pc`；实现选择纯 Rust 挂载路径，
  不以系统开发包为构建前置。
- `stat /dev/fuse`：Codex 执行沙箱失败，沙箱不存在 `/dev/fuse`；后续用户真机已成功挂载，
  该沙箱结果不代表产品环境。
- `cargo test -p m590-daemon linux_virtual_file`：通过，3 passed、0 failed。
- `cargo check -p m590-daemon --examples`：通过。
- `cargo clippy -p m590-daemon --lib --examples --no-deps -- -D warnings`：通过。
- `cargo check -p m590-daemon --target x86_64-pc-windows-gnu --examples`：通过，Linux-only
  `fuser` 未进入 Windows target。
- `rustfmt --edition 2021 --check crates/m590-daemon/src/linux_virtual_file.rs
  crates/m590-daemon/examples/linux_virtual_file.rs`：通过。
- `cargo test -p m590-daemon`：新增测试及多数既有测试通过；5 个既有文件/配置临时目录测试
  因执行环境 `/tmp` 配额返回 `Disk quota exceeded (os error 122)`，与 task-051 无关。
- `cargo run -p m590-daemon --example linux_virtual_file -- 1024 0`：Codex 沙箱按预期失败于
  挂载阶段，输出 `cannot mount FUSE virtual file: No such file (os error 2)`；用户真机随后通过。
- GNOME Wayland + Nautilus 真机：用户运行 64 MiB / 4 ms 探针；粘贴前只有
  `virtual_file_ready`，粘贴后出现 `content_opened`、`content_first_read offset=0` 和系统
  原生复制进度框，完成后输出 `pasted_file_verified=true`。惰性读取、系统进度、文件大小和
  模式内容均验收通过。

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`AGENTS.md`、`docs/discovery/commands.md`、
  `docs/discovery/project-map.md`、`项目说明.md`。
- 无需更新：协议、Hub API、UI、Windows OLE 和现有网络状态机均未修改。

## 风险 / blocker

- Codex 执行沙箱缺少 `/dev/fuse`，但用户 GNOME Wayland 真机已成功挂载并完成粘贴，
  因此该环境差异不再是产品 blocker。
- 当前机器的 `/tmp` 有独立配额限制，不适合作为大文件粘贴目标；真机步骤已改用用户主目录。
- 文件管理器或桌面索引器可能在用户按粘贴前预读 URI；原型会通过日志记录真实首次读取时机，
  不预设一定只由键盘 `Ctrl+V` 触发。
- 网络流通常不能任意回退 seek；本 task 使用可 seek 的本地模式源验证 FUSE/Nautilus 行为，
  后续网络桥接必须根据真机 offset 序列单独设计。

## 下一步

- task-051 已完成；下一步另建 task-052，设计 Linux FUSE 内容工厂与现有网络
  `FileRequest` / 有界流 / `FileCancel` 的桥接，不在本 task 扩大范围。
