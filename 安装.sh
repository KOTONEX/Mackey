#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
#
# 一键安装：
#   0) 校验当前会话是 GNOME Wayland（非 GNOME 桌面或 X11 会话都警告后中止）
#   1) 生成配置（自动识别键盘配列）
#   2) 下载与当前指令集架构匹配的 xremap 最新发布版本（GNOME Wayland，固定 gnome 特性），装到 XDG 数据目录
#   3) 安装 GNOME 扩展与 systemd 用户服务
#
# 全部落点都在 $HOME 内，不需要 sudo（唯一需要提权的动作是加入 input 组，脚本会提示）。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACKEY="$ROOT/命令/mackey"

yes_flag=0
no_fetch=0
keep_running=0
for arg in "$@"; do
    case "$arg" in
        -y|--确认执行) yes_flag=1 ;;
        --不下载) no_fetch=1 ;;
        --保持运行) keep_running=1 ;;
        -h|--help)
            cat <<EOF
用法: ./安装.sh [--确认执行] [--不下载] [--保持运行]

  --确认执行           免确认
  --不下载      跳过下载 xremap（使用本机已装好的引擎）
  --保持运行  不停止正在运行的旧服务（默认会停，使结果与干净安装一致）

要求 GNOME Wayland 会话；非 GNOME 桌面或 X11 会话会警告并中止。
EOF
            exit 0 ;;
        *) echo "未知参数：$arg" >&2; exit 2 ;;
    esac
done

desktop_lc="$(printf '%s' "${XDG_CURRENT_DESKTOP:-${DESKTOP_SESSION:-}}" | tr '[:upper:]' '[:lower:]')"
if [[ "$desktop_lc" != *gnome* ]]; then
    cat >&2 <<EOF
✗ 当前桌面不是 GNOME（XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-未设置}），安装已中止。

  Mackey 仅支持 GNOME Wayland：焦点桥依赖 GNOME Shell 扩展，装到其它桌面上
  也无法启用（mackey 启用 会因焦点来源不可用而拒绝启动）。

  若确实运行在 GNOME 会话、只是环境变量没被识别，可显式指定后重跑：
    XDG_CURRENT_DESKTOP=GNOME ./安装.sh
EOF
    exit 1
fi
if [[ "${XDG_SESSION_TYPE:-}" == "x11" ]]; then
    cat >&2 <<EOF
✗ 检测到 X11 会话（XDG_SESSION_TYPE=x11），安装已中止。

  Mackey 仅支持 GNOME Wayland：GNOME 50 已移除 X11 会话，且 GNOME 扩展 API
  变动频繁，本项目不跟随更旧版本的 GNOME。

  请在 Wayland 会话（GNOME 的默认登录方式）下重跑。
EOF
    exit 1
fi

[[ $yes_flag -eq 1 ]] && export MACKEY_YES=1

echo "==> 1/3 初始化配置（自动识别键盘配列）"
"$MACKEY" 初始化 || { echo "✗ 初始化失败，安装中止" >&2; exit 1; }

echo
echo "==> 2/3 安装引擎、扩展与 systemd 用户服务"
install_args=(--不下载)
[[ $no_fetch -eq 0 ]] && install_args=()
[[ $keep_running -eq 1 ]] && install_args+=(--保持运行)
"$MACKEY" 安装 "${install_args[@]}" || { echo "✗ 安装失败，安装中止" >&2; exit 1; }

echo
"$MACKEY" 状态 || true

cat <<EOF

下一步（必须做，否则按键不会生效）：
  1) 若还没加入 input 组：sudo usermod -aG input "$(id -un)"，然后重新登录
  2) 重新登录（Wayland 下新装的 GNOME 扩展必须重登才会被 shell 加载）
  3) 重新登录后执行：  "$MACKEY" 体检 && "$MACKEY" 应用

彻底卸载：  "$ROOT/卸载.sh"
EOF
