# Mackey —— 在 GNOME 上沿用 macOS 的快捷键

面向**同时使用 macOS 与 Linux** 的人：在 GNOME（Wayland）上，把 macOS 的 ⌘/⌥ 组合
翻译成「当前应用在 Linux 下的等价按键」。终端与普通应用各用一套映射——**快捷键顺手，
GNOME 自身功能也尽量原样保留**。本项目以 **AGPL-3.0-or-later** 发布，许可全文与第三方
引用登记见 [LICENSE](LICENSE) 与 [文档/04-第三方许可证.md](文档/04-第三方许可证.md)。

两条硬约束（写进代码校验，不是口号）：

1. **不污染系统目录** —— 所有持久文件都在 `$HOME` 内；不写 `/etc`、`/usr`；不装 udev 规则；
   不调用 `sudo`；拒绝以 root 运行。
2. **不破坏 GNOME 功能** —— 不做 Ctrl/⌘ 全局交换；只接管清单里列出的键（含可选的
   `吞控制键` 点状替换）；键位冲突时优先把 GNOME 功能**迁移到新组合**而不是删除，
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
**[文档/03-行为清单.md](文档/03-行为清单.md)** —— 由清单自动生成，不是手抄文档。

## 工作原理

```
物理键盘 ──evdev──▶ xremap ──uinput──▶ GNOME / 应用
                     │ 需要按应用区分时 → focusd（socket）─D-Bus→ mackey-focus 扩展（上报焦点）
                     └ modmap 负责把 ⌘/⌥ 摆到 macOS 的物理位置（可选）
```

选型理由、与其它方案（Kinto / keyd / input-remapper / 纯扩展 / XKB）的对比及验证记录，见
**[文档/01-方案调研.md](文档/01-方案调研.md)** 与 **[文档/02-选型与架构.md](文档/02-选型与架构.md)**。

## 安装

### 一键安装（推荐）

`安装.sh` 会自动下载与你的 **指令集架构**匹配的 xremap 最新发布版本
（Mackey 仅支持 GNOME Wayland，固定取 xremap 的 `gnome` 特性），再安装 GNOME 扩展
与 systemd 用户服务。持久文件全程只写 `$HOME`，不需要 sudo；焦点 socket 使用用户运行时目录。脚本会先校验会话：
非 GNOME 桌面或 X11 会话都会警告并中止（GNOME 50 已移除 X11 会话，本项目不跟随更旧版本 GNOME）。

源码安装先执行 `cargo build --locked --release`（需要 Rust/Cargo 与 C 编译器）；发布二进制本身不需要 Rust、Python 或仓库。

```bash
./安装.sh          # 可选：--不下载 跳过下载、--保持运行 保留旧服务、--确认执行 免确认
```

脚本**可重复执行且幂等**：机器上已有旧引擎 / 旧配置 / 旧服务时，会按当前指令集架构 + 会话类型重新
对齐引擎、重写服务与扩展、清理陈旧 socket 与自启软链，结果与「全新安装」一致
（`--保持运行` 可保留正在运行的服务；`config.json` 里已有的字段值会保留，缺失字段自动补齐）。

脚本结尾会提示必须做的两步：加入 `input` 组（唯一需要提权的动作）并**重新登录**；
重新登录后执行 `mackey 体检 && mackey 应用` 即可。

> `安装` 会把命令行入口装到 `${XDG_BIN_HOME:-~/.local/bin}/mackey`。若该目录不在 PATH：
> fish 执行 `fish_add_path ~/.local/bin`；bash 把 `export PATH="$HOME/.local/bin:$PATH"` 写进
> `~/.bashrc`。在此之前可以直接使用仓库内的 `./命令/mackey`。

### 手动安装（等价的分步）

```bash
# 1) 唯一需要提权的动作：加入 input 组（读取 /dev/input 的唯一办法），然后重新登录
sudo usermod -aG input "$USER"

# 2) 下载引擎（也可以 cargo install xremap --features gnome，或手动放到 .vendor/xremap）
./命令/mackey 获取引擎

# 3) 生成配置（自动识别键盘配列）
./命令/mackey 初始化

# 4) 安装扩展与 systemd 用户服务（全部在 $HOME 内）
./命令/mackey 安装 --不下载

# 5) 重新登录（Wayland 下新装的 GNOME 扩展必须重新登录才会被加载），然后：
./命令/mackey 体检      # 体检
./命令/mackey 应用       # 迁移冲突键位（先备份）+ 启用
```

## 文件落点（遵循 XDG）

| 内容 | 位置 |
| --- | --- |
| 命令行入口（指向独立 Rust 二进制） | `${XDG_BIN_HOME:-~/.local/bin}/mackey` |
| 配置（`config.json`、生成的 `xremap.json`、迁移计划、键位备份） | `${XDG_CONFIG_HOME:-~/.config}/mackey/` |
| 引擎二进制（`获取引擎` 下载） | `${XDG_DATA_HOME:-~/.local/share}/mackey/bin/xremap` |
| 下载与解压的中间产物 | `${XDG_CACHE_HOME:-~/.cache}/mackey/` |
| 安装记录（标签 / 来源 URL / sha256） | `${XDG_STATE_HOME:-~/.local/state}/mackey/engine.json` |
| GNOME 扩展 | `${XDG_DATA_HOME:-~/.local/share}/gnome-shell/extensions/mackey-focus@kotonex` |
| systemd 用户服务 | `${XDG_CONFIG_HOME:-~/.config}/systemd/user/mackey-{focusd,engine}.service` |
| 焦点桥 socket | `${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/mackey-focus.sock` |

