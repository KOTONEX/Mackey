#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""Mackey 生成器：把行为清单 + GNOME 实际键位占用情况，编译成 xremap 配置。

设计要点
--------
1. **冲突是探测出来的，不是假设出来的**：直接读 dconf（wm/shell/mutter/media-keys +
   自定义快捷键），得到「GNOME 已占用」的加速键集合。GNOME 版本之间差异很大
   （例如本机 GNOME 51 的 Super+↑/↓ 是空的，而更老版本是「最大化」），
   硬编码必然过时。
2. **能翻译就翻译，翻译会破坏 GNOME 功能就迁移（relocate）**：relocate 只在本机
   真的存在该冲突时才写入 gsettings，并且由 bin/mackey 负责备份/还原。
3. **终端单独一套**：终端里 Ctrl+C/Ctrl+Z/Ctrl+D 是信号与 EOF，必须显式改写或吞掉；
   终端配置档不参与「泛化翻译」，避免误伤。

只依赖 Python 标准库。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional, TypedDict, Union, overload

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def xdg_config_home() -> Path:
    """默认落点必须和 bin/mackey 的 ${XDG_CONFIG_HOME:-$HOME/.config} 一致，
    否则 init 写的 config.json 与 generate 读写的位置会分叉（布局被静默忽略）。"""
    return Path(os.environ.get("XDG_CONFIG_HOME") or (Path.home() / ".config"))

# xremap/evdev 的合法键名（取自 evdev crate 的 scancodes 枚举），用于在生成前校验，
# 避免把 X11 风格的键名（如 bracketleft / Page_Up）写进配置导致引擎起不来。
MODIFIERS = {"C", "S", "A", "SUPER", "M", "W", "CTRL", "SHIFT", "ALT", "HYPER"}
VALID_KEYS = {
    "HOME", "END", "PAGEUP", "PAGEDOWN", "LEFT", "RIGHT", "UP", "DOWN", "DELETE",
    "BACKSPACE", "INSERT", "ENTER", "TAB", "SPACE", "ESC", "GRAVE", "MINUS", "EQUAL",
    "LEFTBRACE", "RIGHTBRACE", "BACKSLASH", "SEMICOLON", "APOSTROPHE", "COMMA",
    "DOT", "SLASH", "PRINT", "SYSRQ", "SUPER_L", "SUPER_R", "LEFTCTRL", "LEFTALT",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "F13", "F14", "F15", "F24",
}
VALID_KEYS |= set("ABCDEFGHIJKLMNOPQRSTUVWXYZ")
VALID_KEYS |= {str(d) for d in range(10)}

# GTK/X11 加速键里的键名 → evdev 键名
GTK_KEY_MAP = {
    "LEFT": "LEFT", "RIGHT": "RIGHT", "UP": "UP", "DOWN": "DOWN",
    "PAGE_UP": "PAGEUP", "PAGE_DOWN": "PAGEDOWN", "PRIOR": "PAGEUP", "NEXT": "PAGEDOWN",
    "BRACKETLEFT": "LEFTBRACE", "BRACKETRIGHT": "RIGHTBRACE",
    "PERIOD": "DOT", "COMMA": "COMMA", "GRAVE": "GRAVE", "ABOVE_TAB": "GRAVE",
    "SPACE": "SPACE", "BACKSPACE": "BACKSPACE", "DELETE": "DELETE",
    "HOME": "HOME", "END": "END", "TAB": "TAB", "RETURN": "ENTER", "ENTER": "ENTER",
    "ESCAPE": "ESC", "ESC": "ESC", "PRINT": "PRINT", "SYSRQ": "PRINT",
    "MINUS": "MINUS", "EQUAL": "EQUAL", "SLASH": "SLASH", "BACKSLASH": "BACKSLASH",
    "SEMICOLON": "SEMICOLON", "APOSTROPHE": "APOSTROPHE", "INSERT": "INSERT",
}
GTK_MOD_MAP = {
    "PRIMARY": "C", "CONTROL": "C", "CTRL": "C", "SHIFT": "S", "ALT": "A", "MOD1": "A",
    "META": "Super", "SUPER": "Super", "MOD4": "Super", "HYPER": "Hyper",
}
MOD_ORDER = {"C": 0, "S": 1, "A": 2, "SUPER": 3, "HYPER": 4}

# 反向表：evdev 键名 → GTK 加速键里的键名（写回 gsettings 时用）
EVDEV_TO_GTK_KEY = {
    "PAGEUP": "Page_Up", "PAGEDOWN": "Page_Down", "LEFTBRACE": "bracketleft",
    "RIGHTBRACE": "bracketright", "DOT": "period", "COMMA": "comma",
    "BACKSPACE": "BackSpace", "ENTER": "Return", "ESC": "Escape", "PRINT": "Print",
    "LEFT": "Left", "RIGHT": "Right", "UP": "Up", "DOWN": "Down",
    "HOME": "Home", "END": "End", "DELETE": "Delete", "INSERT": "Insert",
    "TAB": "Tab", "SPACE": "space", "GRAVE": "grave", "MINUS": "minus",
    "EQUAL": "equal", "SLASH": "slash", "BACKSLASH": "backslash",
    "SEMICOLON": "semicolon", "APOSTROPHE": "apostrophe",
}
EVDEV_TO_GTK_MOD = {"C": "Control", "S": "Shift", "A": "Alt", "SUPER": "Super"}

# ---- 跨进程边界的数据形状（checklist.json / 生成的配置 / 文档渲染的中间结构） ----

TargetSpec = Union[str, list[str]]


class ChecklistEntry(TypedDict, total=False):
    """config/checklist.json 里的一条键位（纯 GNOME 条目没有 trigger/targets）。"""

    id: str
    mac: str
    desc: str
    trigger: str
    targets: dict[str, str]
    status: str
    basis: str
    policy: str
    conflict: str
    note: str
    relocate: str
    generated: bool


# `from` / `to` 是 Python 关键字，class 语法写不出来，只能函数式声明。
RelocationDef = TypedDict("RelocationDef", {
    "id": str,
    "label": str,
    "schema": str,
    "key": str,
    "from": TargetSpec,
    "to": TargetSpec,
    "triggered_by": str,
}, total=False)


class ChecklistProfile(TypedDict):
    match: list[str]


class ChecklistSweep(TypedDict, total=False):
    enabled: bool
    kinds: list[str]
    exclude_ids: list[str]
    exclude_triggers: list[str]


class SwallowSpec(TypedDict, total=False):
    enabled: bool
    triggers: list[str]
    except_profiles: list[str]
    except_apps: list[str]


