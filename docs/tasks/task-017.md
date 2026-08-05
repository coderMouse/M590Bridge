# task-017 · 修复发大图时 TCP EAGAIN 误判断线

## 状态

`completed`

## 问题

复制图片后 UI 报：
`disconnected (tcp io error: 资源暂时不可用 (os error 11)); auto-reconnect...`

原因：`try_recv` 将 socket 设为 non-blocking 后，`send`/`write_all` 发送数 MB 图片帧时遇到 `WouldBlock`，被当成致命 IO 错误并触发重连。

## 修改

- `TcpFrameStream::write_all_blocking`：发送前恢复 blocking，写满整个帧；对 WouldBlock/TimedOut/Interrupted 重试
- 测试：`tcp_loopback_sends_large_image_after_try_recv`（先 try_recv 再发 ~1.2MiB 图）

## 验证

```bash
cargo test -p m590-net --lib
```

## 风险

- 超大图仍受 12MiB 上限；写超时 60s
