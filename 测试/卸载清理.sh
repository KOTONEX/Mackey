#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
# 隔离带验证「一键卸载是否清理干净」：
#   - 把 HOME 指到 $HOME 下的临时沙箱（guard_home_path 仍满足），XDG_* 也随之落在沙箱里；
#   - 用桩 systemctl / gsettings 替代真实服务与 dconf，绝不动当前会话；
#   - 先造出卸载目标文件，再跑 `uninstall --清除配置 --确认执行`，断言全部消失；
#   - gsettings 桩会把完整 argv 记进日志：dry-run 不得 set，
#     真卸载须从 enabled-extensions 摘掉 UUID、并按备份还原每个键位。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="mackey-focus@kotonex"
LEGACY_UUID="xremap-compat@mackey.local"
SANDBOX="$(mktemp -d "$HOME/.mackey-uninstall-test.XXXXXX")"
cleanup() { rm -rf "$SANDBOX"; }
trap cleanup EXIT

HB="$SANDBOX/home"
STUBS="$SANDBOX/stubs"
mkdir -p "$STUBS"

cat > "$STUBS/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat > "$STUBS/gsettings" <<EOF
#!/usr/bin/env bash
# 桩：完整 argv 记日志；get 按 seed 文件应答（默认 []），set 只记录不改状态
printf '%s\n' "\$*" >> "$SANDBOX/gsettings.log"
if [[ "\${MACKEY_TEST_FAIL_RESTORE:-0}" == 1 && "\${1:-}" == set && "\${2:-}" == org.gnome.desktop.wm.keybindings ]]; then
    echo '模拟 GSettings 还原失败' >&2
    exit 1
fi
if [[ "\${1:-}" == "get" ]]; then
    schema="\${2:-}"; key="\${3:-}"
    if [[ -f "$SANDBOX/gsettings-seed" ]]; then
        while IFS=\$'\t' read -r s k v; do
            [[ "\$s" == "\$schema" && "\$k" == "\$key" ]] && { printf '%s\n' "\$v"; exit 0; }
        done < "$SANDBOX/gsettings-seed"
    fi
    echo "[]"
fi
exit 0
EOF

chmod +x "$STUBS/systemctl" "$STUBS/gsettings"

failures=0
ok()   { printf '  ✓ %s\n' "$*"; }
fail() { failures=$((failures + 1)); printf '  ✗ %s\n' "$*"; }

# ---- 造出全部卸载目标（沙箱内） ----
mkdir -p \
    "$HB/.local/bin" \
    "$HB/.config/mackey/backup" \
    "$HB/.config/systemd/user/default.target.wants" \
    "$HB/.local/share/mackey/bin" \
    "$HB/.local/share/gnome-shell/extensions/$UUID" \
    "$HB/.local/share/gnome-shell/extensions/$LEGACY_UUID" \
    "$HB/.cache/mackey/downloads" \
    "$HB/.local/state/mackey" \
    "$HB/run"

for f in config.json xremap.json relocations.json; do
    echo '{}' > "$HB/.config/mackey/$f"
done
# 备份里必须有真内容，才能验证卸载会把每个键位下发还原
cat > "$HB/.config/mackey/backup/gsettings.json" <<'EOF'
{
  "org.gnome.desktop.wm.keybindings switch-applications": ["<Super>Tab"],
  "org.gnome.shell.keybindings toggle-overview": ["<Super>s"]
}
EOF
cp "$HB/.config/mackey/backup/gsettings.json" "$SANDBOX/backup-expected.json"
# GNOME 预置状态：扩展已启用，且启用列表里还有别人的条目
printf '%s\t%s\t%s\n' \
    "org.gnome.shell" "enabled-extensions" "['$UUID', 'other@example.com']" \
    > "$SANDBOX/gsettings-seed"
echo '{}' > "$HB/.config/systemd/user/mackey-focusd.service"
echo '{}' > "$HB/.config/systemd/user/mackey-engine.service"
ln -s ../mackey-focusd.service "$HB/.config/systemd/user/default.target.wants/mackey-focusd.service"
printf '\x7fELF' > "$HB/.local/share/mackey/bin/xremap"
chmod +x "$HB/.local/share/mackey/bin/xremap"
echo '//' > "$HB/.local/share/gnome-shell/extensions/$UUID/extension.js"
echo '// old' > "$HB/.local/share/gnome-shell/extensions/$LEGACY_UUID/extension.js"
echo 'zip' > "$HB/.cache/mackey/downloads/xremap.zip"
echo '{}' > "$HB/.local/state/mackey/engine.json"
printf '#!/usr/bin/env bash\nexec "%s/命令/mackey" "$@"\n' "$ROOT" > "$HB/.local/bin/mackey"
chmod +x "$HB/.local/bin/mackey"
touch "$HB/run/mackey-focus.sock"

