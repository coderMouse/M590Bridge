# 当前计划 · M590Bridge

> 更新：2026-08-05  
> 阶段：文本+图片+V2 文件流式 + mDNS + Linux .deb；**下一刀：Linux 登录自启**（或用户指定）

## 目标（近期）

Linux + Windows 剪贴板与小文件桥；局域网发现；后续安装/自启。

## 已完成

- [x] task-001..018 文本/图片与硬化
- [x] **task-020** 文件协议 + Session loopback
- [x] **task-021** hub 落盘 / send_file / status
- [x] **task-022** UI 选文件发送、进度条、保存目录设置；`send_file_bytes`
- [x] **task-023** 协议不兼容提示（type 11）+ Win 选文件按钮
- [x] **task-024** file_list → 非图片原文件 offer
- [x] **task-025** 文本路径 offer + 发送方 FileComplete 进度 done
- [x] **task-026** GNOME Wayland 文件复制限制：拖放/选文件 + 提示
- [x] **task-027** 原生选文件/窗口拖放 + 托盘文案保活 + 关闭焦点
- [x] **task-028** 桌面裸文件名解析 + 托盘恢复关闭可点
- [x] **task-029** mDNS 广播 + `GET /api/discover` + UI joiner 点选
- [x] **task-030** 配对 reject/超时退出 + 错误提示；清理 smoke 配置污染
- [x] **task-031** 发现列表按 device_id/addr 去重 + 手动刷新
- [x] **task-032** Linux `.deb` 安装包基线
- [x] **task-033** 大文件流式传输（路径发送 / `.part` / SHA-256）

## 进行中 / 下一 task

- （无 in_progress）建议下一刀：**Linux 登录自启**（需新建 task-034）

## 产品分期对照

| 原分期 | 内容 | 状态 |
|--------|------|------|
| MVP | 配对 + 文本 | **已完成** |
| V2 · 图片 | 图片剪贴板双向 | **已完成** |
| V2 · 文件 | 元数据 + 按需 + 进度 + 流式 | **基本可用**（task-033 流式+SHA-256；无文件夹/OS 桌面粘贴/断点续传） |
| V3 · mDNS | 局域网发现 | **第一刀完成**（task-029） |
| V3 · 安装 | 安装包/自启 | **Linux `.deb` 第一刀完成**；Windows/自启未做 |

### 明确取消

- **019A 收图落盘捷径**：**不做**

## 能力边界（当前）

| 能力 | 状态 |
|------|------|
| 文本/图片双向 | 有 |
| 文件 offer/request/chunk/complete | 有 |
| hub 自动落盘 + send_file(_bytes) | 有 |
| UI 选文件发送 + 进度 + 保存目录 | **有** |
| 文件夹 / OS 文件剪贴板 | 无 |
| 大文件流式（磁盘流+SHA-256，软上限 8GiB） | **有**（task-033；同连接串行） |
| file_list 触发原文件 offer（非图片，路径流式） | **有** |
| 路径文本（非图片）→ file offer | **有**（task-025） |
| 发送方 FileComplete → UI done/满进度 | **有**（task-025） |
| GNOME Wayland 文件管理器复制自动同步 | **受限**（用原生选文件/窗口拖放，task-026/027） |
| UI 拖入/原生选文件发送 | **有**（task-026/027） |
| 托盘菜单文案保活 | **有**（task-027） |
| mDNS 发现（`_m590bridge._tcp`） | **有**（task-029；仍需配对码） |
| Linux `.deb` 安装包 | **有**（task-032；amd64、未签名） |
| 设置「发现方式」开关 | 无（默认开启 browse） |

## 下一步（有序）

1. **Linux 登录自启**（用户级、可显式启停）
2. Windows 安装包 / 开机自启
3. （可选）独立文件数据连接 / 更高吞吐调优
4. （可选）设置页发现开关 / 本机显示名
5. （可选）多文件并行 / 文件夹 / OS 文件剪贴板

> 「开始开发」→ 新建并执行下一 pending task（建议 Linux 自启），或用户指定。

## 用户怎么用

```bash
cargo build -p m590-ui && cargo run -p m590-ui
```

Linux 安装包：

```bash
cd ui
npm run desktop:build -- --bundles deb
cd ..
sudo apt install ./target/release/bundle/deb/M590Bridge_*_amd64.deb
```

- **创建配对**：本机生成配对码 → 开始等待（会 mDNS 广播）  
- **加入**：同一局域网列表点选对端，或手动填 `host:port`，输入同一配对码连接  
- 主面板「文件传输」：原生选文件/拖放；设置里改接收目录  

```bash
curl -s http://127.0.0.1:5910/api/discover
curl -s -X POST http://127.0.0.1:5910/api/send_file_bytes \
  -H 'content-type: application/json' \
  -d '{"name":"a.txt","data_base64":"aGVsbG8="}'
```

## 新会话入口

`AGENTS.md` → `docs/agent/*` → **本文件**