`XDG_*` 被显式设置到 `$HOME` 之外时，本工具会拒绝写入——硬约束优先于 XDG。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `mackey 状态` | 当前状态（会话 / 引擎 / 焦点桥 / 扩展 / 服务） |
| `mackey 体检` | 体检：权限、引擎、扩展、焦点来源、冲突、历史遗留的一键排查 |
| `mackey 版本` | 显示版本与引擎路径（等价于 `mackey --version`） |
| `mackey 获取引擎` | 下载与指令集架构匹配的 xremap 最新发布版本（固定 `gnome` 特性；`--标签` 指定版本，`--打印计划` 只看计划） |
| `mackey 探测` | 只读地打印 GNOME 已占用键位与需要迁移的项 |
| `mackey 生成` | 重新生成 xremap 配置 / 迁移计划 / 行为清单 |
| `mackey 应用` | 生成 + 迁移键位（带备份）+ 重启服务 |
| `mackey 还原` | 还原 GNOME 键位 |
| `mackey 停用` | 停用（键盘立刻回到 Linux 原生行为） |
| `mackey 检查` | 完整离线检查：静态检查、差分校验、隔离安装与卸载、① 原生单元（focusd + 生成器 + 引擎下载，不连 D-Bus）② 扩展契约（隔离带加载真实 `extension.js` + 真实 D-Bus 往返）端到端测试另行运行 `mackey 端到端测试`，需要虚拟键盘权限 |
| `mackey 卸载` | 卸载并还原；`--清除配置` 连配置/缓存一起清理，`--预演` 只列清单，`--确认执行` 免确认 |

一键卸载：`./卸载.sh`（默认 `--清除配置`，可加 `--保留配置` / `--预演` / `--确认执行`）。

## 配置

配置文件：`${XDG_CONFIG_HOME:-~/.config}/mackey/config.json`

```jsonc
{
  "修饰键布局": "苹果",   // 苹果 = ⌘ 就是 Super 键；电脑换位 = 交换 Alt/Win，把 ⌘ 摆到空格旁那颗键
  "device": { "only": [] },     // 只重映射指定键盘（名字子串），留空 = 所有键盘
  "keypress_delay_ms": 0,       // 个别应用对合成按键不敏感时可设 20
  "引擎": ""                  // 自定义 xremap 路径（默认用 XDG 数据目录里下载的那份）
}
```

改完执行 `mackey 应用`，它会重新生成配置并重启服务。

## 目录结构

```
Cargo.toml / Cargo.lock            Rust 应用依赖与锁文件
源码/程序/主程序.rs                       clap 命令行入口
源码/库/配置生成.rs                   清单 + dconf → 配置、迁移计划、行为清单
源码/库/焦点桥.rs                      原生 zbus D-Bus → xremap socket
源码/库/引擎下载.rs                     HTTPS 下载、ZIP/ELF 校验、原子安装
源码/库/键盘识别.rs                   键盘识别与布局建议
源码/库/服务.rs                    初始化、安装、备份迁移、服务管理、卸载
源码/库/路径.rs                      XDG 路径、HOME 边界检查、原子写入
命令/mackey                        源码树的薄 Shell 入口
安装.sh / 卸载.sh                 薄安装 / 卸载入口
配置/行为清单.json             行为唯一事实源（编译时嵌入）
扩展/mackey-focus@kotonex/    GNOME Shell 扩展（JS，编译时嵌入）
测试/虚拟键盘.rs          Rust uinput 虚拟键盘（仅测试）
测试/命令行.rs                     独立二进制与 socket 集成测试
测试/基准/                   旧生成器输出，用于差分回归
.github/workflows/                Rust 检查、测试与双架构二进制发布
```

## 环境要求

| 项 | 说明 |
| --- | --- |
| 桌面 | GNOME（Wayland）；不支持 X11 会话与更旧版本 GNOME（GNOME 50 已移除 X11，扩展 API 变动频繁） |
| 引擎 | xremap ≥ 0.15；`安装.sh` / `mackey 获取引擎` 会按指令集架构（x86_64/aarch64）自动下载 `gnome` 特性的发布版本 |
| 构建 | Rust 稳定版、Cargo、C 编译器；`cargo build --locked --release` 构建独立二进制 |
| 运行 | GSettings、systemd 用户会话、GNOME 扩展；不依赖 Python 或源码目录 |
| 权限 | `input` 组（读输入设备）+ `/dev/uinput` 可写（合成按键）；部分镜像（Bazzite / Bluefin / SteamOS 等预装 ydotool）默认已具备 |

## 故障排查

