# 项目结构图 · M590Bridge

> 更新日期：2026-08-04  
> 状态：核心同步 + Web 可操作壳 + Tauri 托盘桌面壳

```text
crates/
  m590-core / clipboard / net / daemon(lib+bin)
ui/
  src/                 # React 可操作壳 + 设计画廊
  src-tauri/           # Tauri 2：m590-ui，托盘 + 内嵌 hub
target/debug/m590-ui   # 桌面可执行文件
```
