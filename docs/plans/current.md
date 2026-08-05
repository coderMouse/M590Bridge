# 当前计划 · M590Bridge

> 更新：2026-08-05  
> 阶段：文本+图片完成；V2 文件通道可用（task-020..025，含路径文本/进度修复）

## 目标（近期）

Linux + Windows 剪贴板与小文件桥；V2 文件走元数据 + 按需传输 + 进度（不做 019A）。

## 已完成

- [x] task-001..018 文本/图片与硬化
- [x] **task-020** 文件协议 + Session loopback
- [x] **task-021** hub 落盘 / send_file / status
- [x] **task-022** UI 选文件发送、进度条、保存目录设置；`send_file_bytes`
- [x] **task-023** 协议不兼容提示（type 11）+ Win 选文件按钮
- [x] **task-024** file_list → 非图片原文件 offer
- [x] **task-025** 文本路径 offer + 发送方 FileComplete 进度 done

## 进行中

- [ ] 无

## 产品分期对照

| 原分期 | 内容 | 状态 |
|--------|------|------|
| MVP | 配对 + 文本 | **已完成** |
| V2 · 图片 | 图片剪贴板双向 | **已完成** |
| V2 · 文件 | 元数据 + 按需 + 进度 | **基本可用**（≤4MiB；无文件夹/OS 桌面粘贴） |
| V3 | mDNS、安装包/自启 | 未做 |

### 明确取消

- **019A 收图落盘捷径**：**不做**

## 能力边界（当前）

| 能力 | 状态 |
|------|------|
| 文本/图片双向 | 有 |
| 文件 offer/request/chunk/complete | 有 |
| hub 自动落盘 + send_file(_bytes) | 有 |
| UI 选文件发送 + 进度 + 保存目录 | **有** |
| 文件夹 / >4MiB / OS 文件剪贴板 | 无 |
| file_list 触发原文件 offer（非图片，≤4MiB） | **有** |
| 路径文本（非图片）→ file offer | **有**（task-025） |
| 发送方 FileComplete → UI done/满进度 | **有**（task-025） |

## 下一步（有序）

1. mDNS 发现  
2. 安装包 / 开机自启  
3. （可选）多文件 / >4MiB / 文件夹  

> 「开始开发」→ 新建一个子 task（建议 mDNS 或安装/自启预研）。

## 用户怎么用

```bash
cargo build -p m590-ui && cargo run -p m590-ui
```

配对后主面板「文件传输」选文件发送；设置里可改接收目录。也可用：

```bash
curl -s -X POST http://127.0.0.1:5910/api/send_file_bytes \
  -H 'content-type: application/json' \
  -d '{"name":"a.txt","data_base64":"aGVsbG8="}'
```

## 新会话入口

`AGENTS.md` → `docs/agent/*` → **本文件**
