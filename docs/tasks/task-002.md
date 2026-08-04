# task-002 · Figma Make UI 落地为仓库前端壳

## 状态

`completed`

## 目标

将 Figma Make **M590Bridge-UI-Kit-Design** 落到仓库 `ui/`，形成可预览、可构建的设计参考前端（配对 / 主面板 / 设置 / 传输 / 托盘通知 / 深色），不接真实 daemon。

## 背景

- 用户提供 Figma Make：`https://www.figma.com/make/Bn0hmJEM901GhzpmAU9R7A/M590Bridge-UI-Kit-Design`
- Make 源为 React 设计画廊；本任务按项目栈适配为 Vite + React + TS + Tailwind 4
- 对照 `docs/ui-spec.md` 文案与信息架构

## 允许修改

- `ui/**`
- `.gitignore`（Node/前端）
- `docs/discovery/*`、`docs/plans/current.md`、本 task

## 禁止修改

- 真实剪贴板 / 网络 / 配对协议
- 完整 Tauri 打包
- Android
- git commit（未要求）

## 验证命令

```bash
cd ui && npm install && npm run build
```

## 完成标准

- [x] `ui/` 可 `npm run build` 通过
- [x] 设计 token 对齐 Figma Make / ui-spec（主色 #2563EB 等）
- [x] 可切换：组件库、配对、主面板、传输、设置、托盘通知、深色
- [x] 组件：StatusPill、DeviceCard、ClipboardPreview、HistoryRow、Toggle、主按钮
- [x] discovery / plan / task 已更新
- [x] 注明 Figma 来源与已知差异

## 实施记录

### 修改文件

- `ui/` 全部前端工程（Vite React TS）
- `ui/src/components/*` 核心组件
- `ui/src/screens/*` 各画板屏幕
- `ui/src/styles/theme.css` 设计 token
- `.gitignore`
- `docs/discovery/project-map.md`
- `docs/discovery/commands.md`
- `docs/plans/current.md`
- `docs/tasks/task-002.md`

### 验证结果

- 命令：`cd ui && npm run build`
- 结果：**通过**（`tsc -b && vite build`，产物 `dist/`，约 251 kB JS / 25 kB CSS）

### 文档影响

- 已更新：`docs/plans/current.md`、`docs/discovery/project-map.md`、`docs/discovery/commands.md`、本 task、`ui/README.md`
- 无需更新：协议/领域文档（尚无）
- 待补：接入 Tauri 时的 `m590-ui` crate 映射；若有独立 `/design/` 节点可再做像素精修清单

### 风险 / blocker

- Make 返回的巨型单文件 `App.tsx` 在 MCP 读取时被截断，落地采用 **token + 画板结构 + ui-spec** 重建为模块化代码，而非逐行粘贴 Make 源文件
- 图标使用 `lucide-react` 与内联 `AppIcon` SVG（与 Make 一致方向）；未使用过期远程临时图床
- 未实现真实交互业务，仅 UI 参考与本地开关状态

### 下一步

- 执行 **task-001** 初始化 Rust workspace
- 或指定某一页面做与 Figma 的像素级 diff
