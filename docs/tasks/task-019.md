# task-019 · 图片落盘 + 文件剪贴板（桌面可粘贴）【待开发】

## 状态

`pending`

## 目标（默认范围 = 方案 A）

接收到的 `ClipboardImage` 在写入图片剪贴板之外，再：

1. 保存为本地 png（下载目录或可配置目录）  
2. 将该路径写入 OS **文件列表剪贴板**（Windows CF_HDROP / Linux text/uri-list）  

使对端可在**桌面或文件夹**中粘贴出图片文件。  

**不做（本 task）**：通用任意文件/文件夹传输、分片协议、进度 UI 全套（那是方案 B / 后续 task）。

## 背景

- task-014..018：图片位图双向已可用（Word 等可粘贴）  
- 桌面粘贴需要文件语义，位图剪贴板不够  
- 计划默认下一刀为体验向的 019A

## 允许修改（开发时）

- `m590-clipboard`：set file_list；可选保存路径辅助  
- `m590-daemon` hub/main：收图后落盘 + set files  
- 配置项（保存目录）若最小需要  
- docs：本 task、plan、commands

## 禁止修改

- 无关大重构、Android、公网、完整加密  
- 一次做完整 LocalSend 级文件网格

## 验证（开发时）

```bash
cargo test -p m590-clipboard -p m590-daemon -p m590-core -p m590-net
# 实机：A 复制图 → B 桌面/文件夹粘贴出现 png；Word 粘贴仍可用
```

## 完成标准（开发时勾选）

- [ ] 收图可落盘  
- [ ] 文件剪贴板可被桌面/资源管理器粘贴  
- [ ] 位图剪贴板路径不回退  
- [ ] 失败可见（日志或 last_error）  
- [ ] 文档已更新  

## 实施记录

（开始开发后填写）
