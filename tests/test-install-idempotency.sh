#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 幂等性验证：在同一个沙箱 HOME 里
#   0) 非 GNOME 桌面或 X11 会话时安装入口必须警告并中止（且不产生任何写入）；
#   1) 先做一次「干净安装」（init + install --no-fetch）并快照全部产物；
#   2) 人为制造旧状态：改坏的 unit、陈旧 socket、自启软链、扩展目录脏文件、
#      缺字段的旧版 config.json、以及一个已存在的旧引擎；
#   3) 重跑同样的安装流程，断言产物与干净安装逐字节一致，且旧残留全部消失。
# 全程用桩 systemctl/gsettings，且 HOME/XDG_* 都在沙箱内，不碰真实会话。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="mackey-focus@kotonex"
LEGACY_UUID="xremap-compat@mackey.local"
SANDBOX="$(mktemp -d "$HOME/.mackey-idem-test.XXXXXX")"
HB="$SANDBOX/home"
STUBS="$SANDBOX/stubs"
cleanup() { rm -rf "$SANDBOX"; }
trap cleanup EXIT

mkdir -p "$STUBS"
cat > "$STUBS/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat > "$STUBS/gsettings" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "get" ]]; then echo "[]"; fi
exit 0
EOF
chmod +x "$STUBS/systemctl" "$STUBS/gsettings"

failures=0
ok()   { printf '  ✓ %s\n' "$*"; }
fail() { failures=$((failures + 1)); printf '  ✗ %s\n' "$*"; }

# GitHub Actions 会导出 XDG_CONFIG_HOME 等指向 runner 的 HOME；只改 HOME 不足以隔离，
# 这里把全部 XDG 落点都钉在沙箱里（guard_home_path 仍然成立）。
run_mac_keys() {
    PATH="$STUBS:$PATH" HOME="$HB" \
        XDG_CONFIG_HOME="$HB/.config" XDG_DATA_HOME="$HB/.local/share" \
        XDG_CACHE_HOME="$HB/.cache" XDG_STATE_HOME="$HB/.local/state" \
        XDG_BIN_HOME="$HB/.local/bin" XDG_RUNTIME_DIR="$HB/run" \
        MACKEY_DOCS="$SANDBOX/行为清单.md" bash "$ROOT/bin/mackey" "$@"
}

snapshot() {
    python3 - "$HB" <<'PY'
import hashlib
import os
import sys

root = sys.argv[1]
rows = []
for dirpath, dirnames, filenames in os.walk(root):
    dirnames.sort()
    for name in sorted(filenames):
        path = os.path.join(dirpath, name)
        rel = os.path.relpath(path, root)
        if os.path.islink(path):
            rows.append(f"L {rel} -> {os.readlink(path)}")
        else:
            with open(path, "rb") as fh:
                digest = hashlib.sha256(fh.read()).hexdigest()
            rows.append(f"F {rel} {digest}")
print("\n".join(sorted(rows)))
PY
}

UNIT_DIR="$HB/.config/systemd/user"
ENGINE_UNIT="$UNIT_DIR/mackey-engine.service"
EXT_DIR="$HB/.local/share/gnome-shell/extensions/$UUID"
ENGINE_BIN="$HB/.local/share/mackey/bin/xremap"

# ---- 场景 0：非 GNOME 桌面 / X11 会话都必须拒绝安装 ----
echo "==> 1/5 非 GNOME 桌面与 X11 会话：安装入口警告并中止"
XDG_CURRENT_DESKTOP=KDE HOME="$HB" PATH="$STUBS:$PATH" \
    XDG_CONFIG_HOME="$HB/.config" XDG_DATA_HOME="$HB/.local/share" \
    XDG_CACHE_HOME="$HB/.cache" XDG_STATE_HOME="$HB/.local/state" \
    XDG_BIN_HOME="$HB/.local/bin" XDG_RUNTIME_DIR="$HB/run" \
    bash "$ROOT/install.sh" > "$SANDBOX/guard.out" 2>&1
guard_rc=$?
if [[ $guard_rc -ne 0 ]] && grep -q '仅支持 GNOME' "$SANDBOX/guard.out"; then
    ok "非 GNOME 会话被拒绝（rc=$guard_rc）"
else
    fail "非 GNOME 会话未被拒绝（rc=$guard_rc）"; sed -n '1,10p' "$SANDBOX/guard.out"
fi
XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=x11 HOME="$HB" PATH="$STUBS:$PATH" \
    XDG_CONFIG_HOME="$HB/.config" XDG_DATA_HOME="$HB/.local/share" \
    XDG_CACHE_HOME="$HB/.cache" XDG_STATE_HOME="$HB/.local/state" \
    XDG_BIN_HOME="$HB/.local/bin" XDG_RUNTIME_DIR="$HB/run" \
    bash "$ROOT/install.sh" > "$SANDBOX/guard-x11.out" 2>&1
x11_rc=$?
if [[ $x11_rc -ne 0 ]] && grep -q 'X11' "$SANDBOX/guard-x11.out"; then
    ok "X11 会话被拒绝（rc=$x11_rc）"
else
    fail "X11 会话未被拒绝（rc=$x11_rc）"; sed -n '1,10p' "$SANDBOX/guard-x11.out"
fi
XDG_SESSION_TYPE=x11 run_mac_keys install --no-fetch > "$SANDBOX/guard-manual.out" 2>&1
manual_rc=$?
if [[ $manual_rc -ne 0 ]] && grep -q 'X11' "$SANDBOX/guard-manual.out"; then
    ok "手动入口（bin/mackey install）同样拒绝 X11（rc=$manual_rc）"
