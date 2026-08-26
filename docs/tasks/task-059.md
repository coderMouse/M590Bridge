# task-059 · 统一应用版本来源

## 状态

`completed`

## 背景

当前应用版本 `0.1.1` 同时手写在根 workspace、Tauri 配置和前端 package 元数据中。
发版时需要多处同步，容易遗漏。仓库目前不新增 GitHub Actions；本 task 只处理版本
来源，不引入 CI。

## 目标

- 以根 `Cargo.toml` 的 `[workspace.package].version` 作为唯一应用版本来源。
- `m590-ui` 通过 Cargo workspace 继承该版本。
- Tauri 打包配置从 Rust 包元数据解析应用版本，不再维护独立版本号。
- 前端 package 元数据不再保存重复的应用版本号。
- 提供可重复的检查方式，确认所有 Rust 包版本一致且打包配置能解析到同一版本。

## 允许修改

- 根 `Cargo.toml`
- `ui/src-tauri/Cargo.toml`
- `ui/src-tauri/tauri.conf.json`
- `ui/package.json` 与必要时重新生成的 `ui/package-lock.json`
- 本 task 和必要的计划/项目说明文档

## 禁止修改

- 传输协议、剪贴板行为和文件传输逻辑。
- 新增 CI 工作流或发布流水线。
- 代码签名、安装包上传和自动升级机制。
- 无关依赖升级与目录结构调整。

## 验证命令与完成标准

```bash
cargo metadata --no-deps --format-version 1 | jq -e \
  '([.packages[].version] | all(. == "0.1.1"))'
npm ci --prefix ui
npm run build --prefix ui
cargo check --workspace
cargo fmt --all -- --check
git diff --check
```

另需运行 Tauri Linux 构建确认删除显式版本后 bundle 元数据仍为 `0.1.1`。若本机环境
无法完成构建，必须在验证结果中记录真实 blocker 与复现步骤。

## 实施记录

- 将 `m590-ui` 的手写版本改为 `version.workspace = true`，由根 workspace 统一提供。
- 从 `tauri.conf.json` 移除显式应用版本；Tauri 在字段缺失时读取 Cargo 包版本。
- 从 `ui/package.json` 移除重复的应用版本，并用 npm 重新同步 lockfile；随后手工去掉
  npm 重写时夹带的无关可选依赖元数据。
- 未新增 CI、发布流程或运行时逻辑。

## 修改文件

- `Cargo.toml`
- `ui/src-tauri/Cargo.toml`
- `ui/src-tauri/tauri.conf.json`
- `ui/package.json`
- `ui/package-lock.json`
- 本 task
- `docs/plans/current.md`

## 验证结果

- `cargo metadata --no-deps --format-version 1 | jq -e '([.packages[].version] | all(. == "0.1.1"))'`：
  输出 `true`，workspace 内全部 Rust 包版本一致。
- `npm ci --prefix ui`：通过，安装 50 packages，0 vulnerabilities。
- `npm run build --prefix ui`：通过，TypeScript 与 Vite production build 成功。
- `cargo check --workspace`：通过，约 2 分 01 秒完成。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。
- 应用版本副本检查 `rg '"version": "0\.1\.1"' ui/package.json ui/package-lock.json ui/src-tauri/tauri.conf.json`：
  无匹配。
- `npm run desktop:build --prefix ui`：通过，生成
  `target/release/bundle/deb/M590Bridge_0.1.1_amd64.deb`。
- `dpkg-deb -f target/release/bundle/deb/M590Bridge_0.1.1_amd64.deb Package Version Architecture`：
  返回 `m590-bridge`、`0.1.1`、`amd64`，确认 bundle 元数据继承成功。

## 文档影响

- 已更新当前计划：记录 task-059 完成，并把“增加 CI”从“统一版本来源”中拆出为独立下一步。

## 风险 / blocker

- blocker：无。Tauri Linux 构建已实际确认版本继承行为。
- Windows 打包未在本轮重跑；其同样通过 Tauri 配置和 `m590-ui` 包元数据取版本，
  后续 Windows 打包回归时应顺带确认 NSIS 产品版本。

## 下一步

- 无阻塞事项；CI 应另建独立 task，不并入本任务。
