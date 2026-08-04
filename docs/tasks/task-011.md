# task-011 · 修复「开始等待配对」间歇 code required

## 状态

`completed`

## 目标

消除本机点击「开始等待配对」时经常出现的 `code required`，提高一次成功率。

## 原因

hub HTTP 只 `read` 一次；POST body 分包到达时 body 为空，解析不到 `code`。

## 修改

- `crates/m590-daemon/src/hub.rs`：按 Content-Length 读全请求；配对码规范化；host 空码可回退/生成
- `ui/src/lib/bridgeApi.ts` / `OperableApp.tsx`：发送前清洗配对码，空码自动补全

## 验证

- `cargo test -p m590-daemon` 通过
- hub 冒烟：`POST /api/listen` 带码/空码均可 200
- `npm run build` 通过