else
    fail "手动入口未拒绝 X11（rc=$manual_rc）"; sed -n '1,10p' "$SANDBOX/guard-manual.out"
fi
if [[ ! -e "$HB/.config/mackey" && ! -e "$HB/.local/share/mackey" ]]; then
    ok "所有拒绝路径都没有产生写入"
else
    fail "拒绝后仍产生了写入"
fi

# ---- 场景 A：干净安装（预置一个「旧引擎」，模拟升级前已存在 xremap） ----
echo "==> 2/5 干净安装并快照"
mkdir -p "$(dirname "$ENGINE_BIN")" "$HB/run"
printf 'OLD-ENGINE' > "$ENGINE_BIN"
chmod +x "$ENGINE_BIN"
run_mac_keys init >/dev/null 2>&1
run_mac_keys install --no-fetch >/dev/null 2>&1
snapshot > "$SANDBOX/snap-clean.txt"
cp "$HB/.config/mackey/config.json" "$SANDBOX/config-clean.json"

[[ -x "$HB/.local/bin/mackey" ]] && ok "命令行入口已安装" || fail "命令行入口未安装"
if grep -q "$ENGINE_BIN" "$ENGINE_UNIT" && ! grep -q '/\.vendor/' "$ENGINE_UNIT"; then
    ok "引擎单元指向 XDG 引擎（不会被仓库 .vendor 抢走）"
else
    fail "引擎单元没有指向 XDG 引擎"; grep ExecStart "$ENGINE_UNIT"
fi

# ---- 制造旧状态 ----
echo "==> 3/5 制造旧状态（脏 unit / 陈积 socket / 自启软链 / 脏扩展 / 旧版 config）"
printf 'STALE\n' >> "$ENGINE_UNIT"
printf 'STALE\n' >> "$UNIT_DIR/mackey-focusd.service"
mkdir -p "$UNIT_DIR/default.target.wants"
ln -sf ../mackey-focusd.service "$UNIT_DIR/default.target.wants/mackey-focusd.service"
ln -sf ../mackey-engine.service "$UNIT_DIR/default.target.wants/mackey-engine.service"
[[ -L "$UNIT_DIR/default.target.wants/mackey-engine.service" ]] || fail "测试自身没造出自启软链"
touch "$HB/run/mackey-focus.sock"
echo 'stale' > "$EXT_DIR/stale.js"
mkdir -p "$(dirname "$EXT_DIR")/$LEGACY_UUID"
echo '// old' > "$(dirname "$EXT_DIR")/$LEGACY_UUID/extension.js"
echo '// broken' > "$EXT_DIR/extension.js"
python3 - "$HB/.config/mackey/config.json" <<'PY'
import json
import sys

path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))
for key in ("keypress_delay_ms", "_keypress_delay_ms_说明", "engine", "_engine_说明"):
    data.pop(key, None)
json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
PY

# ---- 场景 B：重跑安装流程 ----
echo "==> 4/5 重跑安装并比较"
run_mac_keys init >/dev/null 2>&1
run_mac_keys install --no-fetch >/dev/null 2>&1
snapshot > "$SANDBOX/snap-repeat.txt"

if diff -u "$SANDBOX/snap-clean.txt" "$SANDBOX/snap-repeat.txt" > "$SANDBOX/snap.diff"; then
    ok "重装产物与干净安装逐字节一致"
else
    fail "重装产物与干净安装不一致："; sed -n '1,40p' "$SANDBOX/snap.diff"
fi
[[ ! -e "$HB/run/mackey-focus.sock" ]] && ok "陈旧 socket 已清理" || fail "陈旧 socket 仍在"
if [[ ! -e "$UNIT_DIR/default.target.wants/mackey-focusd.service" \
   && ! -e "$UNIT_DIR/default.target.wants/mackey-engine.service" ]]; then
    ok "自启软链已清理"
else
    fail "自启软链仍在"
fi
[[ ! -e "$EXT_DIR/stale.js" ]] && ok "扩展目录陈旧文件已清理" || fail "扩展目录仍有陈旧文件"
[[ ! -e "$(dirname "$EXT_DIR")/$LEGACY_UUID" ]] && ok "改名前的旧扩展已被清理" || fail "旧扩展目录仍在"
grep -q 'STALE' "$ENGINE_UNIT" && fail "unit 仍含旧内容" || ok "unit 已按干净安装重写"

# ---- 场景 C：旧配置损坏时也应收敛到干净安装的结果 ----
echo "==> 5/5 损坏的旧配置：备份后重建，且内容与干净安装一致"
printf '{ not json' > "$HB/.config/mackey/config.json"
run_mac_keys init >/dev/null 2>&1
compgen -G "$HB/.config/mackey/config.json.bak-*" >/dev/null && ok "损坏配置已备份" || fail "未备份损坏配置"
if diff -q "$SANDBOX/config-clean.json" "$HB/.config/mackey/config.json" >/dev/null; then
    ok "重建后的 config 与干净安装一致"
else
    fail "重建后的 config 与干净安装不一致"; diff -u "$SANDBOX/config-clean.json" "$HB/.config/mackey/config.json" | head -20
fi

echo
if [[ $failures -eq 0 ]]; then
    echo "✓ 安装幂等性验证通过"
    exit 0
fi
echo "✗ $failures 项未通过"
exit 1
