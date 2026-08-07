# task-034 · 桌面 UI 自适应原生窗口

## 状态

`completed`

## 目标

修复桌面窗口可拉宽但页面仍被 `max-w-[420px]` 固定的问题，使可操作 UI 像原生桌面工具一样随窗口尺寸重排：窄屏保持紧凑单列，宽屏使用侧边导航和多列内容，并保持稳定的内部滚动区域。

## 背景

- Tauri 主窗口允许缩放，但 `OperableApp` 根容器固定最大宽度 420px，宽窗口两侧产生大量空白。
- 当前 UI 规格只记录 380–420px 设计画板，没有定义可拉伸窗口的响应式行为。
- 本 task 只调整表现层，不改变 hub API、配对状态机、剪贴板或文件传输行为。

## 允许修改

- `ui/src/app/OperableApp.tsx`：自适应外壳、导航和内容布局。
- `ui/src/index.css`：必要的根节点尺寸/溢出约束。
- `ui/src-tauri/tauri.conf.json`：主窗口初始尺寸和最小尺寸。
- `docs/ui-spec.md`：补充桌面窗口响应式规则。
- `docs/plans/current.md`、本 task：状态与实施记录。

## 禁止修改

- Rust core/net/daemon 与协议。
- 配对、剪贴板、文件传输、mDNS 的业务逻辑和 API。
- 新增窗口状态插件、重做视觉主题或设计画廊。
- Linux 自启、Windows 安装包等后续任务。

## 实现要求

1. 页面根容器占满 WebView，不再全局固定 420px。
2. 小窗口使用单列内容和底部导航；较宽窗口使用固定侧栏导航。
3. 主面板、设置页在足够宽时采用双列，避免单列控件无限拉宽。
4. 配对页保持聚焦的可读宽度，但应用背景、标题栏和导航铺满窗口。
5. 高度缩放时标题栏/导航稳定，内容区独立滚动且不溢出。
6. Tauri 设置合理的最小宽高，避免缩到不可操作。

## 验证命令

```bash
cd ui
npm run build
npm run lint

# 视觉检查：至少覆盖 420x780、800x720、1200x800
```

## 完成标准

- [x] 420px 左右窗口保持现有紧凑可用布局。
- [x] 拉宽窗口时应用外壳铺满，主面板/设置页有效利用横向空间。
- [x] 宽屏显示侧边导航，窄屏显示底部导航，操作入口不重复占位。
- [x] 内容和文字在三种验证尺寸下无重叠、裁切或异常横向滚动。
- [x] 前端 build 与 lint 通过。
- [x] UI 规格和计划状态已更新。

## 实施记录

- 应用根容器从居中 `max-w-[420px]` 改为 `h-dvh w-full`，标题栏和应用背景始终铺满 WebView。
- 内容区改为 `min-h-0` flex 外壳：窄屏底部导航；`>=640px` 左侧导航；导航均使用图标、当前页状态和 `aria-current`。
- 配对页保持 `max-w-[720px]` 的聚焦表单宽度；主面板和设置页最大内容宽度 1200px，`>=1024px` 使用双列 grid。
- 主面板内容区独立滚动，页脚操作保持稳定；设置页运行状态跨两列展示。
- Tauri 初始窗口改为 460×760，最小 400×600，并默认居中；仍允许自由缩放。
- 未改变任何 API 调用、状态字段、配对/剪贴板/文件事件处理逻辑。

## 修改文件

- `ui/src/app/OperableApp.tsx`：自适应外壳、响应式导航、主面板/设置页多列布局。
- `ui/src/index.css`：根节点允许 flex/grid 子项正确收缩。
- `ui/src-tauri/tauri.conf.json`：初始尺寸、最小尺寸和居中配置。
- `docs/ui-spec.md`：新增桌面窗口响应式规则。
- `docs/plans/current.md`、本 task：任务状态与记录。

## 验证结果

- `npm run build`（`ui/`）：通过；TypeScript 与 Vite production build 完成，1804 modules transformed。
- `npm run lint`（`ui/`）：通过；oxlint 无问题。
- `cargo check -p m590-ui`：通过；Tauri 配置与 Rust 桌面壳编译检查完成。
- Chromium 400×600 / 420×780：根容器宽高等于 viewport，无横向溢出；侧栏隐藏、底部导航显示。
- Chromium 800×720：根容器等于 viewport，无横向溢出；侧栏显示、底部导航隐藏，设置页保持单列。
- Chromium 1200×800：根容器等于 viewport，无横向溢出；主面板实际为 509px + 509px 双列，设置页实际为 501.5px + 501.5px 双列。
- 人工检查上述截图：文字、按钮、导航和卡片无重叠或裁切；最小窗口通过内部纵向滚动访问完整内容。

## 文档影响检查

- 已更新：`docs/ui-spec.md` 响应式窗口规格、`docs/plans/current.md` 状态、本 task 实施与验证记录。
- 无需更新：`docs/discovery/project-map.md`，未新增/删除模块或文件职责。
- 无需更新：API / 协议 / 产品能力文档，业务行为未改变。

## 风险 / blocker

- Linux 与 Windows 的系统标题栏尺寸略有差异；WebView 内布局已按 viewport 响应，但本机未做 Windows 原生窗口截图。
- 未加入窗口位置/尺寸持久化插件；本 task 只处理窗口内容贴合与重排。

## 下一步

- 新建后续 task：Linux 用户级登录自启，并提供显式启停方式。
