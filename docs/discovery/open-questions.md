# 开放问题 · M590Bridge

| ID | 问题 | 临时默认 | 状态 |
|----|------|----------|------|
| Q1 | 应用显示名是否使用中文名 | `M590Bridge` | open |
| Q2 | MVP 是否包含托盘 UI | 否，先 daemon/CLI；已有 `ui/` 设计参考壳 | open |
| Q3 | 默认端口 | 实现时选定并写配置；文档暂不锁死 | open |
| Q4 | Linux 剪贴板：X11 / Wayland 优先级 | **Wayland 优先**（`WAYLAND_DISPLAY`），否则 X11（`DISPLAY`）；实现用 `arboard` 文本 API + 轮询监听 | closed |
| Q5 | 文件默认保存目录（V2） | 系统下载目录或可配置 | deferred |
| Q6 | 文件夹复制（V2） | MVP/V2 早期可不支持 | deferred |
| Q7 | 加密传输套件 | 配对后会话密钥；算法实现期定 | open |
| Q8 | Android | **明确暂缓，不做** | closed |
| Q9 | 是否需要 Windows 交叉编译 CI | 有余力再加 | open |

### Q4 结论（task-004）

- 检测顺序：`WAYLAND_DISPLAY` → `Wayland`；否则 `DISPLAY` → `X11`；都无则 `ClipboardError::NoDisplay`
- 读写：`PlatformClipboard` + `arboard`（Linux only dep）
- 变更观察：`poll_text_change` 轮询，非事件订阅
- 本开发机 Wayland 实机：`write/read/poll` 通过

关闭问题时：改状态为 `closed`，并在相关 task 或 `项目说明.md` 留下结论。
