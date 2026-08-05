# task-029 · mDNS 局域网发现（第一刀）

## 状态

`done`

## 目标

在**不改配对协议帧**的前提下，让同一局域网内的设备能通过 mDNS/DNS-SD 互相发现，减少手动输入 IP。

1. Host 在 `listen` 等待/已连时广播服务  
2. Hub 提供发现列表 API  
3. UI joiner 可从发现列表点选对端（仍需配对码；手动 IP 保留）

## 背景

- 当前配对：`POST /api/listen` + `POST /api/connect`（addr 必填）  
- 计划 V3 第 1 项：mDNS 发现（`docs/plans/current.md`）  
- UI 规格：配对页「发现设备 / 可点连接」；设置「发现方式：自动/手动」（本 task **不**做设置开关，默认启用发现）

## 设计（本刀）

| 项 | 选择 |
|----|------|
| 服务类型 | `_m590bridge._tcp.local.` |
| 实例名 | 优先 `device_id`（sanitize） |
| 端口 | 当前 `listen_port`（默认 5901） |
| TXT | `id=<device_id>`，`ver=<app_version>`（**不**放 pairing_code） |
| 库 | `mdns-sd` 0.20（纯 Rust DNS-SD） |
| 广播时机 | `start_listen` 后 advertise；`disconnect`/bridge 结束 stop |
| 浏览 | hub 启动后后台 browse；结果经 `GET /api/discover` |
| API | `GET /api/discover` → `{service_type, advertising, peers[]}` |
| UI | joiner：展示 peers，点击填入 `host:port`；手动地址仍可用 |

## 允许修改

- `crates/m590-daemon/**`（discovery 模块、hub API、Cargo.toml）
- `ui/src/lib/bridgeApi.ts`、`ui/src/app/OperableApp.tsx`
- docs：本 task、`plans/current.md`、`discovery/*`、必要时 commands

## 禁止修改

- 帧类型 / `Message` 编解码 / 配对码校验逻辑  
- 文件通道、剪贴板逻辑  
- 安装包 / 开机自启  
- 自动免码连接、把 pairing_code 写入 mDNS TXT  
- 大范围 UI 重构 / 设置页「发现方式」完整实现

## 验证命令

```bash
cargo test -p m590-daemon --lib
cargo build -p m590-daemon -p m590-ui
cd ui && npm run build
# 双 hub 实机：host listen 后 joiner GET /api/discover 可见 peer
```

## 完成标准

- [x] Host listen 时 mDNS 广播；disconnect 后停止  
- [x] `GET /api/discover` 返回结构稳定（空列表也合法）  
- [x] UI joiner 可点选发现结果填地址  
- [x] 手动 IP 流程不回归  
- [x] 测试/构建通过  
- [x] 本 task / plan / discovery 已更新  

## 实施记录

- 新增 `crates/m590-daemon/src/discovery.rs`：`DiscoveryHandle` browse + advertise + JSON  
- hub：启动时 browse；`POST /api/listen` advertise；disconnect / bridge 结束 stop；`GET /api/discover`  
- UI：`fetchDiscover`；joiner 配对页「局域网设备」列表点选填 `addr`  
- 依赖：`mdns-sd` 0.20（default-features off + logging）  
- 过滤本机：同 `device_id` / 本机 fullname 不进 peers  

## 修改文件

- `crates/m590-daemon/Cargo.toml`：mdns-sd  
- `crates/m590-daemon/src/discovery.rs`：新建  
- `crates/m590-daemon/src/lib.rs`：export discovery  
- `crates/m590-daemon/src/hub.rs`：API + advertise 生命周期  
- `ui/src/lib/bridgeApi.ts`：Discover 类型与 `fetchDiscover`  
- `ui/src/app/OperableApp.tsx`：joiner 发现列表  
- docs：本 task、plans/current、discovery/*  

## 验证结果

```text
cargo test -p m590-daemon --lib
  9 passed (含 discovery sanitize / instance / json shape)

cargo build -p m590-daemon -p m590-ui
  ok

cd ui && npm run build
  tsc + vite ok

双 hub 实机（本机 2026-08-05）：
  host :15910 listen port 15901 device_id=smoke-host
  joiner :15911 GET /api/discover ~1.5s 后：
    peers=[{device_id:smoke-host, addr:192.168.100.108:15901, ...}]
  host advertising true → disconnect 后 advertising false
```

## 文档影响检查

- 已更新：本 task、`docs/plans/current.md`、`docs/discovery/project-map.md`、`docs/discovery/commands.md`  
- 无需更新：`protocol-draft.md` 帧表（mDNS 不改线协议）  
- 设置页「发现方式」开关：仍未实现（后续可选）  

## 风险 / blocker

- 部分网络/防火墙屏蔽 mDNS（224.0.0.251:5353）；失败时仍可手动 IP  
- 多网卡时取 mdns-sd 给出的非 loopback IPv4 优先地址，可能非最优  
- Windows 实机 mDNS 本环境未测（代码跨平台，库支持）  

## 下一步

安装包 / 开机自启预研；或设置页发现开关 / 显示名。
