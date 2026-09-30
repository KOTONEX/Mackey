#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""创建一个虚拟键盘（uinput）并注入按键序列。

用途：Mackey 的端到端自测。测试时 xremap 只抓取这个虚拟设备
（--device mackey-test-kbd），因此**完全不会碰真实键盘**，
也不会把按键注入到当前会话。

用法：
    python3 tests/fake_keyboard.py --send "LEFTALT:1,C:1,C:0,LEFTALT:0" \
        --wait-file /tmp/mackey-go --keep-alive 3

--wait-file: 创建完设备后阻塞等待该文件出现，方便外部先启动 xremap。
"""
from __future__ import annotations

import argparse
import fcntl
import os
import struct
import sys
import time

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_DEV_SETUP = 0x405C5503
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502

EV_SYN, EV_KEY, EV_REP = 0x00, 0x01, 0x14
KEY = {
    "ESC": 1, "1": 2, "2": 3, "3": 4, "4": 5, "5": 6, "6": 7, "7": 8, "8": 9,
    "9": 10, "0": 11, "MINUS": 12, "EQUAL": 13, "BACKSPACE": 14, "TAB": 15,
    "Q": 16, "W": 17, "E": 18, "R": 19, "T": 20, "Y": 21, "U": 22, "I": 23,
    "O": 24, "P": 25, "LEFTBRACE": 26, "RIGHTBRACE": 27, "ENTER": 28,
    "LEFTCTRL": 29, "A": 30, "S": 31, "D": 32, "F": 33, "G": 34, "H": 35,
    "J": 36, "K": 37, "L": 38, "SEMICOLON": 39, "APOSTROPHE": 40,
    "GRAVE": 41, "LEFTSHIFT": 42, "BACKSLASH": 43, "Z": 44, "X": 45, "C": 46,
    "V": 47, "B": 48, "N": 49, "M": 50, "COMMA": 51, "DOT": 52, "SLASH": 53,
    "RIGHTSHIFT": 54, "LEFTALT": 56, "SPACE": 57, "CAPSLOCK": 58,
    "F1": 59, "F2": 60, "F3": 61, "F4": 62, "F5": 63, "F6": 64, "F7": 65,
    "F8": 66, "F9": 67, "F10": 68, "F11": 87, "F12": 88,
    "RIGHTCTRL": 97, "RIGHTALT": 100, "LEFTMETA": 125, "RIGHTMETA": 126,
    "F13": 183, "F14": 184, "F15": 185, "UP": 103, "DOWN": 108,
    "LEFT": 105, "RIGHT": 106, "HOME": 102, "END": 107, "DELETE": 111,
    "INSERT": 110, "PAGEUP": 104, "PAGEDOWN": 109,
}
NAME = "mackey-test-kbd"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--send", default="", help="逗号分隔的 按键:值 序列，例如 C:1,C:0")
    ap.add_argument("--wait-file", default=None)
    ap.add_argument("--keep-alive", type=float, default=0.5)
    ap.add_argument("--delay", type=float, default=0.03, help="事件间隔秒")
    ap.add_argument("--name", default=NAME)
    args = ap.parse_args()

    events: list[tuple[int, int]] = []
    for item in filter(None, (s.strip() for s in args.send.split(","))):
        key, _, value = item.partition(":")
        key = key.strip().upper()
        if key not in KEY:
            sys.exit(f"unknown key: {key}")
        events.append((KEY[key], int(value)))

    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_SYN)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_REP)
    for code in sorted({c for c, _ in events}):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)

    # struct uinput_setup { input_id id; char name[80]; __u32 ff_effects_max; }
    setup = struct.pack("HHHH80sI", 0x03, 0x1234, 0x5678, 1, args.name.encode(), 0)
    try:
        fcntl.ioctl(fd, UI_DEV_SETUP, setup)
    except OSError as exc:  # 老内核回退到 legacy uinput_user_dev
        raise SystemExit(f"UI_DEV_SETUP failed: {exc}")
    fcntl.ioctl(fd, UI_DEV_CREATE)
    print(f"READY {args.name}", flush=True)

    try:
        if args.wait_file:
            for _ in range(600):
                if os.path.exists(args.wait_file):
                    break
                time.sleep(0.1)
            else:
                print("TIMEOUT waiting for go-file", flush=True)
                return 2
            time.sleep(0.2)

        for code, value in events:
            ev = struct.pack("llHHi", 0, 0, EV_KEY, code, value)
            os.write(fd, ev + struct.pack("llHHi", 0, 0, EV_SYN, 0, 0))
            time.sleep(args.delay)
        print("SENT", flush=True)
        time.sleep(args.keep_alive)
    finally:
        try:
            fcntl.ioctl(fd, UI_DEV_DESTROY)
        except OSError:
            pass
        os.close(fd)
    return 0


if __name__ == "__main__":
    sys.exit(main())
