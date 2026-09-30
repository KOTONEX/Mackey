#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
#
# 一键卸载：停用服务、还原 GNOME 键位、删除扩展 / 引擎 / 缓存 / 状态 / 配置。
# 默认包含 --purge（连 ~/.config/mackey 一起删）；要保留配置用 --keep-config。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

purge=(--purge)
pass=()
for arg in "$@"; do
    case "$arg" in
        --keep-config|--no-purge) purge=() ;;
        -h|--help)
            cat <<EOF
用法: ./uninstall.sh [--keep-config] [--dry-run] [--yes]

  --keep-config  保留 ~/.config/mackey（配置与备份）
  --dry-run      只列出将要清理的内容，不做任何改动
  --yes          免确认
EOF
            exit 0 ;;
        *) pass+=("$arg") ;;
    esac
done

exec "$ROOT/bin/mackey" uninstall "${purge[@]}" "${pass[@]}"