class TerminalSwallowSpec(TypedDict, total=False):
    """终端配置档内额外吞掉的组合（例如物理 Ctrl+Shift+C/V）。"""

    enabled: bool
    triggers: list[str]


class ChecklistMeta(TypedDict):
    swallow_key: str


class Checklist(TypedDict):
    entries: list[ChecklistEntry]
    meta: ChecklistMeta
    profiles: dict[str, ChecklistProfile]
    relocations: list[RelocationDef]
    swallow_ctrl: SwallowSpec
    swallow_terminal: TerminalSwallowSpec
    sweep: ChecklistSweep


class BindingRow(TypedDict):
    """GNOME 已占用键位的逐键归属行（探测自 dconf）。"""

    ident: str
    accels: list[str]


RelocationPlan = TypedDict("RelocationPlan", {
    "schema": str,
    "key": str,
    "label": str,
    "from": str,
    "to": str,
    "from_list": list[str],
    "targets": list[str],
    "old": list[str],
    "new": list[str],
    "triggered_by": Optional[str],
})


class Migration(TypedDict):
    ident: str
    old: list[str]
    now: list[str]


class UserConfig(TypedDict, total=False):
    modifier_layout: str
    keypress_delay_ms: int
    throttle_ms: int
    notifications: bool
    device: dict[str, Any]


class XremapKeymap(TypedDict, total=False):
    name: str
    application: dict[str, list[str]]
    remap: dict[str, Any]
    device: dict[str, Any]


class XremapConfig(TypedDict, total=False):
    keypress_delay_ms: int
    throttle_ms: int
    notifications: bool
    modmap: list[dict[str, Any]]
    keymap: list[XremapKeymap]


@overload
def norm(combo: str) -> str: ...
@overload
def norm(combo: list[str]) -> list[str]: ...
def norm(combo: Any) -> Any:
    """把组合键规范化：统一大写、修饰键按固定顺序排列（两侧比较必须同构）。

    `Super-S-a` / `s-Super-a` → `S-SUPER-A`
    """
    if isinstance(combo, list):
        return [norm(c) for c in combo]
    if not isinstance(combo, str):
        return combo
    parts = combo.split("-")
    mods, key = parts[:-1], parts[-1]
    mods = sorted({m.upper() for m in mods}, key=lambda m: MOD_ORDER.get(m, 9))
    return "-".join(mods + [key.upper()])


def to_gtk_accel(combo: str) -> str:
    """`S-SUPER-A` → `<Shift><Super>a`（写进 dconf 的形式）。"""
    parts = combo.split("-")
    mods, key = parts[:-1], parts[-1]
    if key in EVDEV_TO_GTK_KEY:
        gtk_key = EVDEV_TO_GTK_KEY[key]
    elif len(key) == 1:
        gtk_key = key.lower()
    else:
        gtk_key = key.capitalize()
    return "".join(f"<{EVDEV_TO_GTK_MOD.get(m, m)}>" for m in mods) + gtk_key

GNOME_SCHEMAS = [
    "org.gnome.desktop.wm.keybindings",
    "org.gnome.shell.keybindings",
    "org.gnome.mutter.keybindings",
    "org.gnome.settings-daemon.plugins.media-keys",
]


class ProbeUnavailable(RuntimeError):
    """环境里没有 gsettings：不是 GNOME 会话，没有可探测的 GNOME 键位。"""


class ProbeError(RuntimeError):
    """gsettings 存在但探测失败：不能当作「没有冲突」继续。"""


def run(cmd: list[str]) -> tuple[int, str, str]:
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=20)
        return p.returncode, p.stdout.strip(), p.stderr.strip()
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 1, "", str(exc)


def canon_key(name: str) -> str:
    n = name.upper()
    if n in GTK_KEY_MAP:
        return GTK_KEY_MAP[n]
    if n.startswith("KP_"):
        return n
    if n.startswith("XF86") or n.startswith("KEY_"):
        return n.replace("KEY_", "", 1)
    return n


def canon_accel(accel: str) -> str | None:
    """`<Super><Shift>a` / `<Control><Alt>Left` → `Super-S-a`（用于比较）。"""
    if not accel:
        return None
    mods: list[str] = []
    rest = accel.strip()
    while rest.startswith("<"):
        end = rest.find(">")
        if end < 0:
            return None
        mods.append(GTK_MOD_MAP.get(rest[1:end].upper(), rest[1:end].upper()))
        rest = rest[end + 1:]
    if not rest:
        return None
    key = canon_key(rest)
    mods = sorted(set(mods), key=lambda m: MOD_ORDER.get(m.upper(), 9))
    return "-".join(mods + [key]).upper()


def split_gtk_list(raw: str) -> list[str]:
    """解析 gsettings 输出的 ['<Super>a', '<Shift><Super>space'] 或单个字符串。"""
    raw = raw.strip()
    if raw in ("@as []", "[]", "''", ""):
        return []
    if raw.startswith("[") and raw.endswith("]"):
        raw = raw[1:-1]
    out = []
    for part in re.findall(r"'([^']*)'", raw):
        if part:
            out.append(part)
    return out


def probe_gnome_occupied() -> tuple[set[str], dict[str, str], list[BindingRow]]:
    """返回 (已占用加速键集合, 原始键位表, 逐键归属行)。

    rows 每项形如 {"ident": "org.gnome.shell.keybindings toggle-quick-settings",
    "accels": ["SUPER-S"]}，供「GNOME 键位归属」一节逐键标注：被接管 / 已迁移 /
    迁移落点 / GNOME 保留。
    """
    occupied: set[str] = set()
    table: dict[str, str] = {}
    rows: list[BindingRow] = []

    # 探测失败不能静默当成「没有冲突」：那会让引擎直接覆盖 GNOME 快捷键而没有迁移。
    if not shutil.which("gsettings"):
        raise ProbeUnavailable("找不到 gsettings 命令（看起来不是 GNOME 会话）")

    for schema in GNOME_SCHEMAS:
        rc, out, err = run(["gsettings", "list-recursively", schema])
        if rc != 0:
            raise ProbeError(f"gsettings list-recursively {schema} 失败：{err or f'rc={rc}'}")
        for line in out.splitlines():
            parts = line.split(" ", 2)
            if len(parts) < 3:
                continue
            key, value = parts[1], parts[2]
            ident = f"{schema} {key}"
            table[ident] = value
            if key.endswith("-static") or key == "custom-keybindings":
                continue
            accels = [c for c in (canon_accel(a) for a in split_gtk_list(value)) if c]
            if accels:
                rows.append({"ident": ident, "accels": accels})
            occupied.update(accels)

    # 自定义快捷键（custom-keybindings）
    for path in split_gtk_list(table.get(
            "org.gnome.settings-daemon.plugins.media-keys custom-keybindings", "")):
        sub = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:" + path
        rc, out, err = run(["gsettings", "list-recursively", sub])
        if rc != 0:
            # 路径不存在时这条自定义快捷键就没有绑定，不存在冲突风险；但要说明，不静默。
            print(f"! 跳过自定义快捷键 {path}：{err or f'rc={rc}'}", file=sys.stderr)
            continue
        name: str = ""
        accels = []
        for line in out.splitlines():
            parts = line.split(" ", 2)
            if len(parts) != 3:
                continue
            if parts[1] == "name":
                parsed = split_gtk_list(parts[2])
                name = parsed[0] if parsed else ""
            elif parts[1] == "binding":
                accels = [c for c in (canon_accel(a) for a in split_gtk_list(parts[2])) if c]
        if accels:
            rows.append({"ident": f"自定义快捷键：{name or path}", "accels": accels})
        occupied.update(accels)
    return occupied, table, rows


