#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 隔离带验证扩展：拿真实 extension.js，只把 shell 专属的 import 换成桩，
# 然后用真实 D-Bus 往返验证契约。见 测试/扩展契约.mjs 顶部说明。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="mackey-focus@kotonex"
WORK=$(mktemp -d "$HOME/.mackey-ext.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

# shell 提供的基类：桩掉，其余代码原样保留
cat > "$WORK/shell-stub.js" <<'EOF'
export class Extension {
    constructor(metadata) { this.metadata = metadata; }
}
EOF

sed 's#resource:///org/gnome/shell/extensions/extension.js#./shell-stub.js#' \
    "$ROOT/扩展/$UUID/extension.js" > "$WORK/extension-undertest.js"

# 确认桩化只改动了 import 那一行：逐行比较原文件与改写结果，
# 避免用 sed 往返比较（那等价于恒真式，永远发现不了多出来的改动）
if ! awk '
    NR == FNR { original[FNR]=$0; count=FNR; next }
    {
        current=FNR
        if ($0 != original[FNR]) {
            changed++
            src="resource:///org/gnome/shell/extensions/extension.js"
            at=index(original[FNR], src)
            expected=substr(original[FNR], 1, at-1) "./shell-stub.js" substr(original[FNR], at+length(src))
            if (!at || $0 != expected) bad=1
        }
    }
    END { if (count != current || changed != 1 || bad) exit 1 }
' "$ROOT/扩展/$UUID/extension.js" "$WORK/extension-undertest.js"
then
    exit 1
fi

echo "==> 用桩化的 shell 环境加载真实 extension.js"
GIO_USE_VFS=local dbus-run-session -- gjs -m "$ROOT/测试/扩展契约.mjs" "$WORK/extension-undertest.js" "$ROOT/target/debug/mackey"
