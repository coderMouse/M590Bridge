# task-024 · file_list → 原文件 offer（非图片）

## 状态

`completed`

## 目标

文件管理器复制普通文件时，经 `file_list` 自动 `offer_file`（≤4MiB）。

## 策略

- 图片：仍只走 ClipboardImage
- 非图片：第一个常规文件读盘 + offer
- 过大/目录：`last_error` / CLI 日志

## 修改

- `m590-clipboard::file_paths`：`first_regular_file` / `read_file_for_offer`
- hub + daemon CLI：`poll_file_list_change` 非图片分支 offer

## 验证

```text
cargo test -p m590-clipboard -p m590-daemon -p m590-core --lib
cargo build -p m590-daemon
```

## 文档影响

- 已更新：本 task、plans/current、commands
