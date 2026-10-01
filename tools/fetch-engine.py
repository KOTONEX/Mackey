#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""下载与当前指令集架构匹配的 xremap 最新发布版本，并安装到 XDG 目录。

Mackey 仅支持 GNOME Wayland：固定取 `gnome` 特性，不识别桌面环境，也不支持
X11 会话——GNOME 50 已移除 X11，且 GNOME 扩展 API 变动频繁，本项目不跟随更旧版本。

为什么不走 GitHub API
---------------------
`api.github.com` 在受限网络里经常返回 403；而
`https://github.com/xremap/xremap/releases/latest` 的 302 跳转稳定可用，
资产命名也是稳定的 `xremap-linux-<arch>-<feature>.zip`（见上游 .github/workflows/build.yml
的 build 矩阵：arch ∈ x86_64/aarch64）。于是可以直接拼 URL，无需枚举资产。

XDG 落点
--------
    dest  : $XDG_DATA_HOME/mackey/bin/xremap          （可执行文件属于数据）
    cache : $XDG_CACHE_HOME/mackey                    （下载与解压的中间产物）
    state : $XDG_STATE_HOME/mackey/engine.json        （安装记录：标签 / 来源 / 哈希）

安全与边界
----------
- 只写这三个 XDG 目录（外加调用方显式传入的路径），绝不动系统目录。
- 下载必须走 HTTPS（含重定向后的最终地址），且有声明/实际大小双重上限。
- 下载先写同目录随机命名的临时文件，fsync 后原子替换目标，失败即清理。
- 校验 zip 内只有预期的 `xremap`（限制成员大小），且 ELF 头架构与当前指令集架构一致。
- 支持 `--print-plan`：只打印将要做什么（配合 `--tag` 时完全不联网）。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import struct
import sys
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import TypedDict

REPO = "xremap/xremap"
LATEST_URL = f"https://github.com/{REPO}/releases/latest"
DOWNLOAD_URL = f"https://github.com/{REPO}/releases/download"
ASSET_TMPL = "xremap-linux-{arch}-{feature}.zip"
USER_AGENT = "mackey-fetch-engine/1"

# 下载与解压的大小上限：上游资产是几 MB 量级，上限只是防跑偏/防灌爆磁盘。
MAX_ARCHIVE_BYTES = 256 * 1024 * 1024
MAX_MEMBER_BYTES = 128 * 1024 * 1024

# 指令集架构：只接受上游矩阵里的两种；其它架构（armv7l/riscv64…）需要自行 cargo install。
ARCH_MAP = {
    "x86_64": "x86_64", "amd64": "x86_64",
    "aarch64": "aarch64", "arm64": "aarch64",
}

# ELF e_machine → 指令集架构：x86_64 = 62 (0x3E)，aarch64 = 183 (0xB7)。
ARCH_ELF_MACHINE = {"x86_64": 0x3E, "aarch64": 0xB7}


class EngineState(TypedDict, total=False):
    """$XDG_STATE_HOME/mackey/engine.json 的记录：安装结果只由它 + 环境决定。"""

    tag: str
    asset: str
    url: str
    sha256: str
    size: int
    arch: str
    feature: str
    installed_at: str
    dest: str


class InstallPlan(TypedDict):
    """--print-plan 的输出形状（也是 main 内部的计算结果）。"""

    tag: str
    arch: str
    feature: str
    reason: str
    candidates: list[str]
    dest: str
    cache: str
    state: str


def xdg_dirs() -> dict[str, Path]:
    home = Path.home()
    return {
        "data": Path(os.environ.get("XDG_DATA_HOME") or home / ".local/share") / "mackey",
        "cache": Path(os.environ.get("XDG_CACHE_HOME") or home / ".cache") / "mackey",
        "state": Path(os.environ.get("XDG_STATE_HOME") or home / ".local/state") / "mackey",
    }


def arch_for(machine: str) -> str | None:
    return ARCH_MAP.get((machine or "").strip().lower())


# Mackey 仅支持 GNOME Wayland（GNOME 50 起已无 X11 会话），构建特性固定为 gnome。
DEFAULT_FEATURE = "gnome"
DEFAULT_FEATURE_REASON = "Mackey 仅支持 GNOME Wayland：固定使用 gnome 特性"


def candidates(feature: str) -> list[str]:
    """优先精确特性；缺失（旧标签没有该资产）时回退到 full。"""
    out = [feature]
    if feature != "full":
        out.append("full")
    return out


def asset_name(arch: str, feature: str) -> str:
    return ASSET_TMPL.format(arch=arch, feature=feature)


def asset_url(tag: str, arch: str, feature: str) -> str:
    return f"{DOWNLOAD_URL}/{tag}/{asset_name(arch, feature)}"


