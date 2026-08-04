# task-009 · Tauri 桌面壳 + 系统托盘

## 状态

`completed`

## 目标

用 **Tauri 2** 包装现有 `ui/` 可操作壳：单窗口主面板、关闭到托盘、启动时内嵌 **hub API**（无需另开 `m590-daemon hub` 终端）。

## 背景

- task-008：浏览器 UI + 外置 hub
- 跨机文本同步已通；需要更接近产品的桌面入口

## 允许修改

- `ui/src-tauri/**`、`ui/package.json`
- `crates/m590-daemon`（lib 导出 hub）
- 根 `Cargo.toml` workspace members
- docs

## 禁止修改

- 文件/图片通道、Android
- 完整安装包多平台签名（可只 debug 构建）
- git commit（除非用户要求）

## 验证命令

```bash
cargo build -p m590-ui
cd ui && npm run build
# 可选：
cargo run -p m590-ui
# 或
cd ui && npm run desktop:dev
```

## 完成标准

- [x] Tauri 工程存在且加入 workspace
- [x] 启动时内嵌 hub（`127.0.0.1:5910`）
- [x] 系统托盘：打开主面板 / 退出；关闭窗口隐藏到托盘
- [x] `cargo build -p m590-ui` 通过
- [x] 文档更新

## 实施记录

### 修改文件

- `ui/src-tauri/**`（Tauri 2 应用）
- `ui/package.json`（`desktop:dev` / `desktop:build`、`@tauri-apps/cli`）
- `crates/m590-daemon`：`lib.rs` + Cargo lib
- 根 `Cargo.toml`：members 含 `ui/src-tauri`
- docs：本 task、plan、commands、project-map、ui README

### 验证结果

- `cargo build -p m590-ui`：通过（产物 `target/debug/m590-ui`）
- 短时运行：日志出现 `hub_api=http://127.0.0.1:5910`；托盘依赖有 ayatana 弃用警告但不阻塞
- `cargo test`（workspace）：通过
- `npm run build`（前端）：随 tauri/构建流程已验证

### 文档影响

- 已更新 plan / discovery / ui README
- 已补：Windows `m590-ui` 构建与联调（见 task-013）
- 待补：正式安装包 / 开机自启

### 风险

- Linux 托盘因桌面环境而异（Wayland/X11）
- hub 仍绑 loopback；跨机同步端口照旧
- 未做自动更新 / 签名安装包

### 下一步

- 已完成：Windows `m590-ui`（task-013）；配置持久化/重连（task-010..012）
- 后续：图片/文件通道、mDNS、安装包