# GNOME 官方截图动作 → 探测用的 ident 候选（GNOME 42+ 在 shell schema，老版本在 media-keys）。
# 三种模式与清单对应：选区 = 交互式 UI（默认选区）、屏幕 = 全屏直接落盘、窗口 = 截当前窗口。
SCREENSHOT_ACTIONS = {
    "screenshot-region": [
        "org.gnome.shell.keybindings show-screenshot-ui",
    ],
    "screenshot-all": [
        "org.gnome.shell.keybindings screenshot",
        "org.gnome.settings-daemon.plugins.media-keys screenshot",
    ],
    "screenshot-window": [
        "org.gnome.shell.keybindings screenshot-window",
        "org.gnome.settings-daemon.plugins.media-keys window-screenshot",
    ],
}


def derived_screenshot_targets(table: dict[str, str], blocked: set[str],
                               allow: set[str] | None = None) -> dict[str, str]:
    """从 dconf 实测的 GNOME 截图绑定推导输出组合（每个动作取第一个有效加速键）。

    硬编码 Shift+Print/Print 只在默认键位的机器上成立：发行版或用户把
    `screenshot` / `show-screenshot-ui` 改绑到别的组合后，⌘⇧3/4/5 会静默失效。
    `blocked` 是清单里会被本工具吞掉的触发键：输出成这种组合没有意义
    （GNOME 侧没有对应绑定，按键会被我们自己拦下），此时退回清单默认值。
    例外是 `allow`（截图条目自己的触发键）：xremap 不会读自己合成的按键，
    所以「GNOME 截图键 == 我们的触发键」这种自指映射反而是可用的。
    """
    allow = allow or set()
    out: dict[str, str] = {}
    for action, idents in SCREENSHOT_ACTIONS.items():
        for ident in idents:
            accels = [canon_accel(a) for a in split_gtk_list(table.get(ident, ""))]
            accel = next((a for a in accels if a), None)
            if accel is None:
                continue
            if check_key_names({accel}):
                continue
            if accel in blocked and accel not in allow:
                continue
            out[action] = accel
            break
    return out


def check_occupied_conflicts(consuming: set[str], occupied: set[str],
                             planned_from: set[str]) -> list[str]:
    """会吞键的触发键若与 GNOME 已占用键位重合，必须有对应的迁移计划。

    否则会出现「静默抢占」：清单以为 GNOME 功能已经让路，实际 dconf 还占着这个组合
    （典型场景是迁移落点被改过、但机器上还是旧落点）。
    """
    return sorted(consuming & occupied - planned_from)


def check_key_names(names: set[str]) -> list[str]:
    bad = []
    for name in names:
        if not name:
            continue
        parts = name.split("-")
        for p in parts[:-1]:
            if p not in MODIFIERS:
                bad.append(f"{name}（修饰键 {p} 不认识）")
        if parts[-1].upper() not in VALID_KEYS:
            bad.append(f"{name}（键名 {parts[-1]} 不是合法 evdev 名）")
    return bad


def check_basis(checklist: Checklist) -> list[str]:
    """校验每条显式条目的 basis 分类，防止「键位表」与真实行为脱节。

    macos = 本工具改写（policy translate/auto 且 targets 非空）；
    gnome = 保持 GNOME 默认（其余全部）。
    """
    bad = []
    for e in checklist["entries"]:
        basis = e.get("basis")
        if basis not in ("macos", "gnome"):
            bad.append(f"{e['id']}：basis={basis!r}（必须是 macos 或 gnome）")
            continue
        expected = "macos" if (
            e.get("policy") in ("translate", "auto") and e.get("targets")
        ) else "gnome"
        if basis != expected:
            bad.append(
                f"{e['id']}：basis={basis} 与 policy={e.get('policy')}/"
                f"targets={'有' if e.get('targets') else '无'} 不一致（应为 {expected}）"
            )
    return bad


def check_relocate_refs(checklist: Checklist) -> list[str]:
    """条目上的 `relocate` 必须指向真实存在的迁移定义。

    这个字段用于人工对照「哪个 GNOME 功能让了路」；不校验的话，
    改了 relocations 的 id 而忘了条目，漂移会一直静默存在。
    """
    ids = {r["id"] for r in checklist.get("relocations", [])}
    bad = []
    for e in checklist["entries"]:
        ref = e.get("relocate")
        if ref and ref not in ids:
            bad.append(f"{e['id']}：relocate={ref} 不在 relocations 里")
    return bad


def parse_combo(combo: str) -> tuple[list[str], str]:
    parts = combo.split("-")
    return parts[:-1], parts[-1]


def build_remap(entries: list[ChecklistEntry], profile: str, swallow: str) -> dict[str, Any]:
    remap: dict[str, Any] = {}
    for e in entries:
        target: TargetSpec | None = e["targets"].get(profile) or e["targets"].get("default")
        if target is None:
            continue
        if isinstance(target, str) and "," in target:
            target = [t.strip() for t in target.split(",")]
        target = norm(target)
        if target == norm(swallow) or target == [norm(swallow)]:
            target = norm(swallow)
        remap[norm(e["trigger"])] = target
    return remap


def swallow_except_matchers(checklist: Checklist) -> list[str]:
    """替换模式的例外名单：终端配置档的匹配串 + 内嵌终端应用（except_apps）。

    这些应用里 Ctrl+C/X/V/Z/A 原样放行（SIGINT、readline、IDE 内嵌终端）。
    """
    spec = checklist.get("swallow_ctrl", {})
    matchers: list[str] = []
    for profile in spec.get("except_profiles", []):
        profile_spec = checklist.get("profiles", {}).get(profile)
        if profile_spec:
            matchers += profile_spec.get("match", [])
    matchers += spec.get("except_apps", [])
    return matchers