def tag_from_url(url: str) -> str:
    """从 `.../releases/tag/vX.Y.Z` 里取出标签。"""
    tag = url.rstrip("/").rsplit("/", 1)[-1]
    if not re.fullmatch(r"v[0-9][\w.\-]*", tag):
        raise RuntimeError(f"无法从 {url} 解析发布标签")
    return tag


def resolve_latest_tag(timeout: float = 30.0) -> str:
    req = urllib.request.Request(LATEST_URL, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return tag_from_url(resp.geturl())


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fsync_dir(path: Path) -> None:
    """fsync 目录本身，保证 os.replace 之后的目录项也落盘。"""
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def parse_content_length(value: str | None) -> int:
    """解析 Content-Length；缺失返回 -1，非法直接拒绝（可能是构造的响应）。"""
    if value is None:
        return -1
    try:
        return int(value.strip())
    except (AttributeError, ValueError):
        raise RuntimeError(f"Content-Length 非法：{value!r}") from None


def download(url: str, dest: Path, timeout: float = 60.0) -> int:
    """下载 url 到 dest：先写随机命名的临时文件，fsync 后原子替换。

    临时文件用 mkstemp 生成，并发运行不会互相踩；任何失败都会清理临时文件。
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    fd, tmp_name = tempfile.mkstemp(dir=dest.parent, prefix=dest.name + ".", suffix=".part")
    tmp = Path(tmp_name)
    size = 0
    try:
        with os.fdopen(fd, "wb") as fh, urllib.request.urlopen(req, timeout=timeout) as resp:
            final_url = resp.geturl()
            if not final_url.startswith("https://"):
                raise RuntimeError(f"下载最终地址不是 HTTPS（{final_url}），拒绝写入")
            declared = parse_content_length(resp.headers.get("Content-Length"))
            if declared > MAX_ARCHIVE_BYTES:
                raise RuntimeError(
                    f"资产声明大小 {declared} 字节，超过上限 {MAX_ARCHIVE_BYTES} 字节，拒绝下载")
            while True:
                chunk = resp.read(1 << 20)
                if not chunk:
                    break
                size += len(chunk)
                if size > MAX_ARCHIVE_BYTES:
                    raise RuntimeError(
                        f"资产实际大小超过上限 {MAX_ARCHIVE_BYTES} 字节，拒绝继续下载")
                fh.write(chunk)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, dest)
        fsync_dir(dest.parent)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise
    return size


def prune_cache(downloads: Path, keep: Path) -> None:
    """只保留本次使用的资产，避免历次下载在缓存里堆积。"""
    if not downloads.is_dir():
        return
    for item in downloads.iterdir():
        if item.is_file() and item != keep:
            item.unlink(missing_ok=True)


def elf_machine(data: bytes) -> int:
    """解析 ELF 头的 e_machine（按 EI_DATA 处理大小端）；不合法时抛 RuntimeError。"""
    if not data.startswith(b"\x7fELF"):
        raise RuntimeError("取出的文件不是 ELF 可执行文件，拒绝安装")
    if len(data) < 20:
        raise RuntimeError("ELF 头不完整，拒绝安装")
    ei_data = data[5]
    if ei_data == 1:
        return struct.unpack_from("<H", data, 18)[0]
    if ei_data == 2:
        return struct.unpack_from(">H", data, 18)[0]
    raise RuntimeError(f"ELF 头 EI_DATA 非法（{ei_data}），拒绝安装")


def extract_and_install(zip_path: Path, dest: Path, arch: str = "") -> str:
    """从 zip 里取出 `xremap`，校验后原子安装到 dest，返回二进制 sha256。

    给出 arch 时会额外校验 ELF 头的 e_machine（按 EI_DATA 处理大小端），
    避免把给别的指令集架构编译的二进制装进来。
    """
    with zipfile.ZipFile(zip_path) as zf:
        names = zf.namelist()
        if "xremap" not in names:
            raise RuntimeError(f"{zip_path} 里没有预期的 xremap 成员（实际：{names}）")
        info = zf.getinfo("xremap")
        if info.file_size > MAX_MEMBER_BYTES:
            raise RuntimeError(
                f"xremap 成员大小 {info.file_size} 字节，超过上限 {MAX_MEMBER_BYTES} 字节，拒绝解压")
        data = zf.read("xremap")
    if not data.startswith(b"\x7fELF"):
        raise RuntimeError("取出的文件不是 ELF 可执行文件，拒绝安装")
    if arch:
        want = ARCH_ELF_MACHINE.get(arch)
        if want is None:
            raise RuntimeError(f"未知指令集架构：{arch}")
        machine = elf_machine(data)
        if machine != want:
            raise RuntimeError(
                f"ELF 架构不匹配：文件 e_machine={machine}（0x{machine:X}），"
                f"期望 {arch}（e_machine={want}）")
    dest.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(dir=dest.parent, prefix=dest.name + ".", suffix=".tmp")
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
        os.chmod(tmp_name, 0o755)
        os.replace(tmp_name, dest)
    except BaseException:
        Path(tmp_name).unlink(missing_ok=True)
        raise
    return hashlib.sha256(data).hexdigest()


def write_state(path: Path, data: EngineState) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def read_state(path: Path) -> EngineState:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def needs_install(dest: Path, state_path: Path, tag: str, arch: str, feature: str,
                  force: bool = False) -> bool:
    """是否需要（重新）安装：必须同时匹配标签 / 指令集架构 / 特性 / 内容哈希。

    只看标签是不够的：同一版本可能装着别的特性（如包含全部特性的 `full`）编的构建，
    或者目标文件被替换过；这些情况都要重新下载，才能保证「安装结果只由当前
    环境决定」，与干净安装一致。
    """
    if force:
        return True
    if not (dest.is_file() and os.access(dest, os.X_OK)):
        return True
    state = read_state(state_path)
    if (state.get("tag") != tag or state.get("arch") != arch
            or state.get("feature") != feature):
        return True
    return state.get("sha256") != sha256_file(dest)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    dirs = xdg_dirs()
    ap = argparse.ArgumentParser(description="下载与指令集架构匹配的 xremap 最新发布版本（GNOME Wayland）")
    ap.add_argument("--dest", default=str(dirs["data"] / "bin/xremap"))
    ap.add_argument("--cache", default=str(dirs["cache"]))
    ap.add_argument("--state", default=str(dirs["state"] / "engine.json"))
    ap.add_argument("--tag", default="", help="指定发布标签（默认解析 latest，--print-plan 时建议给出以避免联网）")
    ap.add_argument("--arch", default="", help="覆盖指令集架构（x86_64/aarch64）")
    ap.add_argument("--feature", default="", help="覆盖 xremap 特性（默认 gnome，可指定 full 等）")
    ap.add_argument("--force", action="store_true", help="即使已装同版本也重新下载")
    ap.add_argument("--print-plan", action="store_true", help="只打印计划（JSON），不下载")
    ap.add_argument("--timeout", type=float, default=60.0)
    return ap.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)

    machine = args.arch or platform.machine()
    arch = ARCH_MAP.get(machine.strip().lower())
    if not arch:
        print(f"✗ 不支持的指令集架构：{machine}。上游只发布 x86_64 / aarch64；"
              f"其它架构请用 cargo install xremap --features gnome 自行编译。", file=sys.stderr)
        return 2

    feature, reason = DEFAULT_FEATURE, DEFAULT_FEATURE_REASON
    if args.feature:
        feature, reason = args.feature, "由 --feature 指定"

    dest = Path(args.dest)
    cache = Path(args.cache)
    state = Path(args.state)

    tag = args.tag
    if not tag:
        try:
            tag = resolve_latest_tag(args.timeout)
        except (urllib.error.URLError, OSError, RuntimeError) as exc:
            print(f"✗ 无法解析最新发布版本：{exc}", file=sys.stderr)
            return 1

    plan: InstallPlan = {
        "tag": tag,
        "arch": arch,
        "feature": feature,
        "reason": reason,
        "candidates": [asset_url(tag, arch, f) for f in candidates(feature)],
        "dest": str(dest),
        "cache": str(cache),
        "state": str(state),
    }

    if args.print_plan:
        print(json.dumps(plan, ensure_ascii=False, indent=2))
        return 0

    if not needs_install(dest, state, tag, arch, feature, args.force):
        print(f"✓ 已是最新：{dest}（{tag} / {arch} / {feature}）")
        return 0

    print(f"→ 目标：xremap {tag} / {arch} / {feature}（{reason}）")
    last_error = ""
    for url in plan["candidates"]:
        asset = Path(cache) / "downloads" / Path(url).name
        try:
            print(f"→ 下载：{url}")
            size = download(url, asset, timeout=args.timeout)
            digest = extract_and_install(asset, dest, arch)
        except (urllib.error.URLError, OSError, RuntimeError, zipfile.BadZipFile) as exc:
            last_error = f"{url} → {exc}"
            print(f"  ! 失败：{exc}", file=sys.stderr)
            continue
        write_state(state, {
            "tag": tag, "asset": Path(url).name, "url": url, "sha256": digest,
            "size": size, "arch": arch, "feature": feature,
            "installed_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "dest": str(dest),
        })
        prune_cache(asset.parent, asset)
        print(f"✓ 已安装：{dest}（{size} 字节，sha256 {digest[:16]}…）")
        print(f"  记录：{state}")
        return 0

    print(f"✗ 全部候选都失败：{last_error}", file=sys.stderr)
    if shutil.which("cargo"):
        print("  备选：cargo install xremap --features gnome", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
