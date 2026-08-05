# 项目结构图 · M590Bridge

> 更新日期：2026-08-05  
> 状态：文本+图片剪贴板双向可用；文件通道 / 桌面文件粘贴待做

```text
crates/
  m590-core/       # DeviceId、Message、Session、ImageEncoding
  m590-clipboard/  # 文本/图片/file_list；PNG prepare；路径提升
  m590-net/        # 帧编解码、TCP（send 前恢复 blocking）
  m590-daemon/     # CLI + hub API + 同步环
ui/
  src/             # React 主面板 / 设置
  src-tauri/       # Tauri 2 m590-ui：托盘 + 内嵌 hub
docs/
  plans/current.md # 计划 source of truth
  tasks/           # task-001..019
  domain/          # 协议草案
```
