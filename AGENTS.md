# AGENTS.md

给自动化代理（含 AI 编码助手）的项目说明。人类贡献者请看 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 项目一句话

在 GNOME（Wayland）上把 macOS 的 ⌘/⌥ 组合翻译成当前应用在 Linux 下的等价按键。
引擎是 xremap（evdev→uinput），按应用分流时经 socket 询问 `mackey 焦点桥`，
焦点来自 GNOME 扩展 `mackey-focus@kotonex` 的 D-Bus。

## 目录

```
Cargo.toml / Cargo.lock            Rust 应用与依赖锁文件
源码/程序/主程序.rs                       主命令（clap）
源码/库/{配置生成,焦点桥,引擎下载}.rs     生成器、zbus 焦点桥、HTTPS 引擎下载
源码/库/{服务,路径,键盘识别}.rs    服务、HOME/XDG 边界、键盘识别
命令/mackey                        源码树 Shell 入口（无运行时逻辑）
配置/行为清单.json             行为唯一事实源（编译时嵌入）
扩展/mackey-focus@kotonex/    GNOME 扩展（JavaScript，编译时嵌入）
测试/虚拟键盘.rs          Rust uinput 虚拟键盘（测试专用）
测试/命令行.rs                     二进制/socket 集成测试
测试/基准/                   Python 旧版输出的差分基准
测试/*.sh                        隔离安装、卸载、扩展契约、虚拟键盘端到端测试
.github/workflows/                Rust CI 与 x86_64/aarch64 发布
```

## 硬约束（改代码时必须遵守）

1. 持久文件不写 `$HOME` 之外；不调用 `sudo`；拒绝以 root 运行。
   新的写入点必须走 `源码/库/路径.rs` 的 `保护主目录路径`。
2. 不做 Ctrl/⌘ 全局交换；只接管 `配置/行为清单.json` 列出的键
   （`吞控制组合` 是清单内的点状替换，实现为独立 keymap + `application.not`，
   例外必须包含终端配置档与 `例外应用`，否则会毁掉 SIGINT 与内嵌终端）；
   与 GNOME 冲突时迁移功能（改 dconf 前备份、卸载还原），而不是删除功能。

上述目录和权限约束适用于 Mackey 运行、安装、卸载与本地测试。GitHub 托管的临时 CI runner
可安装系统开发依赖；Mackey 和测试仍以普通用户运行，源码副本与产物位于 runner 的 HOME 内。

## 常用命令

```bash
cargo run --quiet -- 检查             # cargo fmt / clippy / Shell / JSON / tsc
cargo run --quiet -- 类型检查        # cargo check --all-targets + tsc（CI 严格）
cargo run --quiet -- 校验            # 离线运行 mackey 生成 --不探测
cargo run --quiet -- 生成 --文档 "$PWD/文档/03-行为清单.md"         # 依据本机 dconf 重新生成配置与 文档/03-行为清单.md
cargo run --quiet -- 获取引擎     # 下载 gnome 特性的 xremap 最新发布版本（联网）
cargo run --quiet -- 测试             # 离线单元测试：focusd + 生成器 + 引擎下载 + 卸载清理 + 安装幂等（CI 可跑）
cargo run --quiet -- 扩展测试    # bash 测试/扩展契约.sh（需 gjs + D-Bus 会话）
cargo run --quiet -- 端到端测试         # bash 测试/应用分流.sh（需 input 组 + /dev/uinput）
cargo run --quiet -- 全部测试         # Rust + 隔离安装/卸载 + 扩展契约 + 虚拟键盘
```

离线环境或没有 GNOME 会话时：用 `cargo run --quiet -- 校验` 验证生成器；
`cargo run --quiet -- 检查` 不依赖会话。`cargo run --quiet -- 端到端测试` 只使用虚拟键盘，但需要 `input` 组读设备。

## 行为改动流程

1. 只改 `配置/行为清单.json`（映射、冲突迁移、泛化兜底规则）。
   每条显式条目必须声明 `归属`（`macos` = 本工具改写；`gnome` = 保持 GNOME 默认），
   生成器会校验 归属 与 `策略`/`目标组合` 一致，不一致直接报错。
