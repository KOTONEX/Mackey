#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 端到端验证：焦点桥 → xremap「按应用分流」是否真的生效。
#
# 特点：全程不碰真实键盘（只用 测试/虚拟键盘.rs 的 uinput 虚拟键盘）。
#   注意：未被映射、需要透传的键会由 xremap 转发到当前焦点窗口，
#   所以跑测试时不要让敏感窗口抢焦点。
#   - 用 测试/虚拟键盘.rs 造虚拟键盘，xremap 只抓这个设备（--device 过滤）；
#   - 命中动作是 `launch: touch <文件>`，所以「哪条 keymap 生效」看文件名即可；
#   - 判据用「类名非空」正则而不是写死类名，避免焦点被其他应用抢走导致误判。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="${MACKEY_ENGINE:-}"
if [[ -z "$ENGINE" || ! -x "$ENGINE" ]]; then
    printf '%s\n' '找不到可执行的 xremap 引擎；先 make fetch-engine 或设置 MACKEY_ENGINE=...' >&2
    exit 1
fi
WORK=$(mktemp -d "$HOME/.mackey-e2e.XXXXXX")
PROBE="$WORK/probe"
SOCK="$WORK/focus.sock"

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

mkdir -p "$PROBE"
cleanup() {
    [[ -n "${FOCUSD_PID:-}" ]] && kill "$FOCUSD_PID" 2>/dev/null
    rm -rf "$WORK"
}
trap cleanup EXIT

echo "==> 1/4 启动焦点桥"
FOCUSD_ARGS=()
BACKEND="固定"
BACKEND_LABEL="静态桩"
if "$ROOT/命令/mackey" 焦点桥 --测试 >/dev/null 2>&1; then
    BACKEND="dbus"
    BACKEND_LABEL="真实 D-Bus"
    echo "    使用真实焦点来源（GNOME D-Bus 焦点桥）"
else
    FOCUSD_ARGS=(--后端 固定)
    echo "    ! 本机当前没有可用的真实焦点来源（扩展多半还没重新登录加载）"
    echo "      退回内置静态来源，只验证「xremap ← socket ← 焦点信息」这一段；"
    echo "      真实来源的 D-Bus 契约由 测试/扩展契约.sh 覆盖"
fi
"$ROOT/命令/mackey" 焦点桥 "${FOCUSD_ARGS[@]}" --套接字 "$SOCK" > "$WORK/focusd.log" 2>&1 &
FOCUSD_PID=$!
sleep 1.5
if ! grep -q '监听' "$WORK/focusd.log"; then
    echo "✗ 焦点桥启动失败："; cat "$WORK/focusd.log"; exit 1
fi
tail -1 "$WORK/focusd.log"

CURRENT=$("$ROOT/命令/mackey" 焦点桥 --测试 2>/dev/null | jq -r '.wm_class' 2>/dev/null)
echo "==> 2/4 当前焦点应用：${CURRENT:-（空）}"

# 每个用例用独立的虚拟设备名，这样上一轮遗留的 xremap 进程不会抓错设备。
run_case() {
    local name="$1" pattern="$2" device="mackey-test-$1"
    local out="$PROBE/$name" go="$WORK/go-$name"
    cat > "$WORK/keymap-$name.json" <<EOF
{
  "keymap": [
    { "name": "app-specific",
      "application": { "only": ["$pattern"] },
      "remap": { "Super-c": { "launch": ["/usr/bin/touch", "$out"] } } },
    { "name": "generic",
      "remap": { "Super-c": { "launch": ["/usr/bin/touch", "$PROBE/GENERIC-$name"] } } }
  ]
}
EOF
    sg input -c "'$ROOT/target/debug/examples/虚拟键盘' --名称 '$device' --事件 'LEFTMETA:1,C:1,C:0,LEFTMETA:0' --等待文件 '$go' --存活秒数 3" \
        > "$WORK/kbd-$name.log" 2>&1 &
    sleep 1.5
    sg input -c "GNOME_SOCKET='$SOCK' RUST_LOG=xremap=debug timeout 15 '$ENGINE' --device '$device' '$WORK/keymap-$name.json'" \
        > "$WORK/xremap-$name.log" 2>&1 &
    sleep 2.5
    touch "$go"
    sleep 3
}

echo "==> 3/4 正例：application.only = /.+/ （只有真的拿到非空焦点类才会命中）"
run_case positive '/.+/'
if [[ -f "$PROBE/positive" ]]; then
    echo "    ✓ 命中了应用专属 keymap"
    POS=0
else
    echo "    ✗ 未命中应用专属 keymap"
    grep -E 'application-client|application:' "$WORK/xremap-positive.log" | tail -3
    POS=1
fi
grep -m1 'application-client' "$WORK/xremap-positive.log" | sed 's/^/    /'

echo "==> 4/4 反例：application.only = /this-app-does-not-exist/ （必须落到兜底）"
run_case negative '/this-app-does-not-exist/'
if [[ -f "$PROBE/GENERIC-negative" ]]; then
    echo "    ✓ 正确落到兜底 keymap（说明过滤条件确实在被求值）"
    NEG=0
else
    echo "    ✗ 连兜底都没命中"
    NEG=1
fi

if [[ $POS -eq 0 && $NEG -eq 0 ]]; then
    if [[ "$BACKEND" == "dbus" ]]; then
        echo "✓ 通过：xremap 在 GNOME Wayland 上拿到了焦点应用（焦点后端：$BACKEND_LABEL），且 application 过滤生效"
    else
        echo "✓ 通过：xremap 经 socket 拿到了焦点信息（焦点后端：$BACKEND_LABEL，未经过真实 GNOME D-Bus 焦点桥），且 application 过滤生效"
    fi
    exit 0
fi
echo "✗ 失败（焦点后端：$BACKEND_LABEL）"
exit 1
