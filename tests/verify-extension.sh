#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 隔离带验证扩展：拿真实 extension.js，只把 shell 专属的 import 换成桩，
# 然后用真实 D-Bus 往返验证契约。见 tests/verify-extension.mjs 顶部说明。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="mackey-focus@kotonex"
WORK=$(mktemp -d /tmp/mackey-ext.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

# shell 提供的基类：桩掉，其余代码原样保留
cat > "$WORK/shell-stub.js" <<'EOF'
export class Extension {
    constructor(metadata) { this.metadata = metadata; }
}
EOF

sed 's#resource:///org/gnome/shell/extensions/extension.js#./shell-stub.js#' \
    "$ROOT/extension/$UUID/extension.js" > "$WORK/extension-undertest.js"

# 确认桩化只改动了 import 那一行：逐行比较原文件与改写结果，
# 避免用 sed 往返比较（那等价于恒真式，永远发现不了多出来的改动）
if ! python3 - "$ROOT/extension/$UUID/extension.js" "$WORK/extension-undertest.js" <<'PY'
import sys

orig_path, new_path = sys.argv[1], sys.argv[2]
with open(orig_path, encoding="utf-8") as fh:
    orig = fh.read().splitlines()
with open(new_path, encoding="utf-8") as fh:
    new = fh.read().splitlines()

if len(orig) != len(new):
    print(f"✗ 桩化改变了行数：原 {len(orig)} 行 -> 现 {len(new)} 行", file=sys.stderr)
    sys.exit(1)

changed = [(i + 1, old, cur) for i, (old, cur) in enumerate(zip(orig, new)) if old != cur]
if len(changed) != 1:
    print(f"✗ 桩化应恰好改动 1 行，实际改动了 {len(changed)} 行", file=sys.stderr)
    sys.exit(1)

line_no, old_line, new_line = changed[0]
src = "resource:///org/gnome/shell/extensions/extension.js"
expected = old_line.replace(src, "./shell-stub.js", 1)
if new_line != expected:
    print(f"✗ 第 {line_no} 行的改动不是 import 桩化：", file=sys.stderr)
    print(f"  原：{old_line}", file=sys.stderr)
    print(f"  现：{new_line}", file=sys.stderr)
    sys.exit(1)

print(f"  桩化只改动了第 {line_no} 行（import 替换）")
PY
then
    exit 1
fi

echo "==> 用桩化的 shell 环境加载真实 extension.js"
gjs -m "$ROOT/tests/verify-extension.mjs" "$WORK/extension-undertest.js"
