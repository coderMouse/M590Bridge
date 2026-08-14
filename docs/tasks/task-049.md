# task-049 · 配对总超时与断开后单次重连

## 状态

`in_progress`

## 背景

实机发现两个连接生命周期问题：

- UI 显示“正在配对（约 30 秒超时）”，但当前 30 秒计时只在 TCP 已建立后进入协议握手时启动。
  TCP 建连失败或握手前断开会被 `auto_reconnect` 反复重试，每次重新计时，因此用户等待超过
  30 秒仍看不到超时结果。
- `POST /api/disconnect` 只设置 `STOP_BRIDGE` 并立即返回。旧 worker 尚未退出、
  `BRIDGE_RUNNING` 仍为 true 时，UI 已允许下一次连接，导致
  `bridge already running; disconnect first`；反复点击只是等待旧线程异步清理。

## 目标

- 加入方初次连接从点击开始最多约 30 秒，覆盖 TCP 建连、自动重试和配对握手；到期进入可见
  `error` 并释放 bridge worker。
- 创建方在没有对端时继续无限等待；TCP 对端连入后的配对握手仍限制 30 秒。
- 曾经成功连接后的意外断线继续按现有配置自动重连，不被初次配对总期限截断。
- “断开 / 重置”仅在旧 worker 确认停止后返回；随后第一次点击“连接对端”即可启动新 worker。
- worker 无法在限定时间停止时返回明确错误，不伪装为已断开。

## 允许修改

- `crates/m590-daemon/src/hub.rs`：配对期限、可中断 TCP 建连、bridge 生命周期同步和回归测试。
- `crates/m590-net/src/tcp.rs`、`lib.rs`：增加带明确时间上限的 framed TCP 拨号入口。
- `ui/src/app/OperableApp.tsx`：澄清初次加入的 30 秒提示（如实现需要）。
- 本 task、`docs/plans/current.md`、`AGENTS.md`：任务状态、验证和真机步骤。

## 禁止修改

- 配对协议消息、配对码格式、自动重连开关和已连接后的退避序列。
- mDNS 发现、文件传输、剪贴板、安装/自启逻辑。
- task-042 Windows 登录自启剩余验收。
- 多设备、云中继或远程键鼠控制。

## 完成标准

- [ ] 加入一个无法完成配对的地址并保持自动重连开启，约 30 秒后 UI 显示超时错误，不再持续配对。
- [ ] 超时后无需先断开，第一次点击“连接对端”即可启动新尝试。
- [ ] 配对中点击“断开 / 重置”，接口返回后第一次点击“连接对端”不再出现 bridge already running。
- [ ] 创建方无对端时可持续等待；对端连入但不完成握手时约 30 秒报错。
- [x] 已连接后的意外断线仍按 1/2/4/8/16/30 秒退避自动重连。
- [x] daemon 单测/Clippy、前端 lint/build 与 Windows GNU 类型检查/Clippy 通过。

## 验证命令

```bash
cargo test -p m590-net -p m590-daemon
cargo clippy -p m590-net -p m590-daemon --lib --no-deps -- -D warnings
cd ui && npm run lint
cd ui && npm run build
CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu
CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings
rustfmt --edition 2021 --check --config skip_children=true crates/m590-daemon/src/hub.rs
```

Linux/Windows 真机复测：

1. 加入方填写未监听或无法完成握手的对端，开启自动重连后点击连接；确认约 30 秒进入超时错误，
   然后直接再次点击连接，确认一次成功启动新尝试。
2. 配对进行中点击“断开 / 重置”，等待按钮恢复后立即点击“连接对端”，确认不出现
   `bridge already running; disconnect first`。
3. 创建方点击“开始等待配对”并等待超过 30 秒，确认仍保持等待；再让测试客户端建立 TCP 但不
   完成握手，确认该连接约 30 秒结束并显示错误。
4. 完成正常配对后短暂关闭对端再恢复，确认自动重连退避行为保持。

## 实施记录

- 加入方在 `run_with_reconnect` 创建一次 30 秒总 deadline，限时覆盖 TCP 拨号、自动重连退避和握手；创建方只在 accept 后为单次握手创建 30 秒期限。
- 新增 `connect_framed_timeout`，按剩余总期限限制单次 TCP 拨号；曾成功连接过的 worker 每次重连重新获得单次握手期限，握手超时可继续自动重连。
- 增加 bridge 启停过渡保护和停止等待：`/api/disconnect` 等待 worker 的 running/stopping 标志清除后才返回；worker 发布终态前先释放 running 占用，避免错误态与下一次启动竞态。
- 修正退避上限实现与既定序列一致：`1/2/4/8/16/30/30...`。
- 增加 TCP 限时拨号、初次总期限、停止等待、首次重连和退避决策回归测试。

## 修改文件

- `crates/m590-daemon/src/hub.rs`：配对总期限、重连生命周期互斥/等待、终态发布及回归测试。
- `crates/m590-net/src/tcp.rs`：新增限时 framed TCP 拨号及测试。
- `crates/m590-net/src/lib.rs`：导出 `connect_framed_timeout`。
- `AGENTS.md`、`docs/plans/current.md`：更新当前阶段与下一步。
- `docs/tasks/task-049.md`：记录实施与验证。
- `ui/src/app/OperableApp.tsx`：无需修改，现有“约 30 秒超时”文案与后端行为一致。

## 验证结果

- `cargo test -p m590-net -p m590-daemon`：通过；daemon 40 个测试、net 19 个测试全部通过。
- `cargo clippy -p m590-net -p m590-daemon --lib --no-deps -- -D warnings`：通过。
- `cd ui && npm run lint`：通过（oxlint）。
- `cd ui && npm run build`：通过（TypeScript + Vite）。
- `CARGO_HOME=<临时可写缓存> cargo check -p m590-daemon --target x86_64-pc-windows-gnu`：通过。
- `CARGO_HOME=<临时可写缓存> cargo clippy -p m590-daemon --target x86_64-pc-windows-gnu --lib --no-deps -- -D warnings`：通过。
- `rustfmt --edition 2021 --check --config skip_children=true crates/m590-daemon/src/hub.rs`：通过。
- 备注：首次使用临时构建目录链接测试二进制时遇到环境 `ld ... Bus error`，切换到可写共享构建缓存后同一测试命令完整通过；非代码失败。

## 文档影响检查

- 已更新：`AGENTS.md`、`docs/plans/current.md`、本 task 的实施/验证/下一步记录。
- 无需更新：`ui/src/app/OperableApp.tsx`，现有初次配对 30 秒提示已准确；协议、文件传输、剪贴板、安装/自启文档均未改变。

## 风险 / blocker

- Windows 真机的 TCP/OLE 进程行为无法在当前 Linux 环境完全替代，最终重连交互需用户验收。

## 下一步

- 用户执行 Linux↔Windows 真机回归：初次总超时后直接重连、断开后首次重连、创建方无限等待/握手超时、已连接后自动重连退避。