def sweep_entries(checklist: Checklist, occupied: set[str], explicit: set[str],
                  reserved: set[str]) -> list[ChecklistEntry]:
    """显式的清单条目之外的 ⌘+字母/数字，做泛化翻译。"""
    spec = checklist.get("sweep", {})
    if not spec.get("enabled"):
        return []
    kinds = set(spec.get("kinds", []))
    skipped = set(spec.get("exclude_ids", []))
    skipped_triggers = {norm(t) for t in spec.get("exclude_triggers", [])}
    out: list[ChecklistEntry] = []
    for e in checklist["entries"]:
        if e["id"] in skipped:
            continue
        explicit.add(norm(e["trigger"]))
    for letter in "ABCDEFGHIJKLMNOPQRSTUVWXYZ":
        for kind, trigger, target in (
            ("letter", norm(f"Super-{letter}"), f"C-{letter}"),
            ("letter-shift", norm(f"Super-S-{letter}"), f"C-S-{letter}"),
        ):
            if kind not in kinds:
                continue
            if trigger in explicit or trigger in occupied or trigger in reserved \
                    or trigger in skipped_triggers:
                continue  # 显式条目 / GNOME 已占用 / 已迁移给 GNOME 的键位 / 显式排除，都要避开
            out.append({"id": f"sweep-{trigger}",
                        "mac": f"{'⇧⌘' if trigger.startswith('S-SUPER-') else '⌘'}{letter}",
                        "desc": "（泛化翻译）", "trigger": trigger,
                        "targets": {"default": target}, "status": "mapped",
                        "basis": "macos", "policy": "translate", "generated": True,
                        "note": "自动生成的兜底映射：⌘ 等价于 Ctrl。"})
    for digit in "123456789":
        trigger = norm(f"SUPER-S-{digit}")
        if "digit-shift" not in kinds or trigger in explicit \
                or trigger in occupied or trigger in reserved \
                or trigger in skipped_triggers:
            continue
        out.append({"id": f"sweep-{trigger}", "mac": f"⌘⇧{digit}", "desc": "（泛化翻译）",
                    "trigger": trigger, "targets": {"default": f"C-S-{digit}"},
                    "status": "mapped", "basis": "macos", "policy": "translate",
                    "generated": True, "note": "自动生成的兜底映射。"})
    return out


def relocation_plan(checklist: Checklist, occupied: set[str],
                    table: dict[str, str]) -> list[RelocationPlan]:
    """只为「本机真的存在」的冲突生成迁移计划。"""
    plan: list[RelocationPlan] = []
    for reloc in checklist.get("relocations", []):
        keys: list[str] = []
        if ".." in reloc["key"]:
            head, _, tail = reloc["key"].partition("..")
            prefix = head.rstrip("0123456789")
            start, end = int(head[len(prefix):]), int(tail)
            keys = [f"{prefix}{n}" for n in range(start, end + 1)]
        elif "|" in reloc["key"]:
            keys = reloc["key"].split("|")
        else:
            keys = [reloc["key"]]

        from_specs = reloc["from"] if isinstance(reloc["from"], list) else [reloc["from"]]
        to_specs = reloc["to"] if isinstance(reloc["to"], list) else [reloc["to"]]
        if len(from_specs) != len(to_specs):
            raise SystemExit(f"✗ 迁移定义 {reloc['id']}：from/to 数量不一致")

        for key in keys:
            raw = table.get(f"{reloc['schema']} {key}")
            if raw is None:
                continue
            accels = split_gtk_list(raw)
            canon = [canon_accel(a) for a in accels]
            replacements: dict[str, str] = {}
            moved_from: list[str] = []
            moved_to: list[str] = []
            for from_spec, to_spec in zip(from_specs, to_specs):
                # `{D}` 占位：按键名后缀（left/right/up/down）逐条替换，
                # 用于 move-to-monitor-{left,right,up,down} 这类「同一功能、四个方向」的键位。
                if "{D}" in from_spec:
                    direction = key.rsplit("-", 1)[-1].upper()
                    from_spec = from_spec.replace("{D}", direction)
                    to_spec = to_spec.replace("{D}", direction)
                from_accel = norm(from_spec)
                to_accel = norm(to_spec)
                if ".." in from_accel:
                    from_accel = from_accel.replace("..", "").rstrip("0123456789") + key[-1]
                    to_accel = to_accel.replace("..", "").rstrip("0123456789") + key[-1]
                if from_accel not in canon:
                    continue  # 本机没占用这个组合，不需要迁移
                replacements[from_accel] = to_accel
                moved_from.append(from_accel)
                moved_to.append(to_accel)
            if not replacements:
                continue
            new_accels = [to_gtk_accel(replacements[c]) if c in replacements else a
                          for a, c in zip(accels, canon)]
            plan.append({
                "schema": reloc["schema"], "key": key, "label": reloc["label"],
                "from": "、".join(moved_from), "to": "、".join(moved_to),
                "from_list": moved_from, "targets": moved_to,
                "old": accels, "new": new_accels,
                "triggered_by": reloc.get("triggered_by"),
            })
    return plan


def pretty(combo: Any) -> str:
    """`SUPER-S-EQUAL` → `⌘⇧+`，`C-LEFT` → `⌃←`（只用于文档展示）。"""
    if combo is None:
        return "—"
    if isinstance(combo, (list, tuple)):
        return " → ".join(pretty(c) for c in combo)
    if isinstance(combo, str) and "," in combo:
        return " → ".join(pretty(c.strip()) for c in combo.split(","))
    sym = {"SUPER": "⌘", "S": "⇧", "A": "⌥", "C": "⌃", "M": "⌥", "HYPER": "✥"}
    key_sym = {"LEFT": "←", "RIGHT": "→", "UP": "↑", "DOWN": "↓", "HOME": "Home",
               "END": "End", "PAGEUP": "PgUp", "PAGEDOWN": "PgDn", "BACKSPACE": "⌫",
               "DELETE": "⌦", "ENTER": "↩", "SPACE": "Space", "TAB": "⇥",
               "LEFTBRACE": "[", "RIGHTBRACE": "]", "EQUAL": "＋", "MINUS": "-",
               "COMMA": ",", "DOT": ".", "GRAVE": "`", "SUPER_L": "Super(单击)",
               "SLASH": "/", "BACKSLASH": "\\",
               "PRINT": "Print", "F24": "F24(吞掉)"}
    parts = str(combo).upper().split("-")
    mods, key = parts[:-1], parts[-1]
    if not mods:
        return key_sym.get(key, key)
    rank = {"C": 0, "A": 1, "M": 1, "HYPER": 2, "S": 3, "SUPER": 4}
    mods = sorted(mods, key=lambda m: rank.get(m, 9))
    return "".join(sym.get(m, m + "+") for m in mods) + key_sym.get(key, key)


