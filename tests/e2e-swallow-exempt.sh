#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 端到端验证「替换模式」的应用例外（真实 xremap + 虚拟键盘，不碰真实键盘）：
#   - 终端类应用：显式键命中、泛化不命中、Ctrl+C 不被吞（SIGINT 保留）
#   - IDE 类应用：显式键与泛化都命中、Ctrl+C 不被吞（内嵌终端仍能中断）
#   - 其它应用： 显式键与泛化都命中、Ctrl+C 被吞掉
# 结构与生成器产出的 keymap 一致：terminal(only) / generic / generic-sweep(not) / swallow(not)。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="${MACKEY_ENGINE:-}"
if [[ -z "$ENGINE" || ! -x "$ENGINE" ]]; then
    printf '%s\n' '找不到可执行的 xremap 引擎；先 make fetch-engine 或设置 MACKEY_ENGINE=...' >&2
    exit 1
fi
# 这个用例需要精确构造焦点类名（kitty/code/some-app），只能走内置静态来源。
BACKEND_LABEL="静态桩"
WORK=$(mktemp -d /tmp/mackey-swallow.XXXXXX)
PROBE="$WORK/probe"
mkdir -p "$PROBE"

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

PIDS=()
cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null; done
    rm -rf "$WORK"
}
trap cleanup EXIT

cat > "$WORK/keymap.json" <<JSON
{
  "keymap": [
    { "name": "terminal-like", "application": { "only": ["/^kitty$/"] },
      "remap": { "Super-c": { "launch": ["/usr/bin/touch", "$PROBE/terminal-copy"] } } },
    { "name": "generic",
      "remap": { "Super-l": { "launch": ["/usr/bin/touch", "$PROBE/generic-loc"] } } },
    { "name": "generic-sweep", "application": { "not": ["/^kitty$/"] },
      "remap": { "Super-b": { "launch": ["/usr/bin/touch", "$PROBE/sweep-b"] } } },
    { "name": "swallow", "application": { "not": ["/^kitty$/", "/^code$/"] },
      "remap": { "C-c": { "launch": ["/usr/bin/touch", "$PROBE/swallow-cc"] } } }
  ]
}
JSON

RC=0
expect_present() { [[ -f "$PROBE/$1" ]] && echo "    ✓ $1 命中" || { echo "    ✗ $1 未命中"; RC=1; }; }
expect_absent()  { [[ ! -f "$PROBE/$1" ]] && echo "    ✓ $1 未命中（符合预期）" || { echo "    ✗ $1 不该命中"; RC=1; }; }

run_case() {
    local name="$1" klass="$2" device="mackey-swallow-$1"
    local sock="$WORK/focus-$name.sock" go="$WORK/go-$name"
    rm -f "$PROBE/terminal-copy" "$PROBE/generic-loc" "$PROBE/sweep-b" "$PROBE/swallow-cc"

    python3 "$ROOT/tools/focusd.py" --backend static --static-class "$klass" --socket "$sock" \
        > "$WORK/focusd-$name.log" 2>&1 &
    PIDS+=($!)
    sleep 1.2

    sg input -c "python3 '$ROOT/tests/fake_keyboard.py' --name '$device' \
        --send 'LEFTMETA:1,C:1,C:0,LEFTMETA:0,LEFTMETA:1,L:1,L:0,LEFTMETA:0,LEFTMETA:1,B:1,B:0,LEFTMETA:0,LEFTCTRL:1,C:1,C:0,LEFTCTRL:0' \
        --wait-file '$go' --keep-alive 3" > "$WORK/kbd-$name.log" 2>&1 &
    PIDS+=($!)
    sleep 1.2

    sg input -c "GNOME_SOCKET='$sock' timeout 15 '$ENGINE' --device '$device' '$WORK/keymap.json'" \
        > "$WORK/xremap-$name.log" 2>&1 &
    PIDS+=($!)
    sleep 2.5
    touch "$go"
    sleep 3
}

echo "==> 1/3 终端类应用（kitty）：显式键命中，泛化与吞键都不命中"
run_case term kitty
expect_present terminal-copy
expect_present generic-loc
expect_absent sweep-b
expect_absent swallow-cc

echo "==> 2/3 IDE 类应用（code）：显式键与泛化命中，Ctrl+C 不被吞"
run_case ide code
expect_present generic-loc
expect_present sweep-b
expect_absent swallow-cc

echo "==> 3/3 其它应用（some-app）：Ctrl+C 被吞"
run_case other some-app
expect_present generic-loc
expect_present sweep-b
expect_present swallow-cc

if [[ $RC -eq 0 ]]; then
    echo "✓ 通过：替换模式的应用例外按预期工作（焦点后端：$BACKEND_LABEL；终端/IDE 保留 Ctrl+C，其它应用被吞）"
    exit 0
fi
echo "✗ 失败（焦点后端：$BACKEND_LABEL）"
for log in "$WORK"/xremap-*.log; do echo "--- $log"; tail -5 "$log"; done
exit 1
