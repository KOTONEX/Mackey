# Mackey —— 在 GNOME 上沿用 macOS 的快捷键

面向**同时使用 macOS 与 Linux** 的人：在 GNOME（Wayland 或 X11）上，把 macOS 的 ⌘/⌥ 组合
翻译成「当前应用在 Linux 下的等价按键」。终端与普通应用各用一套映射——**快捷键顺手，
GNOME 自身功能也尽量原样保留**。本项目以 **AGPL-3.0-or-later** 发布，许可全文与第三方
引用登记见 [LICENSE](LICENSE) 与 [docs/04-第三方许可证.md](docs/04-第三方许可证.md)。

两条硬约束（写进代码校验，不是口号）：

1. **不污染系统目录** —— 所有落点都在 `$HOME` 内；不写 `/etc`、`/usr`；不装 udev 规则；
   不调用 `sudo`；拒绝以 root 运行。
2. **不破坏 GNOME 功能** —— 不做 Ctrl/⌘ 全局交换；只接管清单里列出的键（含可选的
   `swallow_ctrl` 点状替换）；键位冲突时优先把 GNOME 功能**迁移到新组合**而不是删除，
   并且只在真的冲突时才改 dconf：改前备份、卸载还原。

## 它解决的核心问题

macOS 用户最容易踩的坑是**终端**：`⌘C` 在 macOS 终端里是「复制」，而在 Linux 终端里
`Ctrl+C` 是 **SIGINT**——「把所有 ⌘ 一律映射成 Ctrl」会在终端里杀掉前台进程。
Mackey 按焦点应用分三套映射：

| 配置档 | 覆盖对象 | 差异示例 |
| --- | --- | --- |
| `terminal` | GNOME Terminal / Console / Ghostty / kitty / Alacritty / WezTerm / foot / xterm… | ⌘C→`Ctrl+Shift+C`、⌘V→`Ctrl+Shift+V`、⌘W→`Ctrl+Shift+W`、⌘1..9→`Alt+1..9`；危险键显式吞掉（⌘D、⌘Z、⌘R，否则会变成 EOF / 挂起 / 反向搜索） |
| `files` | Nautilus / Dolphin / Thunar / Nemo… | ⌘I→`Alt+Enter`（属性）、⌘⌫→`Delete`（移到废纸篓） |
| `generic` | 其余应用（浏览器、编辑器等） | ⌘C→`Ctrl+C`、⌘←/→→`Home/End`、⌥←/→→`Ctrl+←/→`、⌘⇧[/]→`Ctrl+PgUp/PgDn`、⌘[/]→`Alt+←/→`；其余 ⌘+字母做泛化翻译，并与 GNOME 已占用键位自动互斥 |

完整键位表（按「遵循 macOS / 遵循 GNOME 默认」分类，并标注 GNOME 已占用键位的归属）见
**[docs/03-行为清单.md](docs/03-行为清单.md)** —— 由清单自动生成，不是手抄文档。

## 工作原理

```
物理键盘 ──evdev──▶ xremap ──uinput──▶ GNOME / 应用
                     │ 需要按应用区分时 → focusd（socket）─D-Bus→ xremap-compat 扩展（上报焦点）
                     └ modmap 负责把 ⌘/⌥ 摆到 macOS 的物理位置（可选）
```

选型理由、与其它方案（Kinto / keyd / input-remapper / 纯扩展 / XKB）的对比及验证记录，见
**[docs/01-方案调研.md](docs/01-方案调研.md)** 与 **[docs/02-选型与架构.md](docs/02-选型与架构.md)**。

## 安装

### 一键安装（推荐）

`install.sh` 会自动下载与你的 **指令集架构 + 桌面环境**匹配的 xremap 最新发布版本，再安装 GNOME 扩展
与 systemd 用户服务。全程只写 `$HOME`，不需要 sudo。

```bash
./install.sh          # 可选：--no-fetch 跳过下载、--keep-running 保留旧服务、--yes 免确认
```