def load_migrations(backup_path: Path, table: dict[str, str]) -> list[Migration]:
    """从备份推算「已经执行过的 GNOME 键位迁移」。

    备份里存的是改动前的值；若当前 dconf 的值与它不同，说明这条功能已被迁移。
    这样即使迁移早已执行（当前探测不到冲突），文档里的「完整键位表」仍能如实标注。
    """
    if not backup_path.exists():
        return []
    try:
        backup = json.loads(backup_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return []
    out: list[Migration] = []
    for ident, old_accels in backup.items():
        current = table.get(ident)
        if current is None:
            continue
        old = [c for c in (canon_accel(a) for a in old_accels) if c]
        now = [c for c in (canon_accel(a) for a in split_gtk_list(current)) if c]
        removed = [a for a in old if a not in now]
        added = [a for a in now if a not in old]
        if removed or added:
            out.append({"ident": ident, "old": removed, "now": added})
    return out


def render_markdown(entries: list[ChecklistEntry], plan: list[RelocationPlan],
                    rows: list[BindingRow], migrations: list[Migration],
                    swallow_spec: SwallowSpec,
                    terminal_swallow_spec: TerminalSwallowSpec) -> str:
    """渲染文档：先按归属（macOS / GNOME 默认）分类，再逐键展开。"""
    reloc_ids = {p["triggered_by"] for p in plan if p["triggered_by"]}
    plan_from: dict[str, str] = {}
    plan_to: set[str] = set()
    for p in plan:
        for src, dst in zip(p.get("from_list", [p["from"]]), p.get("targets", [p["to"]])):
            plan_from[src] = dst
            plan_to.add(dst)
    mig_old = {a: m for m in migrations for a in m["old"]}
    mig_now = {a for m in migrations for a in m["now"]}
    consuming = {norm(e["trigger"]) for e in entries if e.get("trigger") and e.get("targets")}
    status_label = {
        "native": "🟢 原生等价", "mapped": "🔵 需映射", "relocated": "🟠 需迁移 GNOME 键位",
        "approx": "🟡 近似", "optin": "⚪ 可选（默认关）", "impossible": "🔴 无法模拟",
    }
    gnome_reason = {
        "native": "原生等价（GNOME 默认即 macOS 语义）",
        "approx": "近似放行（GNOME 默认是最接近的行为）",
        "relocated": "已迁移（功能保留在 GNOME 侧，换到新组合）",
        "optin": "可选（默认不接管）",
        "impossible": "无法模拟（GNOME 无对应概念）",
    }
    explicit = [e for e in entries if not e.get("generated")]
    generated = [e for e in entries if e.get("generated")]
    macos_entries = [e for e in explicit if e.get("basis") == "macos"]
    gnome_entries = [e for e in explicit if e.get("basis") == "gnome"]
    total_accels = sum(len(r["accels"]) for r in rows)
    unique_accels = len({a for r in rows for a in r["accels"]})
    swallow_triggers = [norm(t) for t in (swallow_spec or {}).get("triggers", [])]
    swallow_on = bool((swallow_spec or {}).get("enabled")) and bool(swallow_triggers)

    lines = [
        "# 行为清单（自动生成）",
        "",
        "> 由 `config/checklist.json` + 本机 GNOME 实际键位探测结果生成，"
        "请勿手工编辑；改清单请改 JSON 后重新运行 `tools/generate.py`。",
        "",
        f"- 本机 GNOME 已占用的加速键共 **{unique_accels}** 个（探测自 dconf）",
        f"- 需要迁移的 GNOME 键位：**{len(plan)}** 条" + ("（见文末）" if plan else "（无待迁移）"),
        f"- 归属分类：遵循 macOS **{len(macos_entries)}** 条 · 遵循 GNOME 默认 "
        f"**{len(gnome_entries)}** 条 · 泛化兜底（遵循 macOS）**{len(generated)}** 条",
        f"- 替换模式：吞掉 **{len(swallow_triggers) if swallow_on else 0}** 个 Linux Ctrl 组合（终端配置档例外）",
        "",
        "## 一、遵循 macOS 的键位（由 Mackey 改写）",
        "",
        f"共 **{len(macos_entries)}** 条：这些组合被本工具接管，翻译成当前应用在 Linux 下的等价按键。",
        "",
        "| macOS | 行为 | 触发 | 默认输出 | 终端输出 | 文件管理器 | 状态 | 冲突 / 备注 |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for e in macos_entries:
        t = e.get("targets", {})
        note = e.get("note", "")
        if e.get("conflict"):
            note = f"**冲突**：{e['conflict']}。" + note
        if e["id"] in reloc_ids:
            note = "**本机已迁移 GNOME 键位** " + note
        lines.append("| {mac} | {desc} | {trigger} | {d} | {t} | {f} | {s} | {n} |".format(
            mac=e["mac"], desc=e["desc"], trigger=pretty(e["trigger"]),
            d=pretty(t.get("default")), t=pretty(t.get("terminal")), f=pretty(t.get("files")),
            s=status_label.get(e["status"], e["status"]), n=note or "",
        ))

    lines += [
        "",
        "## 二、遵循 GNOME 默认的键位（Mackey 不接管）",
        "",
        f"共 **{len(gnome_entries)}** 条：不做映射，按键完全交给 GNOME 处理。",
        "",
        "| 键位 | 行为 | 归因 | 说明 |",
        "|---|---|---|---|",
    ]
    for e in gnome_entries:
        key = pretty(e["trigger"]) if e.get("trigger") else e["mac"]
        lines.append("| {key} | {desc} | {why} | {note} |".format(
            key=key, desc=e["desc"],
            why=gnome_reason.get(e["status"], e["status"]),
            note=e.get("note", "") or "",
        ))

    lines += [
        "",
        "## 三、GNOME 已占用键位的归属（探测自 dconf）",
        "",
        f"共 **{total_accels}** 条绑定（{unique_accels} 个不同组合）。归属含义："
        "**被接管** = 让给 macOS 组合；**已迁移** = GNOME 功能换到新组合；"
        "**迁移落点** = 迁移后 GNOME 功能所在位置；**GNOME 保留** = 本工具完全不碰。",
        "",
        "| GNOME 绑定 | 加速键 | 归属 |",
        "|---|---|---|",
    ]
    for r in rows:
        for accel in r["accels"]:
            if accel in plan_from:
                attr = f"已迁移 → `{plan_from[accel]}` {pretty(plan_from[accel])}"
            elif accel in mig_old:
                dest = "、".join(f"`{a}` {pretty(a)}" for a in mig_old[accel]["now"]) or "（未检测到）"
                attr = f"已迁移 → {dest}"
            elif accel in plan_to or accel in mig_now:
                attr = "迁移落点（GNOME 功能新位置）"
            elif accel in consuming:
                attr = "被 Mackey 接管（macOS 行为）"
            else:
                attr = "GNOME 保留"
            lines.append(f"| `{r['ident']}` | `{accel}` {pretty(accel)} | {attr} |")

    lines += [
        "",
        f"## 四、泛化兜底（自动生成 {len(generated)} 条，遵循 macOS）",
        "",
        "清单里没有逐个列出的 ⌘+字母/数字组合，统一按「⌘=Ctrl」翻译；"
        "与 GNOME 已占用键位（如 ⌘A/⌘V/⌘S/⌘D/⌘N/⌘Q/⌘H/⌘E/⌘L/⌘M/⌘P/⌘Space）自动互斥，不会抢占。",
        "终端配置档 **不参与**泛化：由独立的 `generic-sweep` keymap 加 `application.not` 排除终端实现"
        "（xremap 是按每个键回退的，混在 generic 里会漏进终端）。",
        "显式条目的默认输出**仍会进入终端**（截图、⌘Space、⌘⇧[/] 等是有意保留的）；"
        "个别在终端里含义有害的条目（⌘\\、⌘I、⌘O、⌘G）已显式标 `F24` 吞掉。",
        "",
        "| macOS | 触发 | 输出 |",
        "|---|---|---|",
    ]
    for e in generated:
        lines.append(f"| {e['mac']} | {pretty(e['trigger'])} | {pretty(e['targets'].get('default'))} |")

    lines += [
        "",
        "## 五、替换模式：被吞掉的 Linux Ctrl 组合",
        "",
    ]
    if swallow_spec.get("enabled") and swallow_triggers:
        except_apps = swallow_spec.get("except_apps", [])
        lines += [
            f"共 **{len(swallow_triggers)}** 个组合在**非例外应用**里被吞掉（发射无害的 F24，应用收不到），"
            "只保留对应的 ⌘ 版本——模拟 macOS 的语义。",
            "实现方式是独立 keymap + `application.not`，例外名单 = 终端配置档 + 内嵌终端应用："
            "真实终端里 `Ctrl+C` 仍是 SIGINT、`Ctrl+A/Z/W/L` 仍是 readline 与作业控制，"
            "VS Code / JetBrains 等**内嵌终端**里的 `Ctrl+C` 也仍能中断进程。",
            "",
            "| 物理按键 | 在 macOS 上 | 替换为 |",
            "|---|---|---|",
        ]
        for trigger in swallow_triggers:
            letter = trigger.rsplit("-", 1)[-1]
            mate = next((e for e in entries if norm(e.get("trigger") or "") == f"SUPER-{letter}"), None)
            use = f"{mate['mac']}（{mate['desc']}）" if mate else "对应的 ⌘ 组合"
            lines.append(f"| {pretty(trigger)} | 无此功能 | {use} |")
        lines += ["", "例外应用（wm_class 正则，可在 `swallow_ctrl.except_apps` 里增删）："]
        for matcher in except_apps:
            lines.append(f"- `{matcher}`")
    else:
        lines.append("未启用（保持叠加：⌘ 与 Ctrl 两套都可用）。")

    terminal_swallow_triggers = [norm(t) for t in terminal_swallow_spec.get("triggers", [])]
    if terminal_swallow_spec.get("enabled") and terminal_swallow_triggers:
        lines += [
            "",
            f"终端配置档里另有 **{len(terminal_swallow_triggers)}** 个组合被吞掉（`swallow_terminal`）："
            + "、".join(f"`{pretty(t)}`" for t in terminal_swallow_triggers)
            + " —— 终端里复制/粘贴只保留 `⌘C`/`⌘V`。"
            "其中 `⌃⇧C`/`⌃⇧V` 正是 ⌘C/⌘V 的输出，但走的是引擎合成路径（xremap 不读自己合成的按键），"
            "所以 ⌘ 路径不受影响；其它应用里这些组合原样放行。",
        ]

    lines += ["", "## 六、需要迁移的 GNOME 键位", ""]
    if plan:
        lines += ["| GNOME 功能 | 原按键 | 迁移到 | 为谁让路 |", "|---|---|---|---|"]
        for p in plan:
            lines.append(f"| {p['label']} | `{p['from']}` {pretty(p['from'])} | `{p['to']}` {pretty(p['to'])} | {p['triggered_by'] or ''} |")
        lines += ["", "> 迁移写在本机 dconf（用户配置）里，安装时备份、卸载时还原；"
                  "只在检测到冲突时才执行。"]
    elif migrations:
        lines += [
            "本机当前无待迁移冲突；下表是**此前已经执行过**的迁移（由备份推算）。"
            "`mackey revert` / `uninstall` 会还原为原按键。",
            "",
            "| GNOME 绑定 | 原按键 | 现按键 |",
            "|---|---|---|",
        ]
        for m in migrations:
            old = "、".join(f"`{a}` {pretty(a)}" for a in m["old"])
            now = "、".join(f"`{a}` {pretty(a)}" for a in m["now"]) or "（未检测到）"
            lines.append(f"| `{m['ident']}` | {old} | {now} |")
    else:
        lines += ["本机无冲突，无需改动任何 GNOME 键位。"]

    lines += [
        "",
        "## 七、明确不做的事",
        "",
        "- 不接管 `⌃←/⌃→`（切换桌面空间）：在 Linux 上它们是「按词移动」，接管代价大于收益。",
        "- 不接管 `⌃↑/⌃↓`（调度中心/窗口展览）：同上，默认关闭，可按需在清单里改为 translate。",
        "- 不模拟 macOS 菜单栏的「按住 ⌘ 显示快捷键提示」「⌥⌘Esc 强制退出」：GNOME 无对应概念。",
        "- 不做「Ctrl 与 ⌘ 全局互换」（modmap）：那会让终端失去 SIGINT、摧毁 readline 与作业控制。",
        "- 只做**点状替换**：`swallow_ctrl` 列出的组合在非例外应用里被吞掉（见第五节），"
        "其余 Linux Ctrl 习惯原样保留；终端与内嵌终端应用（VS Code/JetBrains 等）不受影响。",
    ]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="生成 xremap 配置与行为清单")
    conf_dir = xdg_config_home() / "mackey"
    ap.add_argument("--checklist", default=str(ROOT / "config" / "checklist.json"))
    ap.add_argument("--user-config", default=str(conf_dir / "config.json"))
    ap.add_argument("--out-dir", default=str(conf_dir))
    ap.add_argument("--docs", default=os.environ.get("MACKEY_DOCS") or str(ROOT / "docs" / "03-行为清单.md"))
    ap.add_argument("--no-probe", action="store_true", help="跳过 dconf 探测（离线渲染文档用）")
    ap.add_argument("--no-docs", action="store_true", help="不渲染行为清单（运行时命令别改仓库文档）")
    ap.add_argument("--backup", default=str(conf_dir / "backup" / "gsettings.json"),
                    help="GNOME 键位备份（用于推算已执行过的迁移）")
    ap.add_argument("--report", action="store_true", help="只打印报告，不写文件")
    args = ap.parse_args(argv)

    checklist: Checklist = json.loads(Path(args.checklist).read_text(encoding="utf-8"))
    user: UserConfig = {}
    if Path(args.user_config).exists():
        user = json.loads(Path(args.user_config).read_text(encoding="utf-8"))

    probed = False
    if args.no_probe:
        occupied: set[str] = set()
        table: dict[str, str] = {}
        rows: list[BindingRow] = []
    else:
        try:
            occupied, table, rows = probe_gnome_occupied()
            probed = True
        except ProbeUnavailable as exc:
            print(f"! {exc}；按「无 GNOME 冲突数据」继续（等同 --no-probe）", file=sys.stderr)
            occupied, table, rows = set(), {}, []
        except ProbeError as exc:
            print(f"✗ GNOME 键位探测失败：{exc}", file=sys.stderr)
            print("  无法确认冲突就覆盖 GNOME 快捷键是危险的，因此这里直接失败。", file=sys.stderr)
            print("  离线渲染文档请显式加 --no-probe。", file=sys.stderr)
            return 2

    # ---- 校验 basis 分类：键位表必须与真实行为一致 ----
    bad_basis = check_basis(checklist)
    if bad_basis:
        print("✗ 行为清单的 basis 分类有问题：", file=sys.stderr)
        for b in bad_basis:
            print("   -", b, file=sys.stderr)
        return 2

    bad_reloc = check_relocate_refs(checklist)
    if bad_reloc:
        print("✗ 行为清单的 relocate 引用有问题：", file=sys.stderr)
        for b in bad_reloc:
            print("   -", b, file=sys.stderr)
        return 2

    entries = list(checklist["entries"])
    plan: list[RelocationPlan] = [] if not probed else relocation_plan(checklist, occupied, table)
    migrations: list[Migration] = [] if not probed else load_migrations(Path(args.backup), table)
    reserved = {norm(t) for p in plan for t in p.get("targets", [p["to"]])}
    explicit = {norm(e["trigger"]) for e in entries if e.get("trigger")}
    entries += sweep_entries(checklist, occupied, explicit, reserved)

    # ---- 截图输出跟随 GNOME 实测绑定（探测不可用时保留清单默认值）----
    if probed:
        blocked_outputs = {norm(e["trigger"]) for e in entries
                           if e.get("trigger") and e.get("targets")}
        blocked_outputs |= {norm(t) for t in checklist.get("swallow_ctrl", {}).get("triggers", [])}
        own_triggers = {norm(e["trigger"]) for e in entries
                        if e["id"] in SCREENSHOT_ACTIONS and e.get("trigger")}
        derived = derived_screenshot_targets(table, blocked_outputs, own_triggers)
        for e in entries:
            if e.get("generated") or not e.get("targets"):
                continue
            accel = derived.get(e["id"])
            if not accel:
                continue
            old = e["targets"].get("default")
            if old and norm(old) != accel:
                print(f"! {e['id']}：输出跟随 GNOME 实测绑定 {accel}（清单默认 {old}）",
                      file=sys.stderr)
            e["targets"]["default"] = accel

    # ---- 校验键名 ----
    swallow_spec = checklist.get("swallow_ctrl", {})
    swallow_triggers = [norm(t) for t in swallow_spec.get("triggers", [])]
    terminal_swallow_spec = checklist.get("swallow_terminal", {})
    terminal_swallow_triggers = [norm(t) for t in terminal_swallow_spec.get("triggers", [])]
    all_names: set[str] = set()
    for e in entries:
        if e.get("trigger"):
            all_names.add(norm(e["trigger"]))
        for v in e.get("targets", {}).values():
            for item in ([v] if isinstance(v, str) else v):
                all_names.update(norm(x.strip()) for x in item.split(","))
    all_names.add(norm(checklist["meta"]["swallow_key"]))
    all_names.update(swallow_triggers)
    all_names.update(terminal_swallow_triggers)
    bad_names = check_key_names(all_names)
    if bad_names:
        print("✗ 非法的键名（会被引擎拒绝）：", file=sys.stderr)
        for b in bad_names:
            print("   -", b, file=sys.stderr)
        return 2

    # ---- 硬校验：迁移目标不能被清单自己吃掉，否则 GNOME 功能会彻底消失 ----
    # 只检查「真的会吞掉按键」的条目（targets 非空）；纯 passthrough 条目（例如
    # 锁屏把 ⌃⌘Q 直接让给 GNOME）与迁移目标同键是设计如此。
    relocation_targets = {norm(t) for p in plan for t in p.get("targets", [p["to"]])}
    consuming = {norm(e["trigger"]) for e in entries if e.get("trigger") and e.get("targets")}
    if swallow_spec.get("enabled"):
        consuming |= set(swallow_triggers)
    if terminal_swallow_spec.get("enabled"):
        consuming |= set(terminal_swallow_triggers)
    clashes = sorted(relocation_targets & consuming)
    if clashes:
        print("✗ 迁移目标与清单触发键冲突（GNOME 功能会被吃掉）：", file=sys.stderr)
        for c in clashes:
            print("   -", c, file=sys.stderr)
        return 2

    # ---- 硬校验：与 GNOME 已占用键位重合的触发键，必须有迁移计划 ----
    unplanned = check_occupied_conflicts(
        consuming, occupied, {norm(f) for p in plan for f in p.get("from_list", [p["from"]])})
    if unplanned:
        print("✗ 以下触发键与 GNOME 已占用键位冲突，但没有迁移计划（会静默抢占）：", file=sys.stderr)
        for c in unplanned:
            print("   -", c, file=sys.stderr)
        print("  处理：先 mackey revert 回到 GNOME 默认键位，再 generate/apply。", file=sys.stderr)
        return 2

    # ---- 组装引擎配置 ----
    layout = user.get("modifier_layout", "apple")
    modmap: list[dict[str, Any]] = []
    if layout == "pc-swap":
        modmap.append({
            "name": "pc-position-swap: 把 ⌘/⌥ 摆到 macOS 的物理位置",
            "remap": {"ALT_L": "SUPER_L", "SUPER_L": "ALT_L",
                      "ALT_R": "SUPER_R", "SUPER_R": "ALT_R"},
        })
    elif layout not in ("apple",):
        print(f"✗ 未知的 modifier_layout: {layout}", file=sys.stderr)
        return 2

    terminal_entries = [e for e in entries if "terminal" in e["targets"]]
    files_entries = [e for e in entries if "files" in e["targets"]]
    config: XremapConfig = {}
    if user.get("keypress_delay_ms"):
        config["keypress_delay_ms"] = user["keypress_delay_ms"]
    if user.get("throttle_ms"):
        config["throttle_ms"] = user["throttle_ms"]
    if user.get("notifications"):
        config["notifications"] = True
    if modmap:
        config["modmap"] = modmap

    device_filter = user.get("device", {})
    def with_device(node: XremapKeymap) -> XremapKeymap:
        if device_filter.get("only") or device_filter.get("not"):
            node["device"] = device_filter
        return node

    swallow = checklist["meta"]["swallow_key"]
    explicit_entries = [e for e in entries if not e.get("generated")]
    generated_entries = [e for e in entries if e.get("generated")]
    terminal_map = build_remap(terminal_entries, "terminal", swallow)
    files_map = build_remap(files_entries, "files", swallow)
    generic_map = build_remap(explicit_entries, "default", swallow)
    sweep_map = build_remap(generated_entries, "default", swallow)

    keymaps: list[XremapKeymap] = []
    keymaps.append(with_device({
        "name": "terminal：终端里 Ctrl+C/Z/D 是信号与 EOF，必须单独一套",
        "application": {"only": checklist["profiles"]["terminal"]["match"]},
        "remap": terminal_map,
    }))
    if terminal_swallow_spec.get("enabled") and terminal_swallow_triggers:
        # 只作用于终端：物理 Ctrl+Shift+C/V 被吞掉，终端里复制/粘贴只走 ⌘C/⌘V。
        # ⌘C/⌘V 合成出的 Ctrl+Shift+C/V 不在拦截范围内（引擎不读自己合成的按键）；
        # Ctrl+V 特意不吞，保留 readline 的 quoted-insert。
        keymaps.append(with_device({
            "name": "swallow-terminal：终端里吞掉 Ctrl+Shift+C/V（复制/粘贴只认 ⌘C/⌘V）",
            "application": {"only": checklist["profiles"]["terminal"]["match"]},
            "remap": {trigger: norm(swallow) for trigger in terminal_swallow_triggers},
        }))
    keymaps.append(with_device({
        "name": "files：文件管理器语义不同（⌘⌫=废纸篓、⌘I=属性）",
        "application": {"only": checklist["profiles"]["files"]["match"]},
        "remap": files_map,
    }))
    keymaps.append(with_device({
        "name": "generic：其余应用的兜底（只含显式条目）",
        "remap": generic_map,
    }))
    if sweep_map:
        # xremap 是按「每个键」顺序回退的：若把泛化放进 generic，终端也会命中它。
        # 单独一套并用 application.not 排除终端，才能真正做到「终端不参与泛化」。
        keymaps.append(with_device({
            "name": "generic-sweep：泛化兜底（终端不参与）",
            "application": {"not": checklist["profiles"]["terminal"]["match"]},
            "remap": sweep_map,
        }))
    if swallow_spec.get("enabled") and swallow_triggers:
        keymaps.append(with_device({
            "name": "swallow：替换模式（终端与内嵌终端应用除外）",
            "application": {"not": swallow_except_matchers(checklist)},
            "remap": {trigger: norm(swallow) for trigger in swallow_triggers},
        }))
    config["keymap"] = keymaps

    # ---- 汇总 ----
    conflicts = [e for e in entries if e.get("conflict")]
    stats: dict[str, int] = {}
    basis_stats: dict[str, int] = {}
    for e in entries:
        stats[e["status"]] = stats.get(e["status"], 0) + 1
        basis = "macos" if e.get("basis") == "macos" else "gnome"
        basis_stats[basis] = basis_stats.get(basis, 0) + 1

    print("Mackey 生成报告")
    print("─" * 68)
    print(f"布局           : {layout}（{'⌘=Super 原生' if layout == 'apple' else '⌘=物理 Alt，由 modmap 交换'}）")
    print(f"清单条目       : {len([e for e in entries if not e.get('generated')])} 条显式 + {len([e for e in entries if e.get('generated')])} 条泛化兜底")
    print(f"GNOME 已占用   : {len(occupied)} 个加速键（探测自 dconf；已执行迁移 {len(migrations)} 条）")
    print(f"冲突条目       : {len(conflicts)} 条；需要迁移 GNOME 键位 {len(plan)} 条")
    print(f"归属分类       : " + "、".join(f"{k}={v}" for k, v in sorted(basis_stats.items())))
    swallow_desc = (f"吞掉 {len(swallow_triggers) if swallow_spec.get('enabled') else 0} 个 Linux Ctrl 组合"
                    "（终端配置档例外）")
    if terminal_swallow_spec.get("enabled") and terminal_swallow_triggers:
        swallow_desc += f"；终端内另吞 {len(terminal_swallow_triggers)} 个物理组合"
    print(f"替换模式       : {swallow_desc}")
    print(f"状态分布       : " + "、".join(f"{k}={v}" for k, v in sorted(stats.items())))
    print(f"终端专用映射   : {len(terminal_entries)} 条；文件管理器 {len(files_entries)} 条")

    if args.report:
        print("\n（--report 模式，未写文件）")
        return 0

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "xremap.json").write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    (out_dir / "relocations.json").write_text(json.dumps(plan, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    written = [out_dir / "xremap.json", out_dir / "relocations.json"]
    if not args.no_docs:
        docs = Path(args.docs)
        docs.parent.mkdir(parents=True, exist_ok=True)
        docs.write_text(render_markdown(entries, plan, rows, migrations, swallow_spec,
                                        terminal_swallow_spec),
                        encoding="utf-8")
        written.append(docs)
    print("\n已写入：\n" + "\n".join(f"  {p}" for p in written))
    return 0


if __name__ == "__main__":
    sys.exit(main())
