#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""识别主键盘，并给出 modifier_layout 的建议。

为什么要它：⌘ 应该落在「空格键旁边那颗键」上。
  - Apple 键盘 / 机械键盘的 Mac 模式：那颗键本身就发 Super → apple（不需要交换）
  - PC 配列键盘：那颗键是 Alt → pc-swap（需要把 Alt/Win 交换，让 ⌘ 回到 macOS 的位置）
判定 Apple 键盘的依据：USB vendor 0x05ac，或设备名里出现 apple/magic keyboard/macbook。
"""
from __future__ import annotations

import json
import re
import sys

# 明显不是主键盘的设备名
NOISE = re.compile(
    r"button|video bus|speaker|hdmi|headphone|mic|touchpad|mouse|"
    r"lid switch|power button|sleep button|extra buttons|hid events|avrcp|"
    r"consumer control|wireless radio",
    re.I,
)


def parse_devices(path: str = "/proc/bus/input/devices") -> list[dict[str, str]]:
    blocks: list[dict[str, str]] = []
    current: dict[str, str] = {}
    try:
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return blocks
    for line in text.splitlines():
        if not line.strip():
            if current:
                blocks.append(current)
                current = {}
            continue
        key, _, value = line.partition(":")
        current[key.strip()] = value.strip()
    if current:
        blocks.append(current)
    return blocks


def score(block: dict[str, str]) -> int:
    handlers = block.get("H", "")
    if "kbd" not in handlers:
        return -1                 # 纯鼠标/手柄等：不是键盘
    ev = block.get("B", "")
    m = re.search(r"EV=([0-9a-f]+)", ev)
    keybits = re.search(r"KEY=([0-9a-f ]+)", ev)
    value = 5
    if m and m.group(1).lower() in ("120013", "100013", "12001b"):
        value += 10          # 典型「真键盘」的 EV 掩码
    if keybits:
        value += min(len(keybits.group(1).split()), 20)
    return value


def normalize_name(raw: str) -> str:
    """`N: Name="AT Translated Set 2 keyboard"` → `AT Translated Set 2 keyboard`。"""
    value = raw.strip()
    if value.lower().startswith("name="):
        value = value[5:]
    return value.strip().strip('"')


def main() -> int:
    seen: dict[str, int] = {}
    for block in parse_devices():
        name = normalize_name(block.get("N", ""))
        if not name or NOISE.search(name):
            continue
        s = score(block)
        if s < 0:                    # 没有 kbd handler，是鼠标/手柄之类
            continue
        seen[name] = max(seen.get(name, -1), s)

    if not seen:
        print(json.dumps({"name": "", "vendor": "", "suggest_layout": "pc-swap"}, ensure_ascii=False))
        return 1

    ranked = sorted(seen.items(), key=lambda kv: -kv[1])
    names = [n for n, _ in ranked]
    # 内建键盘（笔记本自带）通常叫这些名字；有外接键盘时优先按外接键盘判定
    builtin = re.compile(r"AT Translated Set 2 keyboard|ThinkPad|Intel HID|Sony|Dell WMI|"
                         r"Lenovo|Asus|Acer|MSI|Toshiba|HP WMI", re.I)
    external = [n for n in names if not builtin.search(n)]
    primary = external[0] if external else names[0]

    block_vendor = ""
    for block in parse_devices():
        if normalize_name(block.get("N", "")) == primary:
            m = re.search(r"Vendor=([0-9a-f]{4})", block.get("I", ""), re.I)
            block_vendor = m.group(1) if m else ""
            break

    apple = block_vendor.lower() == "05ac" or re.search(r"apple|magic keyboard|macbook", primary, re.I) is not None
    print(json.dumps({
        "name": primary,
        "keyboards": names,
        "vendor": block_vendor,
        "apple": bool(apple),
        "suggest_layout": "apple" if apple else "pc-swap",
        "reason": ("Apple 键盘：Command 键原生就是 Super" if apple else
                   "非 Apple 键盘：空格旁那颗是 Alt，需要交换 Alt/Win 才能让 ⌘ 回到 macOS 的物理位置"),
    }, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
