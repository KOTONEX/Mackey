#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""Mackey 焦点上报桥（focusd）。

为什么需要它
------------
xremap 要在 GNOME Wayland 上做「按应用区分」的映射（终端 vs 普通应用），
就必须知道当前焦点窗口。它的做法是 D-Bus 调用

    org.gnome.Shell  /com/k0kubun/Xremap  com.k0kubun.Xremap.ActiveWindow() -> s(JSON)

上游扩展 xremap@k0kubun.com 在 GNOME 45+ 上已经失效（xremap 启动日志会打印
`application-client: GNOME (supported: false)`，见 xremap discussion #818），
于是「终端特殊处理」整体失效——这正是最危险的地方：⌘C 会退化成 Ctrl+C 触发 SIGINT。

focusd 做两件事：
1. 把**任意可用的焦点来源**翻译成 xremap 支持的 socket 协议
   （`GNOME_SOCKET=/run/user/1000/mackey-focus.sock xremap ...`），
   这样换来源不需要重新编译 xremap，也不依赖上游扩展的存亡。
2. 提供统一的健康检查（`--test`），供 `bin/mackey doctor` 使用。

后端
----
  k0kubun         : org.gnome.Shell /com/k0kubun/Xremap          com.k0kubun.Xremap.ActiveWindow
                    （本仓库自带 extension/xremap-compat@mackey.local 提供）
  focused-window  : org.gnome.Shell /org/gnome/shell/extensions/FocusedWindow
                    org.gnome.shell.extensions.FocusedWindow.Get
                    （flexagoon/focused-window-dbus、ickyicky/window-calls 一类扩展提供）

socket 协议（xremap 侧实现，见 xremap src/client/gnome_client.rs）：
  请求 一行 JSON：`"ActiveWindow"`  或  `{"Run": ["cmd", ...]}`
  响应 一行 JSON：`{"wm_class": "...", "title": "..."}` 或 `"Ok"`
