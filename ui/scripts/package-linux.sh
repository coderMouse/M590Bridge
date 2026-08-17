#!/usr/bin/env bash

set -euo pipefail

fail() {
  printf '打包失败：%s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "缺少命令 '$1'。请先安装 Ubuntu 打包依赖，详见 ui/README.md。"
}

script_path="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/$(basename -- "${BASH_SOURCE[0]}")"

# 构建过程不需要 root 权限。使用 `sudo ./package-linux.sh` 时，sudo 的
# secure_path 往往会隐藏 nvm/rustup 中的用户级 node、npm 和 cargo，导致
# 预检误报缺少 node。切回发起 sudo 的用户，并让其登录 shell 恢复完整 PATH，
# 同时避免在仓库里生成 root 所有的 node_modules/target 文件。
if (( EUID == 0 )); then
  if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
    command -v sudo >/dev/null 2>&1 || fail "检测到 root 且找不到 sudo，请切换普通用户运行打包脚本。"
    printf '检测到 sudo，改用用户 %s 执行打包（构建无需 root 权限）……\n' "$SUDO_USER"
    if ! exec sudo -u "$SUDO_USER" -H bash -lc 'exec "$0" "$@"' "$script_path" "$@"; then
      fail "无法切换回用户 '$SUDO_USER' 执行打包。"
    fi
  fi

  fail "打包不需要 root 权限，请使用普通用户直接运行 ./ui/scripts/package-linux.sh。"
fi

if [[ "$(uname -s)" != "Linux" ]]; then
  fail "package-linux.sh 只能在 Linux 构建机上运行。"
fi

script_directory="$(dirname -- "$script_path")"
ui_directory="$(cd -- "$script_directory/.." && pwd)"
repository_directory="$(cd -- "$ui_directory/.." && pwd)"

for required_command in node npm cargo rustc pkg-config dpkg-deb file; do
  require_command "$required_command"
done

[[ -f "$ui_directory/package-lock.json" ]] || fail "找不到 ui/package-lock.json。"

required_packages=(
  gtk+-3.0
  webkit2gtk-4.1
  ayatana-appindicator3-0.1
)

missing_packages=()
for required_package in "${required_packages[@]}"; do
  if ! pkg-config --exists "$required_package"; then
    missing_packages+=("$required_package")
  fi
done

if (( ${#missing_packages[@]} > 0 )); then
  printf '打包失败：缺少 pkg-config 开发库：%s\n' "${missing_packages[*]}" >&2
  printf '%s\n' 'Ubuntu 可执行：' >&2
  printf '%s\n' '  sudo apt-get update' >&2
  printf '%s\n' '  sudo apt-get install -y build-essential curl file libayatana-appindicator3-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev wget' >&2
  exit 1
fi

printf '%s\n' '正在安装锁定的 Node.js 依赖……'
(
  cd -- "$ui_directory"
  npm ci
)

printf '%s\n' '正在构建 Linux .deb 安装包……'
(
  cd -- "$ui_directory"
  npm run desktop:build -- --bundles deb
)

artifact_directory="$repository_directory/target/release/bundle/deb"
shopt -s nullglob
artifacts=("$artifact_directory"/*.deb)
shopt -u nullglob

if (( ${#artifacts[@]} == 0 )); then
  fail "构建命令已结束，但 $artifact_directory 中没有 .deb 产物。"
fi

printf '\n%s\n' 'Linux 打包完成：'
printf '  %s\n' "${artifacts[@]}"