- **按键没变化**：`mackey 状态` 看引擎服务是否在跑；`mackey 日志 50` 看引擎日志。
- **终端里 ⌘C 变成了杀进程**：说明焦点来源没生效（`体检` 里那一项是红色）。先确认扩展是否
  ACTIVE——新装扩展要**重新登录**。这是本方案唯一「静默降级会变危险」的地方，所以专门做了
  红色告警。
- **扩展一直不是 ACTIVE**：Wayland 下 GNOME 不会热扫描扩展目录（实测复制 / 原子 mv 都无效），
  必须重新登录；`gnome-extensions enable <uuid>` 之后仍未加载，就是没重新登录。
- **引擎服务起不来（journal 报 `No input devices`）**：`input` 组是加在会话建立之后才写入
  `/etc/group` 的，重新登录即可（用户服务只在登录时继承该附加组）。绝不要给用户服务写
  `SupplementaryGroups=`：用户管理器没有 `CAP_SETGID`，写了会 `216/GROUP` 崩溃循环。
- **和残留的旧方案冲突**：用 `systemctl --user list-units` 检查是否还在运行其它键位重映射服务，避免同时抓取键盘。

## 明确不做

- 不做 Ctrl↔⌘ 全局交换（modmap 层面；那会让终端失去 SIGINT、摧毁 readline 与作业控制）。
- 只做**点状替换**：`配置/行为清单.json` 的 `吞控制键` 列出的组合（当前 ⌃C/⌃X/⌃V/⌃Z/⌃A）
  在非例外应用里被吞掉，只保留 ⌘ 版本。例外名单 = 终端配置档 + `例外应用`
  （默认含 VS Code / JetBrains / GNOME Builder / Emacs 等带内嵌终端的应用），所以真实终端与
  内嵌终端里的 ⌃C 仍是 SIGINT。自己的编辑器没被覆盖时，把它的 wm_class 正则加进
  `例外应用` 再 `mackey 生成` 即可。
- 不接管 `⌃←/⌃→/⌃↑/⌃↓`（在 Linux 上是「按词移动 / 按段落移动」，接管代价大于收益；
  `⌃↑/⌃↓` 在清单里留作可选）。
- 不模拟 macOS 菜单栏体系（「按住 ⌘ 显示快捷键提示」「⌥⌘Esc 强制退出对话框」）。
- 不写任何系统目录、不需要 root、不装 udev 规则。

## 开发

只固定 Rust Edition 2024，不固定 Rust 工具链版本号；应用版本以 `Cargo.toml` 为唯一事实源。

见 [CONTRIBUTING.md](CONTRIBUTING.md)（含中文提交类型约定）与 [AGENTS.md](AGENTS.md)
（给自动化代理的项目说明）。提交前请跑 `cargo run --quiet -- 检查`。

Rust 主程序由 `cargo check`、`rustfmt` 和 `clippy` 检查；
GNOME 扩展与契约测试不引入打包链，直接在 GJS 源码上启用 `// @ts-check`，
配手写的 `类型声明/GJS环境.d.ts` 描述最小 GJS 类型面，交给严格模式的 `tsc --noEmit` 检查。
两者统一由 `cargo run --quiet -- 类型检查` 驱动，缺少 tsc 会明确失败。
迁移与发布说明见 [文档/05-Rust迁移.md](文档/05-Rust迁移.md)。

## 许可证

本项目以 **AGPL-3.0-or-later** 发布，全文见 [LICENSE](LICENSE)；第三方引用与兼容性说明见
[文档/04-第三方许可证.md](文档/04-第三方许可证.md)。

- 引擎 [xremap](https://github.com/xremap/xremap) 是 MIT，本仓库以独立二进制方式使用，
  未修改其源码。
- GNOME 扩展独立实现了 [xremap-gnome](https://github.com/xremap/xremap-gnome)
  （Copyright (C) 2022 Takashi Kokubun，GPLv2+）的 `com.k0kubun.Xremap` D-Bus 契约；
  GPLv2+ 可升级为 GPLv3，而 GPLv3 与 AGPLv3 可依 AGPLv3 第 13 节组合，
  因此本仓库整体采用 AGPL-3.0-or-later。

## Rust 迁移

既有配置文件路径、GNOME 扩展 UUID、服务名称与 xremap 协议保持兼容；自有命令和参数统一为中文。安装时把清单、扩展和版本嵌入的
Rust 二进制复制到 XDG 数据目录；安装后移动或删除源码目录不会影响服务。源码中的行为清单修改后
需要重新编译并安装；`mackey 生成 --行为清单 <文件>` 可临时使用外部清单。

持久文件均限制在 HOME 下，路径检查解析软链接与 `..`。唯一运行时例外是
`/run/user/$UID/mackey-focus.sock`（权限 0600），用于与 xremap 通信；自定义 socket 必须在 HOME 下。
用户态虚拟键盘测试只访问 `/dev/uinput`，不安装系统规则。迁移细节和验证范围见
[文档/05-Rust迁移.md](文档/05-Rust迁移.md)。

中文接口及旧配置升级说明见 [文档/07-中文接口迁移.md](文档/07-中文接口迁移.md)。
