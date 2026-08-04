# 项目结构图 · M590Bridge

> 更新日期：2026-08-04  
> 状态：文本 + 图片剪贴板同步；Linux/Windows 桌面壳（`m590-ui`）已可用

```text
crates/
  m590-core / clipboard / net / daemon(lib+bin)
ui/
  src/                 # React 可操作壳 + 设计画廊
  src-tauri/           # Tauri 2：m590-ui，托盘 + 内嵌 hub
target/debug/m590-ui   # 桌面可执行文件（本机构建产物，勿提交）
```