脚本**可重复执行且幂等**：机器上已有旧引擎 / 旧配置 / 旧服务时，会按当前指令集架构 + 桌面环境重新
对齐引擎、重写服务与扩展、清理陈旧 socket 与自启软链，结果与「全新安装」一致
（`--keep-running` 可保留正在运行的服务；`config.json` 里已有的字段值会保留，缺失字段自动补齐）。

脚本结尾会提示必须做的两步：加入 `input` 组（唯一需要提权的动作）并**重新登录**；
重新登录后执行 `mackey doctor && mackey apply` 即可。

> `install` 会把命令行入口装到 `${XDG_BIN_HOME:-~/.local/bin}/mackey`。若该目录不在 PATH：
> fish 执行 `fish_add_path ~/.local/bin`；bash 把 `export PATH="$HOME/.local/bin:$PATH"` 写进
> `~/.bashrc`。在此之前可以直接使用仓库内的 `./bin/mackey`。

### 手动安装（等价的分步）

```bash
# 1) 唯一需要提权的动作：加入 input 组（读取 /dev/input 的唯一办法），然后重新登录
sudo usermod -aG input "$USER"

# 2) 下载引擎（也可以 cargo install xremap --features gnome，或手动放到 .vendor/xremap）
./bin/mackey fetch-engine

# 3) 生成配置（自动识别键盘配列）
./bin/mackey init

# 4) 安装扩展与 systemd 用户服务（全部在 $HOME 内）
./bin/mackey install --no-fetch

# 5) 重新登录（Wayland 下新装的 GNOME 扩展必须重新登录才会被加载），然后：
./bin/mackey doctor      # 体检
./bin/mackey apply       # 迁移冲突键位（先备份）+ 启用
```

## 文件落点（遵循 XDG）

| 内容 | 位置 |
| --- | --- |
| 命令行入口（`install` 生成，指向本仓库） | `${XDG_BIN_HOME:-~/.local/bin}/mackey` |
| 配置（`config.json`、生成的 `xremap.json`、迁移计划、键位备份） | `${XDG_CONFIG_HOME:-~/.config}/mackey/` |
| 引擎二进制（`fetch-engine` 下载） | `${XDG_DATA_HOME:-~/.local/share}/mackey/bin/xremap` |
| 下载与解压的中间产物 | `${XDG_CACHE_HOME:-~/.cache}/mackey/` |
| 安装记录（标签 / 来源 URL / sha256） | `${XDG_STATE_HOME:-~/.local/state}/mackey/engine.json` |
| GNOME 扩展 | `${XDG_DATA_HOME:-~/.local/share}/gnome-shell/extensions/xremap-compat@mackey.local` |
| systemd 用户服务 | `${XDG_CONFIG_HOME:-~/.config}/systemd/user/mackey-{focusd,engine}.service` |
| 焦点桥 socket | `${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/mackey-focus.sock` |

`XDG_*` 被显式设置到 `$HOME` 之外时，本工具会拒绝写入——硬约束优先于 XDG。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `mackey status` | 当前状态（会话 / 引擎 / 焦点桥 / 扩展 / 服务） |
| `mackey doctor` | 体检：权限、引擎、扩展、焦点来源、冲突、历史遗留的一键排查 |
| `mackey version` | 显示版本与引擎路径（等价于 `mackey --version`） |
| `mackey fetch-engine` | 下载与指令集架构/桌面匹配的 xremap 最新发布版本（`--tag` 指定版本，`--print-plan` 只看计划） |
| `mackey probe` | 只读地打印 GNOME 已占用键位与需要迁移的项 |
| `mackey generate` | 重新生成 xremap 配置 / 迁移计划 / 行为清单 |
| `mackey apply` | 生成 + 迁移键位（带备份）+ 重启服务 |
| `mackey revert` | 还原 GNOME 键位 |
| `mackey disable` | 停用（键盘立刻回到 Linux 原生行为） |
| `mackey test` | 三套自测：① 离线单元（focusd + 生成器 + 引擎下载，不连 D-Bus）② 扩展契约（隔离带加载真实 `extension.js` + 真实 D-Bus 往返）③ 端到端「焦点桥 → xremap 按应用分流」（只用独立 uinput 虚拟键盘，不碰真实键盘；未被映射的键会经 xremap 转发到当前焦点窗口） |
| `mackey uninstall` | 卸载并还原；`--purge` 连配置/缓存一起清理，`--dry-run` 只列清单，`--yes` 免确认 |