"""
from __future__ import annotations

import argparse
import json
import os
import socketserver
import sys
import time
from typing import Any, ClassVar, TypedDict


class BackendSpec(TypedDict):
    bus: str
    path: str
    iface: str
    method: str
    hint: str


class FocusInfo(TypedDict):
    """一次焦点查询的结果，也是 socket 协议里返回给 xremap 的 JSON 形状。"""

    wm_class: str
    title: str


BACKENDS: dict[str, BackendSpec] = {
    "k0kubun": {
        "bus": "org.gnome.Shell",
        "path": "/com/k0kubun/Xremap",
        "iface": "com.k0kubun.Xremap",
        "method": "ActiveWindow",
        "hint": "自带的 xremap-compat 扩展（需要重启会话后才会被 GNOME 加载）",
    },
    "focused-window": {
        "bus": "org.gnome.Shell",
        "path": "/org/gnome/shell/extensions/FocusedWindow",
        "iface": "org.gnome.shell.extensions.FocusedWindow",
        "method": "Get",
        "hint": "第三方焦点扩展（focused-window-dbus / window-calls 等）",
    },
}

try:
    import gi
    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib
    HAVE_GI = True
except (ImportError, ValueError):
    HAVE_GI = False

DEFAULT_CACHE_MS = 40
DEFAULT_STATIC_CLASS = "mackey-e2e-static"


class FocusSource:
    """把「取一次焦点应用」抽象出来，支持 PyGObject（快）与 CLI 回退（慢但总是可用）。"""

    def __init__(self, backend: str, cache_ms: int = 40, verbose: bool = False) -> None:
        self.backend_name = backend
        self.spec = BACKENDS[backend]
        self.cache_ms = cache_ms
        self.verbose = verbose
        self._cache: tuple[float, FocusInfo] | None = None
        self._last: FocusInfo | None = None
        self._conn: Any = None
        if HAVE_GI:
            try:
                self._conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            except GLib.Error as exc:  # pragma: no cover
                log(f"D-Bus 连接失败，回退到 busctl：{exc}")

    def _call_gi(self) -> str:
        res = self._conn.call_sync(
            self.spec["bus"], self.spec["path"], self.spec["iface"],
            self.spec["method"], None, None, Gio.DBusCallFlags.NONE, 2000, None,
        )
        return res.unpack()[0]

    def _call_cli(self) -> str | dict[str, Any]:
        import subprocess
        out = subprocess.run(
            ["busctl", "--user", "call", self.spec["bus"], self.spec["path"],
             self.spec["iface"], self.spec["method"]],
            capture_output=True, text=True, timeout=3,
        )
        if out.returncode != 0:
            raise RuntimeError(out.stderr.strip() or "busctl failed")
        # busctl 输出形如：s "{\"wm_class\":\"x\"}"
        raw = out.stdout.strip()
        if raw.startswith("s "):
            raw = raw[2:]
        return json.loads(raw)

    def get(self) -> FocusInfo:
        now = time.monotonic() * 1000
        if self._cache and now - self._cache[0] < self.cache_ms:
            return self._cache[1]
        try:
            payload = self._call_gi() if self._conn else self._call_cli()
            data = json.loads(payload) if isinstance(payload, str) else payload
            result: FocusInfo = {
                "wm_class": data.get("wm_class") or "",
                "title": data.get("title") or "",
            }
        except Exception as exc:  # noqa: BLE001
            # 取焦点失败时绝不能默认成空：空 wm_class 会让 xremap 落到无过滤的
            # generic keymap，终端里的 ⌘C 就会退化成 Ctrl+C（SIGINT）。
            # 宁可短暂沿用最后一次已知结果，并把降级写进日志。
            if self._last is not None:
                log(f"取焦点失败，沿用最后一次结果（{exc}）")
                return self._last
            raise
        self._cache = (now, result)
        self._last = result
        return result


class StaticSource:
    """测试专用：不连任何真实来源，固定返回一个类名。

    用途：让 tests/e2e-app-match.sh 在没有真实焦点来源（比如扩展还没重新登录加载）
    时也能验证「xremap ← socket ← 焦点信息」这条链路。
    真实来源的 D-Bus 契约由 tests/verify-extension.sh 单独覆盖。
    """

    def __init__(self, wm_class: str = DEFAULT_STATIC_CLASS, title: str = "static") -> None:
        self.backend_name = "static"
        self.wm_class = wm_class
        self.title = title

    def get(self) -> FocusInfo:
        return {"wm_class": self.wm_class, "title": self.title}


def log(msg: str) -> None:
    print(f"[focusd] {msg}", file=sys.stderr, flush=True)


def make_source(backend: str, cache_ms: int = DEFAULT_CACHE_MS,
                static_class: str = DEFAULT_STATIC_CLASS) -> FocusSource | StaticSource:
    """按「已解析」的后端名构造焦点来源。

    调用方必须传 pick_backend() 的返回值，而不是 --backend 的原始值：
    "auto" 不是 BACKENDS 里的键，直接构造会 KeyError——那会让「焦点其实可用」
    时服务仍然起不来（脚本会把它误判成焦点不可用而拒绝启动引擎）。
    """
    if backend == "static":
        return StaticSource(static_class)
    return FocusSource(backend, cache_ms=cache_ms)


def pick_backend(preferred: str) -> str | None:
    if preferred == "static":
        return "static"
    order = list(BACKENDS) if preferred == "auto" else [preferred]
    for name in order:
        try:
            src = FocusSource(name)
            info = src.get()
            if src.backend_name:
                log(f"后端 {name} 可用（当前焦点：{info['wm_class'] or '(空)'}）")
                return name
        except Exception as exc:  # noqa: BLE001
            log(f"后端 {name} 不可用：{exc}")
    return None


class Handler(socketserver.StreamRequestHandler):
    source: ClassVar[FocusSource | StaticSource]
    timeout = 10

    def handle(self) -> None:
        line = self.rfile.readline().decode("utf-8", "replace").strip()
        if not line:
            return
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            self.wfile.write(b"\n")
            return

        if request == "ActiveWindow" or (isinstance(request, dict) and "ActiveWindow" in request):
            try:
                payload = json.dumps(self.source.get(), ensure_ascii=False)
            except Exception as exc:  # noqa: BLE001
                log(f"取焦点失败：{exc}")
                payload = json.dumps({"wm_class": "", "title": ""})
        elif isinstance(request, dict) and "Run" in request:
            # 只有配置里显式使用 `{run: ...}` 才会走到这里。
            try:
                import subprocess
                subprocess.Popen(list(request["Run"]))
                payload = json.dumps("Ok")
            except Exception as exc:  # noqa: BLE001
                payload = json.dumps(f"Error: {exc}")
        else:
            payload = json.dumps({"wm_class": "", "title": ""})

        self.wfile.write((payload + "\n").encode("utf-8"))


class Server(socketserver.ThreadingUnixStreamServer):
    daemon_threads = True
    allow_reuse_address = True


def default_socket_path() -> str:
    runtime = os.environ.get("XDG_RUNTIME_DIR") or f"/run/user/{os.getuid()}"
    return os.path.join(runtime, "mackey-focus.sock")


def socket_in_use(path: str) -> bool:
    """socket 文件存在不代表有服务在听：连一下，连得上才是活的。

    旧实现直接 unlink，会把另一个正在服务的 focusd 的路径删掉，
    客户端在 bind 窗口里只会看到 ECONNREFUSED，xremap 静默退化成无过滤。
    """
    import socket as socket_mod
    probe = socket_mod.socket(socket_mod.AF_UNIX, socket_mod.SOCK_STREAM)
    probe.settimeout(0.5)
    try:
        probe.connect(path)
        return True
    except OSError:
        return False
    finally:
        probe.close()


def main() -> int:
    ap = argparse.ArgumentParser(description="Mackey 焦点上报桥")
    ap.add_argument("--backend", default="auto", choices=["auto", "static", *BACKENDS])
    ap.add_argument("--socket", default=default_socket_path())
    ap.add_argument("--cache-ms", type=int, default=DEFAULT_CACHE_MS)
    ap.add_argument("--static-class", default=DEFAULT_STATIC_CLASS,
                    help="--backend static 时固定返回的类名（仅测试用）")
    ap.add_argument("--test", action="store_true", help="打印当前焦点后退出")
    ap.add_argument("--list-backends", action="store_true")
    args = ap.parse_args()

    if args.list_backends:
        for name, spec in BACKENDS.items():
            print(f"{name:16} {spec['bus']} {spec['path']} {spec['iface']}.{spec['method']}")
            print(f"{'':16} {spec['hint']}")
        print(f"{'static':16} 测试专用：固定返回 --static-class，不连任何真实来源")
        return 0

    if args.test:
        backend = pick_backend(args.backend)
        if not backend:
            print("✗ 没有可用的焦点来源", file=sys.stderr)
            return 1
        print(json.dumps(make_source(backend, 0, args.static_class).get(),
                         ensure_ascii=False, indent=2))
        return 0

    backend = pick_backend(args.backend)
    if not backend:
        log("✗ 没有可用的焦点来源；请先安装 xremap-compat 扩展并重启会话，"
            "或安装/启用任意焦点上报扩展")
        return 1

    Handler.source = make_source(backend, args.cache_ms, args.static_class)
    if os.path.exists(args.socket):
        if socket_in_use(args.socket):
            log(f"✗ {args.socket} 已有 focusd 在监听；先停掉它再启动")
            return 1
        os.unlink(args.socket)
    os.makedirs(os.path.dirname(args.socket), exist_ok=True)
    try:
        with Server(args.socket, Handler) as server:
            os.chmod(args.socket, 0o600)
            log(f"监听 {args.socket}（后端 {backend}）")
            try:
                server.serve_forever()
            except KeyboardInterrupt:
                pass
    finally:
        # 退出时清掉自己的 socket，避免留下「看起来有服务」的路径
        try:
            os.unlink(args.socket)
        except OSError:
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