2. 修改嵌入资源后重新编译并安装二进制；运行 `cargo run --quiet -- 生成 --文档 "$PWD/文档/03-行为清单.md"`，它会重写 `xremap.json`、`relocations.json` 与 `文档/03-行为清单.md`。
3. 不要手工编辑 `文档/03-行为清单.md`。

## 已知环境前提

- 命令行入口由 `mackey 安装` 生成在 `${XDG_BIN_HOME:-~/.local/bin}/mackey`（指向独立 Rust 二进制）；
  该目录需位于 PATH 中。仓库内的等价入口是 `./命令/mackey`。
- 引擎默认装在 `${XDG_DATA_HOME:-~/.local/share}/mackey/bin/xremap`（`获取引擎` 下载）；
  `查找引擎` 的查找顺序是：`MACKEY_ENGINE` → 配置里的 `引擎` → 上述路径 → 仓库 `.vendor/xremap` → `PATH`。
- 新装 GNOME 扩展在 Wayland 下**必须重新登录**才会被 shell 扫描到；
  在此之前 `gnome-extensions info` 会显示「不存在」，`mackey 焦点桥 --测试` 会失败。
  这是设计上的保护：焦点来源不可用时 `mackey 启用` 会拒绝启动引擎，
  以免终端里的 ⌘C 退化成 Ctrl+C（SIGINT）。
- `input` 组是读 `/dev/input` 的唯一前提；加入后同样需要重新登录本会话。
- `/dev/uinput` 需要可写（合成按键）；部分发行版已预置 udev 规则。

## 提交

提交信息使用中文类型前缀（`新增:` / `修复:` / `文档:` / `测试:` / `重构:` / `杂务:` / `初始化:`，
见 [CONTRIBUTING.md](CONTRIBUTING.md)）；提交前跑 `cargo run --quiet -- 检查`。

## 用户运行时 IPC 例外

原有 xremap 协议使用 `/run/user/$UID/mackey-focus.sock`；允许在该固定位置创建权限 0600 的
瞬时 Unix socket，不创建系统目录。自定义 socket 与所有测试临时文件必须在 HOME 下。
测试专用 `测试/虚拟键盘.rs` 只向 `/dev/uinput` 写事件，不安装 udev 规则。

## 项目规范与中文接口

- 规范参照 AdwCode；保留 Mackey 的 AGPL-3.0-or-later，不引入其他项目的许可或自动安装行为。
- 自有 API、配置键、命令、参数、文件名和目录采用简体中文，不保留英文命令别名。
- Cargo、Git、GNOME、systemd、XDG、GJS 和 xremap 规定的名称、字段、原始许可文件及
  `mackey` 项目前缀保留原名。部署路径与外部协议文件名保留约定名称，见中文接口文档。
- 只固定 Rust Edition 2024，不固定 Rust 版本号；CI 使用 stable。
- 应用版本唯一事实源为 Cargo.toml；Cargo.lock 纳入版本管理。
- 完成改动必须运行 `cargo run --quiet -- 检查`，同时报告未运行测试的具体原因。
- 不手工维护 CHANGELOG.md；Git 提交标题与版本标签生成 builddir/CHANGELOG.md
  和 builddir/发布说明.md。CI 需要完整检出历史和标签，未提交的改动不会进入日志。
- 提交、推送、打标签与发布分别按用户授权执行，不自行修改全局 Git 身份。

## 发行渠道与接口变更

- 只有已提交到 `main` 的代码可以发布正式发行版。发布工作流以标签提交是否属于远程 `main` 历史为准；其余分支、提交或标签一律标记为预发行（Prerelease），不得标记 Latest。
- 不保留旧版自有命令、配置键或布局值的兼容读取，不自动升级旧配置。检测到旧配置应停止，并引导用户先在旧版本运行卸载脚本、还原键位并清除配置，再安装新版。
- 布局值使用 `苹果`、`微软`、`自动`。按键行为以清单为准。
