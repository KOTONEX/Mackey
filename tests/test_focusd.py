#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""focusd 的离线单元测试：不连 D-Bus、不起服务、不注入按键。

重点回归：make_source() 必须接受「已解析」的后端名。
历史 bug —— main() 曾把 --backend 的原始值（默认 "auto"）直接传给 FocusSource，
而 "auto" 不是 BACKENDS 的键，于是在焦点其实可用时仍然 KeyError：
  * 服务（默认 --backend auto）起来就崩溃；
  * `mackey enable` 的 `focusd --test` 前置检查返回非零，误判焦点不可用而拒绝启动引擎。
本测试保证「解析出的后端名 → 构造来源」这条链不会再断。
"""
from __future__ import annotations

import contextlib
import io
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

import focusd  # noqa: E402  （路径注入后再导入）

failures = 0


def check(name: str, cond: bool, extra: str = "") -> None:
    global failures
    if cond:
        print(f"  ✓ {name}")
    else:
        failures += 1
        print(f"  ✗ {name} {extra}")


print("focusd 单元测试（离线）")

# 1) 每个真实后端名都能被 make_source 构造（不会 KeyError）
for name in focusd.BACKENDS:
    try:
        src = focusd.make_source(name, 0)
        check(f"make_source({name!r}) 可构造", src.backend_name == name)
    except Exception as exc:  # noqa: BLE001
        check(f"make_source({name!r}) 可构造", False, f"→ {type(exc).__name__}: {exc}")

# 2) 回归：pick_backend 的返回值可以直接交给 make_source（main() 的实际用法）
try:
    src = focusd.make_source(focusd.pick_backend("static"), 0)
    check("pick_backend -> make_source 串联", src.backend_name == "static")
except Exception as exc:  # noqa: BLE001
    check("pick_backend -> make_source 串联", False, f"→ {type(exc).__name__}: {exc}")

# 3) static 来源返回 e2e 依赖的固定形状（自建 fixture，只取一次）
static_src = focusd.make_source("static", 0)
shape = static_src.get()
check("static 来源形状",
      shape == {"wm_class": focusd.DEFAULT_STATIC_CLASS, "title": "static"},
      f"→ {shape}")

# 4) --test 的 CLI 契约：stdout 是合法 JSON、退出码 0
proc = subprocess.run(
    [sys.executable, str(ROOT / "tools/focusd.py"), "--test", "--backend", "static"],
    capture_output=True, text=True,
)
try:
    payload = json.loads(proc.stdout)
    check("--test --backend static 输出合法 JSON",
          proc.returncode == 0 and payload.get("wm_class") == focusd.DEFAULT_STATIC_CLASS,
          f"→ rc={proc.returncode} out={proc.stdout!r}")
except json.JSONDecodeError as exc:
    check("--test --backend static 输出合法 JSON", False,
          f"→ {exc}: {proc.stdout!r}")

# 5) --list-backends 覆盖所有后端（含 static）
proc = subprocess.run(
    [sys.executable, str(ROOT / "tools/focusd.py"), "--list-backends"],
    capture_output=True, text=True,
)
check("--list-backends 列出全部后端",
      all(name in proc.stdout for name in focusd.BACKENDS) and "static" in proc.stdout)

# 6) 原始 bug 的端到端回归：main() 在默认 --backend auto 下必须使用解析出的后端名。
#    旧实现把 "auto" 直接交给 FocusSource，会 KeyError；这里用 ProbeSource 复现
#    真实构造函数对后端名的解析，再用桩掉的 pick_backend 强制解析到 k0kubun。


class ProbeSource:
    """鸭子类型的 FocusSource 桩（只实现 make_source/main 用到的接口）。"""

    def __init__(self, backend: str, cache_ms: int = focusd.DEFAULT_CACHE_MS,
                 **kwargs: object) -> None:
        self.spec = focusd.BACKENDS[backend]
        self.backend_name = backend

    def get(self) -> focusd.FocusInfo:
        return {"wm_class": f"probe-{self.backend_name}", "title": ""}


orig_argv = sys.argv
orig_pick = focusd.pick_backend
orig_focus = focusd.FocusSource
try:
    focusd.pick_backend = lambda preferred: "k0kubun"
    focusd.FocusSource = ProbeSource
    sys.argv = ["focusd.py", "--test"]
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        rc = focusd.main()
    payload = json.loads(out.getvalue())
    check("main --test 默认 auto 用解析后的后端（原始 KeyError 回归）",
          rc == 0 and payload.get("wm_class") == "probe-k0kubun",
          f"→ rc={rc} out={out.getvalue()!r}")
except Exception as exc:  # noqa: BLE001
    check("main --test 默认 auto 用解析后的后端（原始 KeyError 回归）", False,
          f"→ {type(exc).__name__}: {exc}")
finally:
    sys.argv = orig_argv
    focusd.pick_backend = orig_pick
    focusd.FocusSource = orig_focus

print()
if failures:
    print(f"✗ {failures} 项未通过")
    sys.exit(1)
print("✓ focusd 单元测试通过")
