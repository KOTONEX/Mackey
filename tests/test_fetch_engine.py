#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""fetch-engine.py 的离线单元测试：不联网、不写真实 XDG 目录。

覆盖：
  * 指令集架构/桌面环境 → 资产名与下载 URL 的映射（含 X11、未知桌面回退 full）；
  * 候选回退（精确特性 → full）；
  * `--print-plan` 在上游标签明确时不碰网络，且落点参数被尊重；
  * 从 zip 取二进制：成员校验、ELF 校验（含架构匹配）、原子安装、权限、sha256、state 记录；
  * 下载：强制 HTTPS、声明/实际大小上限、临时文件失败清理（全程用假响应，不联网）；
  * latest 标签跳转 URL 的解析（用假 URL，不联网）。
"""
from __future__ import annotations

import hashlib
import json
import struct
import subprocess
import sys
import tempfile
import zipfile
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

import importlib

fetch = importlib.import_module("fetch-engine")

failures = 0


def check(name: str, cond: bool, extra: str = "") -> None:
    global failures
    if cond:
        print(f"  ✓ {name}")
    else:
        failures += 1
        print(f"  ✗ {name} {extra}")


print("fetch-engine 单元测试（离线）")

# 1) 指令集架构
check("x86_64/amd64 归一化",
      fetch.arch_for("x86_64") == "x86_64" and fetch.arch_for("AMD64") == "x86_64")
check("aarch64/arm64 归一化",
      fetch.arch_for("aarch64") == "aarch64" and fetch.arch_for("arm64") == "aarch64")
check("未知指令集架构返回 None", fetch.arch_for("riscv64") is None)

# 2) 桌面环境 → 特性
cases = [
    ("GNOME", "wayland", "gnome"),
    ("ubuntu:GNOME", "wayland", "gnome"),
    ("KDE", "wayland", "kde"),
    ("Hyprland", "wayland", "hypr"),
    ("niri", "wayland", "niri"),
    ("COSMIC", "wayland", "cosmic"),
    ("Pantheon", "wayland", "pantheon"),
    ("sway", "wayland", "wlroots"),
    ("river", "wayland", "wlroots"),
    ("GNOME", "x11", "x11"),
    ("", "wayland", "full"),
]
for desktop, session, expected in cases:
    feature, _ = fetch.detect_feature(desktop, session)
    check(f"{desktop or '(未知)'}/{session} → {expected}", feature == expected, f"→ {feature}")

# 3) 资产名与 URL
check("资产名", fetch.asset_name("x86_64", "gnome") == "xremap-linux-x86_64-gnome.zip")
check("下载 URL",
      fetch.asset_url("v0.15.13", "aarch64", "kde")
      == "https://github.com/xremap/xremap/releases/download/v0.15.13/xremap-linux-aarch64-kde.zip")
check("候选回退到 full", fetch.candidates("gnome") == ["gnome", "full"])
check("full 不重复", fetch.candidates("full") == ["full"])

# 4) 标签解析（假 URL，不联网）
check("标签解析", fetch.tag_from_url("https://github.com/xremap/xremap/releases/tag/v0.15.13") == "v0.15.13")
try:
    fetch.tag_from_url("https://github.com/xremap/xremap/releases/tag/not-a-tag")
    check("非法标签会报错", False)
except RuntimeError:
    check("非法标签会报错", True)

# 5) --print-plan：给定标签时完全离线，且尊重落点参数
with tempfile.TemporaryDirectory() as tmp:
    tmpdir = Path(tmp)
    dest, state = tmpdir / "bin/xremap", tmpdir / "state.json"
    proc = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "fetch-engine.py"), "--print-plan",
         "--tag", "v0.15.13", "--arch", "x86_64", "--desktop", "GNOME", "--session", "wayland",
         "--dest", str(dest), "--cache", str(tmpdir / "cache"), "--state", str(state)],
        capture_output=True, text=True,
    )
    try:
        plan = json.loads(proc.stdout)
        check("--print-plan 输出合法 JSON", proc.returncode == 0, f"→ rc={proc.returncode} {proc.stderr[-200:]}")
        check("--print-plan 选出 gnome 资产",
              plan["candidates"][0].endswith("xremap-linux-x86_64-gnome.zip"), f"→ {plan['candidates'][0]}")
        check("--print-plan 不落盘", not dest.exists() and not state.exists())
    except json.JSONDecodeError as exc:
        check("--print-plan 输出合法 JSON", False, f"→ {exc}: {proc.stdout!r}")

# 6) zip → 安装（成员校验 / ELF 校验 / 原子安装 / 权限 / sha256）
def make_zip(path: Path, member: str, payload: bytes) -> None:
    with zipfile.ZipFile(path, "w") as zf:
        zf.writestr(member, payload)


def elf_payload(machine: int, endian: str = "<") -> bytes:
    """构造带合法 ELF 头的最小负载，用于测 e_machine 校验。"""
    data = bytearray(b"\x7fELF" + b"\x00" * 64)
    data[4] = 2  # EI_CLASS = ELFCLASS64
    data[5] = 1 if endian == "<" else 2  # EI_DATA = 小端 / 大端
    struct.pack_into(endian + "H", data, 18, machine)
    return bytes(data)


class FakeResponse:
    """只实现 download() 需要的接口：geturl / headers / read / 上下文管理器。"""

    def __init__(self, body: bytes, *, url: str = "https://example.test/x.zip",
                 content_length: str | None = None) -> None:
        self.body = body
        self.url = url
        self.headers = {} if content_length is None else {"Content-Length": content_length}
        self.offset = 0

    def geturl(self) -> str:
        return self.url

    def read(self, n: int) -> bytes:
        chunk = self.body[self.offset:self.offset + n]
        self.offset += len(chunk)
        return chunk

    def __enter__(self) -> "FakeResponse":
        return self

    def __exit__(self, *exc: object) -> bool:
        return False


@contextmanager
def stub_urlopen(resp: FakeResponse) -> Iterator[None]:
    """把 urlopen 换成假响应，退出时恢复；测试全程不联网。"""
    orig = fetch.urllib.request.urlopen
    fetch.urllib.request.urlopen = lambda *args, **kwargs: resp
    try:
        yield
    finally:
        fetch.urllib.request.urlopen = orig


with tempfile.TemporaryDirectory() as tmp:
    tmpdir = Path(tmp)
    payload = b"\x7fELF" + b"\x00" * 64
    good = tmpdir / "good.zip"
    make_zip(good, "xremap", payload)
    dest = tmpdir / "bin/xremap"
    digest = fetch.extract_and_install(good, dest)
    check("安装后文件存在且可执行", dest.is_file() and bool(dest.stat().st_mode & 0o111))
    check("安装后内容一致", dest.read_bytes() == payload)
    check("返回的 sha256 正确", digest == hashlib.sha256(payload).hexdigest())

    state = tmpdir / "state.json"
    fetch.write_state(state, {"tag": "v0.15.13", "sha256": digest})
    check("state 可读回", fetch.read_state(state).get("tag") == "v0.15.13")
    check("坏 state 返回空 dict", fetch.read_state(tmpdir / "missing.json") == {})

    bad_member = tmpdir / "bad-member.zip"
    make_zip(bad_member, "not-xremap", payload)
    try:
        fetch.extract_and_install(bad_member, dest)
        check("成员名不对会拒绝", False)
    except RuntimeError:
        check("成员名不对会拒绝", True)

    bad_elf = tmpdir / "bad-elf.zip"
    make_zip(bad_elf, "xremap", b"#!/bin/sh\n")
    try:
        fetch.extract_and_install(bad_elf, dest)
        check("非 ELF 会拒绝", False)
    except RuntimeError:
        check("非 ELF 会拒绝", True)
    check("拒绝后旧文件未被破坏", dest.read_bytes() == payload)

    right_arch = tmpdir / "right-arch.zip"
    make_zip(right_arch, "xremap", elf_payload(0x3E))
    digest = fetch.extract_and_install(right_arch, dest, "x86_64")
    check("架构匹配的 ELF 可安装",
          dest.read_bytes() == elf_payload(0x3E)
          and digest == hashlib.sha256(elf_payload(0x3E)).hexdigest())

    wrong_arch = tmpdir / "wrong-arch.zip"
    make_zip(wrong_arch, "xremap", elf_payload(0xB7))
    try:
        fetch.extract_and_install(wrong_arch, dest, "x86_64")
        check("aarch64 ELF 装到 x86_64 会拒绝", False)
    except RuntimeError as exc:
        check("aarch64 ELF 装到 x86_64 会拒绝", "架构不匹配" in str(exc), f"→ {exc}")
    check("架构不匹配后旧文件未被破坏", dest.read_bytes() == elf_payload(0x3E))

    big_endian = tmpdir / "big-endian.zip"
    make_zip(big_endian, "xremap", elf_payload(0xB7, ">"))
    fetch.extract_and_install(big_endian, dest, "aarch64")
    check("大端 ELF 头按 EI_DATA 解析", dest.read_bytes() == elf_payload(0xB7, ">"))

    orig_member_cap = fetch.MAX_MEMBER_BYTES
    fetch.MAX_MEMBER_BYTES = len(payload) - 1
    try:
        try:
            fetch.extract_and_install(good, dest)
            check("成员超过大小上限会拒绝", False)
        except RuntimeError as exc:
            check("成员超过大小上限会拒绝", "上限" in str(exc), f"→ {exc}")
    finally:
        fetch.MAX_MEMBER_BYTES = orig_member_cap
    check("成员超限后旧文件未被破坏", dest.read_bytes() == elf_payload(0xB7, ">"))

# 7) needs_install：只有标签 / 指令集架构 / 特性 / 内容哈希全匹配才跳过重装
with tempfile.TemporaryDirectory() as tmp:
    tmpdir = Path(tmp)
    payload = b"\x7fELF" + b"\x00" * 32
    zip_path = tmpdir / "e.zip"
    make_zip(zip_path, "xremap", payload)
    dest, state = tmpdir / "bin/xremap", tmpdir / "state.json"
    digest = fetch.extract_and_install(zip_path, dest)
    fetch.write_state(state, {"tag": "v1", "arch": "x86_64", "feature": "gnome", "sha256": digest})
    check("全匹配 → 不重装", fetch.needs_install(dest, state, "v1", "x86_64", "gnome") is False)
    check("标签不同 → 重装", fetch.needs_install(dest, state, "v2", "x86_64", "gnome") is True)
    check("特性不同（旧装的 full）→ 重装",
          fetch.needs_install(dest, state, "v1", "x86_64", "full") is True)
    check("指令集架构不同 → 重装", fetch.needs_install(dest, state, "v1", "aarch64", "gnome") is True)
    check("force → 重装", fetch.needs_install(dest, state, "v1", "x86_64", "gnome", force=True) is True)
    dest.write_bytes(payload + b"x")
    dest.chmod(0o755)
    check("目标文件被改 → 重装", fetch.needs_install(dest, state, "v1", "x86_64", "gnome") is True)
    state.unlink()
    check("state 缺失 → 重装", fetch.needs_install(dest, state, "v1", "x86_64", "gnome") is True)

# 8) prune_cache：只保留本次使用的资产
with tempfile.TemporaryDirectory() as tmp:
    downloads = Path(tmp) / "downloads"
    downloads.mkdir()
    keep, stale = downloads / "keep.zip", downloads / "old.zip"
    keep.write_bytes(b"x")
    stale.write_bytes(b"y")
    fetch.prune_cache(downloads, keep)
    check("缓存只保留本次资产", keep.exists() and not stale.exists())

# 9) download：强制 HTTPS / 声明与实际大小上限 / 失败不留临时文件
with tempfile.TemporaryDirectory() as tmp:
    tmpdir = Path(tmp)
    dest = tmpdir / "downloads/xremap-linux-x86_64-gnome.zip"
    body = b"x" * 32

    with stub_urlopen(FakeResponse(body, content_length=str(len(body)))):
        size = fetch.download("https://example.test/x.zip", dest, timeout=5)
    check("合法下载写入目标且大小正确", size == len(body) and dest.read_bytes() == body)
    check("成功下载后无 .part 残留", not list(dest.parent.glob("*.part")))

    dest.unlink()
    with stub_urlopen(FakeResponse(body, url="http://example.test/x.zip")):
        try:
            fetch.download("https://example.test/x.zip", dest, timeout=5)
            check("重定向到非 HTTPS 会拒绝", False)
        except RuntimeError as exc:
            check("重定向到非 HTTPS 会拒绝", "HTTPS" in str(exc), f"→ {exc}")
    check("HTTPS 拒绝后不落盘且无残留",
          not dest.exists() and not list(dest.parent.glob("*.part")))

    with stub_urlopen(FakeResponse(body, content_length=str(fetch.MAX_ARCHIVE_BYTES + 1))):
        try:
            fetch.download("https://example.test/x.zip", dest, timeout=5)
            check("声明大小超限会拒绝", False)
        except RuntimeError as exc:
            check("声明大小超限会拒绝", "上限" in str(exc), f"→ {exc}")
    check("声明超限后不落盘且无残留",
          not dest.exists() and not list(dest.parent.glob("*.part")))

    orig_archive_cap = fetch.MAX_ARCHIVE_BYTES
    fetch.MAX_ARCHIVE_BYTES = 16
    try:
        with stub_urlopen(FakeResponse(body)):
            try:
                fetch.download("https://example.test/x.zip", dest, timeout=5)
                check("实际大小超限会中止", False)
            except RuntimeError as exc:
                check("实际大小超限会中止", "上限" in str(exc), f"→ {exc}")
    finally:
        fetch.MAX_ARCHIVE_BYTES = orig_archive_cap
    check("实际超限后不落盘且无残留",
          not dest.exists() and not list(dest.parent.glob("*.part")))

print()
if failures:
    print(f"✗ {failures} 项未通过")
    sys.exit(1)
print("✓ fetch-engine 单元测试通过")