一键卸载：`./uninstall.sh`（默认 `--purge`，可加 `--keep-config` / `--dry-run` / `--yes`）。

## 配置

配置文件：`${XDG_CONFIG_HOME:-~/.config}/mackey/config.json`

```jsonc
{
  "modifier_layout": "apple",   // apple = ⌘ 就是 Super 键；pc-swap = 交换 Alt/Win，把 ⌘ 摆到空格旁那颗键
  "device": { "only": [] },     // 只重映射指定键盘（名字子串），留空 = 所有键盘
  "keypress_delay_ms": 0,       // 个别应用对合成按键不敏感时可设 20
  "engine": ""                  // 自定义 xremap 路径（默认用 XDG 数据目录里下载的那份）
}
```

改完执行 `mackey generate && mackey enable`。

## 目录结构

```
install.sh                         一键安装：下载引擎 → 生成配置 → 安装扩展与服务
uninstall.sh                       一键卸载：停服务 → 还原键位 → 清理全部相关文件
VERSION                            版本号（mackey version 读取它）
bin/mackey                         主命令（bash），所有子命令的唯一入口
config/checklist.json              行为清单 = 唯一事实源（改行为只改这里）
tools/generate.py                  清单 + dconf 探测 → xremap 配置 / 迁移计划 / 行为清单文档
tools/fetch-engine.py              按指令集架构 + 桌面环境下载 xremap 最新发布版本（XDG 落点）
tools/focusd.py                    焦点上报桥（把任意焦点来源翻译成 xremap 的 socket 协议）
tools/detect-keyboard.py           识别主键盘并建议 modifier_layout
extension/xremap-compat@…/         最小 GNOME 扩展：只报焦点窗口，不抓键、不注入
.github/workflows/                 CI（静态检查 + 单元测试 + 扩展契约）与发布（标签自动构建）
tests/fake_keyboard.py             uinput 虚拟键盘（自测用，不碰真实键盘）
tests/test_focusd.py               focusd 离线单元测试（不连 D-Bus）
tests/test_generate.py             生成器离线单元测试（basis 分类 / 文档渲染 / 完整生成）
tests/test_fetch_engine.py         引擎下载器离线单元测试（指令集架构/桌面映射、zip 校验、原子安装）
tests/test-uninstall-cleanup.sh    隔离带验证卸载清理（沙箱 HOME + 桩 systemctl/gsettings）
tests/test-install-idempotency.sh  隔离带验证安装幂等（干净安装 vs 旧状态重装，逐字节比较）
tests/e2e-app-match.sh             端到端「焦点桥 → xremap 按应用分流」验证
tests/e2e-swallow-exempt.sh        端到端验证替换模式的应用例外（终端/IDE 保留 ⌃C）
tests/verify-extension.sh          隔离带验证扩展契约（真实 extension.js + 真实 D-Bus）
tests/verify-extension.mjs         上面那个测试的 GJS 本体
docs/01-方案调研.md                 现有方案横评
docs/02-选型与架构.md               选型理由、决策表、验证记录、已知限制
docs/03-行为清单.md                 自动生成的逐条对照表
docs/04-第三方许可证.md             引用项目的许可证登记与兼容性说明
```

## 环境要求

| 项 | 说明 |
| --- | --- |
| 桌面 | GNOME（Wayland / X11 均可）；其它桌面需更换焦点来源后端 |
| 引擎 | xremap ≥ 0.15；`install.sh` / `mackey fetch-engine` 会按指令集架构（x86_64/aarch64）与桌面环境自动下载对应发布版本（gnome/kde/hypr/wlroots/niri/cosmic/pantheon/x11） |
| Python | 3.10+；有 PyGObject 时进程内调用 D-Bus，没有则回退 `busctl`（CI 使用 3.14） |
| 权限 | `input` 组（读输入设备）+ `/dev/uinput` 可写（合成按键）；部分镜像（Bazzite / Bluefin / SteamOS 等预装 ydotool）默认已具备 |