TARGETS=(
    "$HB/.config/systemd/user/mackey-focusd.service"
    "$HB/.config/systemd/user/mackey-engine.service"
    "$HB/.config/systemd/user/default.target.wants/mackey-focusd.service"
    "$HB/.local/bin/mackey"
    "$HB/.local/share/gnome-shell/extensions/$UUID"
    "$HB/.local/share/gnome-shell/extensions/$LEGACY_UUID"
    "$HB/.local/share/mackey"
    "$HB/.cache/mackey"
    "$HB/.local/state/mackey"
    "$HB/run/mackey-focus.sock"
    "$HB/.config/mackey"
)

# GitHub Actions 会导出 XDG_CONFIG_HOME 等指向 runner 的 HOME；只改 HOME 不足以隔离，
# 这里把全部 XDG 落点都钉在沙箱里（guard_home_path 仍然成立）。
run_mac_keys() {
    PATH="$STUBS:$PATH" HOME="$HB" \
        XDG_CONFIG_HOME="$HB/.config" XDG_DATA_HOME="$HB/.local/share" \
        XDG_CACHE_HOME="$HB/.cache" XDG_STATE_HOME="$HB/.local/state" \
        XDG_BIN_HOME="$HB/.local/bin" XDG_RUNTIME_DIR="$HB/run" \
        MACKEY_YES=1 \
        bash "$ROOT/命令/mackey" "$@"
}

echo "==> 1/4 --预演 只列清单：不删任何东西，也不调用 gsettings set"
run_mac_keys 卸载 --清除配置 --预演 >/dev/null 2>&1
missing=0
for t in "${TARGETS[@]}"; do [[ -e "$t" ]] || missing=$((missing + 1)); done
if [[ $missing -eq 0 ]]; then ok "--预演 后全部目标仍在"; else fail "--预演 删掉了 $missing 项"; fi
if [[ -f "$SANDBOX/gsettings.log" ]] && grep -q '^set ' "$SANDBOX/gsettings.log"; then
    fail "--预演 触发了 gsettings set：$(grep -m1 '^set ' "$SANDBOX/gsettings.log")"
else
    ok "--预演 没有触发任何 gsettings set"
fi

echo "==> 2/4 uninstall --清除配置 --确认执行 清理干净"
if run_mac_keys 卸载 --清除配置 --确认执行 > "$SANDBOX/uninstall.log" 2>&1; then
    ok "卸载退出码为 0"
else
    fail "卸载退出码非 0"; cat "$SANDBOX/uninstall.log"
fi
left=()
for t in "${TARGETS[@]}"; do [[ -e "$t" ]] && left+=("$t"); done
if [[ ${#left[@]} -eq 0 ]]; then ok "全部目标已删除（含配置/缓存/状态/socket）"; else fail "仍残留：${left[*]}"; fi

echo "==> 3/4 GNOME 侧清理：摘除启用记录 + 按备份还原键位"
set_line="$(grep -m1 '^set org.gnome.shell enabled-extensions ' "$SANDBOX/gsettings.log" 2>/dev/null || true)"
if [[ -n "$set_line" && "$set_line" != *"$UUID"* && "$set_line" == *"other@example.com"* ]]; then
    ok "已从 org.gnome.shell enabled-extensions 摘除扩展 UUID（保留其它条目）"
else
    fail "扩展 UUID 未被正确摘除：${set_line:-（日志里没有对应的 set 调用）}"
fi

restore_missing=0
while IFS= read -r expected; do
    if ! grep -Fxq "$expected" "$SANDBOX/gsettings.log"; then
        echo "未按备份还原：$expected" >&2
        restore_missing=$((restore_missing + 1))
    fi
done < <(jq -r 'to_entries[] | (.key | split(" ")) as $id |
    "set " + $id[0] + " " + $id[1] + " [" + (.value | map(@sh) | join(", ")) + "]"' \
    "$SANDBOX/backup-expected.json")
if [[ $restore_missing -eq 0 ]]
then
    ok "备份的 GNOME 键位全部下发还原"
else
    fail "备份键位没有被完整还原"
fi

echo "==> 4/4 还原失败时 --清除配置 保留原始备份与配置"
mkdir -p "$HB/.config/mackey/backup"
cp "$SANDBOX/backup-expected.json" "$HB/.config/mackey/backup/gsettings.json"
printf '{}\n' > "$HB/.config/mackey/config.json"
if MACKEY_TEST_FAIL_RESTORE=1 run_mac_keys 卸载 --清除配置 --确认执行 > "$SANDBOX/restore-failed.log" 2>&1; then
    fail "还原失败没有导致卸载失败"
elif cmp -s "$SANDBOX/backup-expected.json" "$HB/.config/mackey/backup/gsettings.json" && [[ -f "$HB/.config/mackey/config.json" ]]; then
    ok "还原失败正确退出，原始备份与配置保留"
else
    fail "还原失败后丢失了备份或配置"
fi

echo
if [[ $failures -eq 0 ]]; then
    echo "✓ 卸载清理验证通过"
    exit 0
fi
echo "✗ $failures 项未通过"
exit 1
