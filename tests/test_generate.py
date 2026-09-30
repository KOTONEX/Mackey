#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
"""generate.py 的离线单元测试：不读 dconf、不写用户配置。

覆盖：
  * 清单里每条显式条目都声明了 basis，且与 policy/targets 一致（键位表不会与行为脱节）；
  * 泛化兜底条目一律 basis=macos；
  * 渲染出的文档同时含「遵循 macOS」与「遵循 GNOME 默认」两张分类表，计数与清单一致；
  * `--no-probe` 能跑完整的离线生成，产物合法（CI 可跑）。
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

import generate  # noqa: E402  （路径注入后再导入）

failures = 0


def check(name: str, cond: bool, extra: str = "") -> None:
    global failures
    if cond:
        print(f"  ✓ {name}")
    else:
        failures += 1
        print(f"  ✗ {name} {extra}")


print("generate 单元测试（离线）")

checklist = json.loads((ROOT / "config" / "checklist.json").read_text(encoding="utf-8"))

# 1) basis 校验应通过
bad_basis = generate.check_basis(checklist)
check("check_basis 全部通过", bad_basis == [], f"→ {bad_basis}")

# 2) 独立复算分类规则（不依赖被测函数）
wrong = []
for e in checklist["entries"]:
    expected = "macos" if (
        e.get("policy") in ("translate", "auto") and e.get("targets")
    ) else "gnome"
    if e.get("basis") != expected:
        wrong.append(e["id"])
check("basis 与 policy/targets 一致", not wrong, f"→ {wrong}")

# 3) 负例：改坏一条必须被抓住
broken = json.loads(json.dumps(checklist))
first = broken["entries"][0]
first["basis"] = "gnome" if first["basis"] == "macos" else "macos"
check("check_basis 能抓出不一致", bool(generate.check_basis(broken)))

# 4) 泛化兜底：非空且一律 macos
explicit = {generate.norm(e["trigger"]) for e in checklist["entries"] if e.get("trigger")}
sweep = generate.sweep_entries(checklist, set(), explicit, set())
check("泛化兜底非空", bool(sweep))
check("泛化兜底全部 basis=macos", all(e["basis"] == "macos" for e in sweep))

# 4b) 泛化排除与展示
check("泛化不包含被排除的 ⌘U", all(e["trigger"] != "SUPER-U" for e in sweep))
check("泛化 mac 文案正确（⇧⌘ 而非 ⌘+⇧）",
      all("+⇧" not in e["mac"] for e in sweep) and any("⇧⌘" in e["mac"] for e in sweep))

# 4c) 键位修复的回归点
by_id = {e["id"]: e for e in checklist["entries"]}
check("new 在文件管理器映射 Ctrl+N", by_id["new"]["targets"].get("files") == "C-n")
check("hidden-files 不再全局映射", "default" not in by_id["hidden-files"]["targets"])
check("delete-line-start 用「选中→删除」序列",
      by_id["delete-line-start"]["targets"].get("default") == "S-Home,Delete")
check("emoji 标为近似", by_id["emoji"]["status"] == "approx")
check("新增 fullscreen / comment-toggle / split-editor",
      {"fullscreen", "comment-toggle", "split-editor"} <= set(by_id))

# 4d) 替换模式：吞掉 Linux Ctrl 组合，但终端与内嵌终端应用必须例外
swallow = checklist.get("swallow_ctrl", {})
check("替换模式已启用并覆盖 5 个组合",
      bool(swallow.get("enabled")) and len(swallow.get("triggers", [])) == 5)
check("例外包含终端配置档", "terminal" in swallow.get("except_profiles", []))
check("例外包含内嵌终端应用（VS Code 等）",
      any("code" in m for m in swallow.get("except_apps", [])))

# 4e) 终端吞键与新增键位（本轮修复）
for eid in ("split-editor", "properties", "open", "find-next", "find-prev"):
    check(f"{eid} 在终端里吞掉", by_id[eid]["targets"].get("terminal") == "F24")
check("新增 命令面板 / Find Action / DevTools",
      {"command-palette", "find-action", "devtools", "devtools-console"} <= set(by_id))
check("命令面板映射到 Ctrl+Shift+P",
      by_id["command-palette"]["targets"].get("default") == "C-S-p")
relocs = {r["id"]: r["to"] for r in checklist["relocations"]}
check("应用网格让路到 ⌥⌘A", relocs.get("app-grid") == "Super-A-a")
check("切换显示器让路到 ⌥⌘P", relocs.get("switch-monitor") == "Super-A-p")

# 4f) 静默抢占校验
check("check_occupied_conflicts 能抓出未计划的重合",
      generate.check_occupied_conflicts({"SUPER-A"}, {"SUPER-A"}, set()) == ["SUPER-A"])
check("check_occupied_conflicts 认可已计划的迁移",
      generate.check_occupied_conflicts({"SUPER-A"}, {"SUPER-A"}, {"SUPER-A"}) == [])

# 4g) 键位表必须覆盖「全新 GNOME 默认键位」：离线可跑的可移植性回归。
# 下面这组是编译进 gschema 的默认值里、与本清单吞键触发键真正重合的项；
# 漏掉任何一项，全新系统上 mackey init 会以「冲突但没有迁移计划」直接失败。
PRISTINE_GNOME = {
    "org.gnome.shell.keybindings toggle-application-view": ["<Super>a"],
    "org.gnome.shell.keybindings toggle-message-tray": ["<Super>v", "<Super>m"],
    "org.gnome.shell.keybindings toggle-quick-settings": ["<Super>s"],
    "org.gnome.shell.keybindings focus-active-notification": ["<Super>n"],
    **{f"org.gnome.shell.keybindings switch-to-application-{n}": [f"<Super>{n}"]
       for n in range(1, 10)},
    "org.gnome.mutter.keybindings switch-monitor": ["<Super>p", "XF86Display"],
    "org.gnome.mutter.keybindings toggle-tiled-left": ["<Super>Left"],
    "org.gnome.mutter.keybindings toggle-tiled-right": ["<Super>Right"],
    "org.gnome.desktop.wm.keybindings move-to-monitor-left": ["<Super><Shift>Left"],
    "org.gnome.desktop.wm.keybindings move-to-monitor-right": ["<Super><Shift>Right"],
    "org.gnome.desktop.wm.keybindings move-to-monitor-up": ["<Super><Shift>Up"],
    "org.gnome.desktop.wm.keybindings move-to-monitor-down": ["<Super><Shift>Down"],
    "org.gnome.desktop.wm.keybindings maximize": ["<Super>Up"],
    "org.gnome.desktop.wm.keybindings unmaximize": ["<Super>Down", "<Alt>F5"],
    "org.gnome.desktop.wm.keybindings switch-input-source": ["<Super>space", "XF86Keyboard"],
    "org.gnome.desktop.wm.keybindings show-desktop": ["<Super>d"],
    "org.gnome.settings-daemon.plugins.media-keys screensaver": ["<Super>l"],
}
pristine_table = {ident: "[" + ", ".join(f"'{a}'" for a in accels) + "]"
                  for ident, accels in PRISTINE_GNOME.items()}
pristine_occupied = {generate.canon_accel(a) for accels in PRISTINE_GNOME.values() for a in accels}
pristine_occupied.discard(None)
pristine_plan = generate.relocation_plan(checklist, pristine_occupied, pristine_table)
pristine_planned = {f for p in pristine_plan for f in p["from_list"]}
consuming_now = {generate.norm(e["trigger"]) for e in checklist["entries"]
                 if e.get("trigger") and e.get("targets")
                 and e.get("policy") in ("translate", "auto")}
consuming_now |= {generate.norm(t) for t in checklist.get("swallow_ctrl", {}).get("triggers", [])}
unplanned = generate.check_occupied_conflicts(consuming_now, pristine_occupied, pristine_planned)
check("全新 GNOME 默认键位下没有未计划的冲突", unplanned == [], f"→ {unplanned}")

# 4h) relocate 引用校验
check("check_relocate_refs 对现有清单通过", generate.check_relocate_refs(checklist) == [])
broken_ref = json.loads(json.dumps(checklist))
broken_ref["entries"][0]["relocate"] = "does-not-exist"
check("check_relocate_refs 能抓出悬空引用", generate.check_relocate_refs(broken_ref) != [])

# 4i) 截图输出跟随 GNOME 实测绑定
sc_default = {
    "org.gnome.shell.keybindings screenshot": "['<Shift>Print']",
    "org.gnome.shell.keybindings screenshot-window": "['<Alt>Print']",
    "org.gnome.shell.keybindings show-screenshot-ui": "['Print']",
}
check("截图输出按 GNOME 默认绑定推导（选区/屏幕/窗口）",
      generate.derived_screenshot_targets(sc_default, set()) ==
      {"screenshot-region": "PRINT", "screenshot-all": "S-PRINT", "screenshot-window": "A-PRINT"})
sc_rebound = dict(sc_default)
sc_rebound["org.gnome.shell.keybindings screenshot"] = "['<Control>Print']"
check("GNOME 重绑 screenshot 后输出跟随",
      generate.derived_screenshot_targets(sc_rebound, set())["screenshot-all"] == "C-PRINT")
check("输出与吞键触发重合时跳过",
      generate.derived_screenshot_targets(sc_default, {"PRINT", "S-PRINT", "A-PRINT"}) == {})
check("自指映射（GNOME 截图键就是我们的触发键）仍然可用",
      generate.derived_screenshot_targets(sc_default, {"PRINT", "S-PRINT", "A-PRINT"},
                                          {"S-PRINT"}) == {"screenshot-all": "S-PRINT"})
check("无绑定/非法绑定时返回空（退回清单默认）",
      generate.derived_screenshot_targets({}, set()) == {})
check("老版本 media-keys 的 screenshot / window-screenshot 也认",
      generate.derived_screenshot_targets(
          {"org.gnome.settings-daemon.plugins.media-keys screenshot": "['Print']",
           "org.gnome.settings-daemon.plugins.media-keys window-screenshot": "['<Alt>Print']"},
          set()) == {"screenshot-all": "PRINT", "screenshot-window": "A-PRINT"})

# 4j) 集成：探测到 GNOME 重绑后，写出的 xremap.json 跟着变
_orig_probe = generate.probe_gnome_occupied
try:
    generate.probe_gnome_occupied = lambda: (set(), {
        "org.gnome.shell.keybindings screenshot": "['<Control>Print']",
        "org.gnome.shell.keybindings screenshot-window": "['<Alt>Print']",
        "org.gnome.shell.keybindings show-screenshot-ui": "['Print']",
    }, [])
    with tempfile.TemporaryDirectory() as tmp:
        import contextlib
        import io as _io
        with contextlib.redirect_stdout(_io.StringIO()), contextlib.redirect_stderr(_io.StringIO()):
            rc = generate.main(["--no-docs", "--out-dir", tmp,
                                "--user-config", str(Path(tmp) / "none.json"),
                                "--backup", str(Path(tmp) / "bak.json")])
        cfg = json.loads((Path(tmp) / "xremap.json").read_text(encoding="utf-8"))
        generic = next(k["remap"] for k in cfg["keymap"] if k["name"].startswith("generic："))
        check("探测到重绑后生成的配置跟随（⌘⇧3 选区 / ⌘⇧4 屏幕 / ⌘⇧5 窗口）",
              rc == 0 and generic.get("S-SUPER-3") == "PRINT"
              and generic.get("S-SUPER-4") == "C-PRINT"
              and generic.get("S-SUPER-5") == "A-PRINT",
              f"→ rc={rc} S-SUPER-4={generic.get('S-SUPER-4')}")
finally:
    generate.probe_gnome_occupied = _orig_probe

# 5) 离线渲染：两张分类表都在，计数与清单一致
entries = list(checklist["entries"]) + sweep
doc = generate.render_markdown(entries, [], [], [], checklist.get("swallow_ctrl", {}))
macos = [e for e in checklist["entries"] if e["basis"] == "macos"]
gnome = [e for e in checklist["entries"] if e["basis"] == "gnome"]
check("文档含「遵循 macOS」表", "## 一、遵循 macOS 的键位" in doc)
check("文档含「遵循 GNOME 默认」表", "## 二、遵循 GNOME 默认的键位" in doc)
check("文档含「替换模式」一节", "## 五、替换模式" in doc)
check("文档计数与清单一致",
      f"遵循 macOS **{len(macos)}** 条" in doc and f"遵循 GNOME 默认 **{len(gnome)}** 条" in doc)

# 6) --no-probe 完整离线生成
with tempfile.TemporaryDirectory() as tmp:
    tmpdir = Path(tmp)
    proc = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "generate.py"), "--no-probe",
         "--user-config", str(tmpdir / "nonexistent.json"),
         "--out-dir", tmp, "--docs", str(tmpdir / "行为清单.md")],
        capture_output=True, text=True,
    )
    ok = proc.returncode == 0
    detail = f"→ rc={proc.returncode} {proc.stderr[-200:]}"
    if ok:
        try:
            config = json.loads((tmpdir / "xremap.json").read_text(encoding="utf-8"))
            json.loads((tmpdir / "relocations.json").read_text(encoding="utf-8"))
            keymaps = config.get("keymap", [])
            by_name = {km["name"].split("：")[0]: km for km in keymaps}
            remaps = {name: km.get("remap", {}) for name, km in by_name.items()}
            swallow_nots = by_name.get("swallow", {}).get("application", {}).get("not", [])
            sweep_nots = by_name.get("generic-sweep", {}).get("application", {}).get("not", [])
            swallow_ok = (
                remaps.get("swallow", {}).get("C-C") == "F24"
                and "C-C" not in remaps.get("generic", {})
                and "C-C" not in remaps.get("generic-sweep", {})
                and "C-C" not in remaps.get("terminal", {})
                and remaps.get("generic", {}).get("SUPER-C") == "C-C"
                and remaps.get("terminal", {}).get("SUPER-C") == "C-S-C"
                and remaps.get("generic", {}).get("S-SUPER-3") == "PRINT"
                and remaps.get("generic", {}).get("S-SUPER-4") == "S-PRINT"
                and remaps.get("generic", {}).get("S-SUPER-5") == "A-PRINT"
                and any("Terminal" in m for m in swallow_nots)
                and any("code" in m for m in swallow_nots)
                and any("Terminal" in m for m in sweep_nots)
                and keymaps and keymaps[-1]["name"].startswith("swallow")
            )
            ok = "keymap" in config and swallow_ok and (tmpdir / "行为清单.md").stat().st_size > 0
            detail = f"→ swallow={swallow_ok}"
        except (OSError, json.JSONDecodeError) as exc:
            ok, detail = False, f"→ {type(exc).__name__}: {exc}"
    check("--no-probe 完整生成产物合法", ok, detail)

print()
if failures:
    print(f"✗ {failures} 项未通过")
    sys.exit(1)
print("✓ generate 单元测试通过")
