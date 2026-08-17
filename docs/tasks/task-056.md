# task-056 · 多文件选择与批次顺序传输

## 状态

`pending`

## 背景

task-055 固定了批次清单和路径安全边界。本任务把 UI 的多选/拖放输入转换为一个批次，
并沿用现有单文件请求与分片通道按条目串行传输。

## 目标

- UI 支持一次选择多个文件和包含文件的文件夹，生成稳定的批次顺序与相对路径。
- 发送批次清单后，按条目顺序复用单文件 `FileRequest → FileChunk → FileComplete`，
  同时暴露整体进度和当前条目进度。
- 接收端在清单确认前不创建越界路径；失败、取消和替换时清理整个批次状态。

## 允许修改

- `crates/m590-core/src/session.rs`
- `crates/m590-daemon/src/hub.rs`
- `ui/src/` 相关文件选择、拖放和进度组件
- 本 task 及必要的计划/发现文档

## 禁止修改

- Windows OLE 多文件 IDataObject（task-057）和 Linux FUSE 目录树（task-058）。
- 断点续传、并行条目传输和独立数据连接。

## 验证命令

```bash
cargo test -p m590-core -p m590-daemon
npm run lint
npm run build
```

## 完成标准

- [ ] 多选/文件夹输入能生成合法批次并按序完成传输。
- [ ] 取消、替换、断线和非法清单均有确定结果且无残留临时文件。
- [ ] Linux/Windows 至少完成各自本地 UI 测试；跨机验收另行记录。

## 下一步

- 依赖 task-055 完成后实现 UI 输入与串行批次状态机。