## 故障排查

- **按键没变化**：`mackey status` 看引擎服务是否在跑；`mackey logs 50` 看引擎日志。
- **终端里 ⌘C 变成了杀进程**：说明焦点来源没生效（`doctor` 里那一项是红色）。先确认扩展是否
  ACTIVE——新装扩展要**重新登录**。这是本方案唯一「静默降级会变危险」的地方，所以专门做了
  红色告警。
- **扩展一直不是 ACTIVE**：Wayland 下 GNOME 不会热扫描扩展目录（实测复制 / 原子 mv 都无效），
  必须重新登录；`gnome-extensions enable <uuid>` 之后仍未加载，就是没重新登录。
- **引擎服务起不来（journal 报 `No input devices`）**：`input` 组是加在会话建立之后才写入
  `/etc/group` 的，重新登录即可（用户服务只在登录时继承该附加组）。绝不要给用户服务写
  `SupplementaryGroups=`：用户管理器没有 `CAP_SETGID`，写了会 `216/GROUP` 崩溃循环。
- **和残留的旧方案冲突**：`mackey doctor` 会检测历史遗留（例如反复启动失败的
  `gnome-macos-remap.service`），提示用 `systemctl --user disable --now gnome-macos-remap` 清掉。

## 明确不做

- 不做 Ctrl↔⌘ 全局交换（modmap 层面；那会让终端失去 SIGINT、摧毁 readline 与作业控制）。
- 只做**点状替换**：`config/checklist.json` 的 `swallow_ctrl` 列出的组合（当前 ⌃C/⌃X/⌃V/⌃Z/⌃A）
  在非例外应用里被吞掉，只保留 ⌘ 版本。例外名单 = 终端配置档 + `except_apps`
  （默认含 VS Code / JetBrains / GNOME Builder / Emacs 等带内嵌终端的应用），所以真实终端与
  内嵌终端里的 ⌃C 仍是 SIGINT。自己的编辑器没被覆盖时，把它的 wm_class 正则加进
  `except_apps` 再 `mackey generate` 即可。
- 不接管 `⌃←/⌃→/⌃↑/⌃↓`（在 Linux 上是「按词移动 / 按段落移动」，接管代价大于收益；
  `⌃↑/⌃↓` 在清单里留作可选）。
- 不模拟 macOS 菜单栏体系（「按住 ⌘ 显示快捷键提示」「⌥⌘Esc 强制退出对话框」）。
- 不写任何系统目录、不需要 root、不装 udev 规则。

## 开发

见 [CONTRIBUTING.md](CONTRIBUTING.md)（含中文提交类型约定）与 [AGENTS.md](AGENTS.md)
（给自动化代理的项目说明）。提交前请跑 `make lint && make check`。

代码带类型注解，并由静态检查把关：`tools/` 下的 Python 以 mypy 严格模式检查；
GNOME 扩展与契约测试不引入打包链，直接在 GJS 源码上启用 `// @ts-check`，
配手写的 `types/gjs.d.ts` 描述最小 GJS 类型面，交给 `tsc --noEmit` 检查。
两者统一由 `make typecheck` 驱动：本机缺工具时跳过并提示，
CI 用 `MACKEY_TYPECHECK_STRICT=1` 强制，不允许静默退化。

## 许可证

本项目以 **AGPL-3.0-or-later** 发布，全文见 [LICENSE](LICENSE)；第三方引用与兼容性说明见
[docs/04-第三方许可证.md](docs/04-第三方许可证.md)。

- 引擎 [xremap](https://github.com/xremap/xremap) 是 MIT，本仓库以独立二进制方式使用，
  未修改其源码。
- GNOME 扩展独立实现了 [xremap-gnome](https://github.com/xremap/xremap-gnome)
  （Copyright (C) 2022 Takashi Kokubun，GPLv2+）的 `com.k0kubun.Xremap` D-Bus 契约；
  GPLv2+ 可升级为 GPLv3，而 GPLv3 与 AGPLv3 可依 AGPLv3 第 13 节组合，
  因此本仓库整体采用 AGPL-3.0-or-later。
