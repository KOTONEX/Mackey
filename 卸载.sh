#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
#
# 一键卸载：停用服务、还原 GNOME 键位、删除扩展 / 引擎 / 缓存 / 状态 / 配置。
# 默认包含 --清除配置（连 ~/.config/mackey 一起删）；要保留配置用 --保留配置。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

purge=(--清除配置)
pass=()
for arg in "$@"; do
    case "$arg" in
        --保留配置) purge=() ;;
        -h|--help)
            cat <<EOF
用法: ./卸载.sh [--保留配置] [--预演] [--确认执行]

  --保留配置  保留 ~/.config/mackey（配置与备份）
  --预演      只列出将要清理的内容，不做任何改动
  --确认执行          免确认
EOF
            exit 0 ;;
        *) pass+=("$arg") ;;
    esac
done

exec "$ROOT/命令/mackey" 卸载 "${purge[@]}" "${pass[@]}"
