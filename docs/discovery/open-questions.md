# 开放问题 · M590Bridge

| ID | 问题 | 临时默认 | 状态 |
|----|------|----------|------|
| Q1 | 应用显示名是否使用中文名 | `M590Bridge` | open |
| Q2 | MVP 是否包含托盘 UI | **是**：Tauri `m590-ui` 托盘 + 内嵌 hub；Linux/Windows 已联调 | closed |
| Q3 | 默认端口 | 实现时选定并写配置；文档暂不锁死 | open |
| Q4 | Linux 剪贴板：X11 / Wayland 优先级 | **Wayland 优先**（`WAYLAND_DISPLAY`），否则 X11（`DISPLAY`）；实现用 `arboard` 文本 API + 轮询监听 | closed |
| Q5 | 文件/收图默认保存目录 | 系统下载目录或可配置；task-019 落地时选定 | open |
| Q6 | 文件夹复制（V2） | MVP/V2 早期可不支持 | deferred |
| Q7 | 加密传输套件 | 配对后会话密钥；算法实现期定 | open |
| Q8 | Android | **明确暂缓，不做** | closed |
| Q9 | 是否需要 Windows 交叉编译 CI | 有余力再加 | open |
| Q10 | 收图是否同时写入文件剪贴板（桌面可粘贴） | **否**（用户取消 019A）；桌面文件体验归正式文件通道 | closed |

### Q2 结论（task-009 / task-013）

- MVP 桌面入口为 `m590-ui`（托盘、主面板、内嵌 hub）
- Linux 与 Windows 均可构建运行；跨机文本同步已实机确认
- CLI `m590-daemon` 仍保留作调试/无 UI 场景

### Q4 结论（task-004）

- 检测顺序：`WAYLAND_DISPLAY` → `Wayland`；否则 `DISPLAY` → `X11`；都无则 `ClipboardError::NoDisplay`
- 读写：`PlatformClipboard` + `arboard`（Linux only dep）
- 变更观察：`poll_text_change` 轮询，非事件订阅
- 本开发机 Wayland 实机：`write/read/poll` 通过

关闭问题时：改状态为 `closed`，并在相关 task 或 `项目说明.md` 留下结论。


### Q10 结论（2026-08-05）

- 用户取消「收图落盘 + 文件剪贴板」捷径  
- 桌面/文件体验通过 **V2 文件通道**（元数据 + 传输 + 进度）实现，不单靠骗文件剪贴板  
