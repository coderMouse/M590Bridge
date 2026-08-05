# task-031 · 发现列表去重 + 手动刷新

## 状态

`done`

## 问题

1. 局域网设备列表出现同一台机器多条  
2. 需要手动刷新发现列表的图标按钮

## 根因

- peers 仅按 mDNS `fullname` 索引；同一 `device_id` 因冲突改名/重注册可产生多个 fullname  
- 无按 `device_id` / `addr` 合并；无手动清空重扫  

## 修复

1. `upsert_peer`：按 `device_id`（忽略大小写）或 `addr` 或 `fullname` 去重，新记录覆盖  
2. `DiscoveryHandle::refresh`：清缓存、`stop_browse` + 新 browse 线程（generation 防旧线程写回）  
3. `POST /api/discover/refresh`  
4. UI：局域网设备旁 `RefreshCw` 按钮，loading 旋转，刷新后再拉一次列表  

## 修改文件

- `crates/m590-daemon/src/discovery.rs`  
- `crates/m590-daemon/src/hub.rs`  
- `ui/src/lib/bridgeApi.ts`（`postDiscoverRefresh`）  
- `ui/src/app/OperableApp.tsx`  
- docs：本 task、plans/current、commands  

## 验证结果

```text
cargo test -p m590-daemon --lib
  11 passed（含 upsert 去重 2 例）

cargo build -p m590-daemon -p m590-ui  ok
cd ui && npm run build  ok
```

## 文档影响检查

- 已更新：本 task、plans/current、commands  
- 无需更新：protocol 帧表  

## 风险

- 刷新瞬间列表可能短暂为空（mDNS 再解析需要几百 ms～数秒）  
- 极端情况下无 device_id 且多网卡不同 addr 仍可能两条（可接受）  

## 下一步

安装包/自启；或跨机再验发现列表。
