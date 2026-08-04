# task-012 · 设置页正式配置项（去 JSON）

## 状态

`completed`

## 目标

设置页不再展示原始 JSON，改为分组表单配置项，可保存到本机 hub 配置。

## 完成标准

- [x] 设备 / 网络 / 同步 / 运行状态 分组展示
- [x] 可编辑：设备 ID、监听端口、默认对端地址、默认配对码
- [x] 开关：自动同步、断线重连
- [x] 「保存配置」调用 `POST /api/config`
- [x] 无 `JSON.stringify(status)` 调试块
- [x] `npm run build` 通过

## 修改文件

- `ui/src/app/OperableApp.tsx`
- `ui/src/lib/bridgeApi.ts`（`HubConfigPatch`）
- docs：本 task、plan
