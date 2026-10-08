# 贡献指南

感谢参与 Mackey。这个项目面向「同时使用 macOS 与 Linux」的人，它的价值来自两条硬约束，
所以对改动的要求比对一般项目更严格。

## 两条不可协商的约束

1. **不污染系统目录**：所有持久文件都在 `$HOME` 内；不写 `/etc`、`/usr`；不装 udev 规则；
   不调用 `sudo`；拒绝以 root 运行。任何新的写入点都必须经过 `源码/库/路径.rs` 里的
   `保护主目录路径` 检查。
2. **尽量保证 GNOME 功能正常**：不做 Ctrl/⌘ 全局交换；只接管清单里明确列出的键
   （`吞控制组合` 属于清单内的点状替换，实现里必须用 `application.not` 排除终端配置档
   与 `例外应用`，否则会毁掉 SIGINT 与内嵌终端）；
   与 GNOME 冲突时优先「把功能迁移到新组合」而不是删除，且只在真的冲突时才改 dconf，
   改前备份、卸载还原。

破坏其中任何一条的改动都不会被合并。

这些目录和权限约束适用于 Mackey 运行、安装、卸载与本地测试。GitHub 托管的临时 CI runner
可安装检查所需的系统开发依赖；Mackey 及其测试仍以普通用户运行，测试源码和产物放在 HOME 内。

## 开发环境

| 依赖 | 用途 |
|---|---|
| Rust 稳定版 / Cargo / C 编译器 | 编译主程序、原生焦点桥、虚拟键盘测试 |
| TypeScript（`tsc`）、ShellCheck | 扩展严格类型与 Shell 静态检查 |
| `jq` | 生成器报告解析、迁移计划 |
| `gjs` | 扩展契约自测 |
| `gsettings` / `dbus-run-session` | 探测 GNOME 键位、D-Bus |
| `xremap`（见下） | 端到端自测 |

获取引擎（任一即可）：

```bash
./命令/mackey 获取引擎       # 按指令集架构下载 xremap 最新发布版本（固定 gnome 特性），装到 XDG 数据目录
cargo install xremap --features gnome
# 或把发布页下载的二进制放到仓库的 .vendor/xremap（该目录已被 .gitignore 忽略）
```

## 常用命令

```bash
cargo run --quiet -- 检查             # cargo fmt / clippy / Shell / JSON / JS 类型检查
cargo run --quiet -- 类型检查        # cargo check --all-targets + tsc 检查扩展与契约测试
cargo run --quiet -- 校验            # 离线跑生成器，校验键名与配置组装
cargo run --quiet -- 生成 --文档 "$PWD/文档/03-行为清单.md"         # 依据本机 dconf 重新生成配置与行为清单
cargo run --quiet -- 获取引擎     # 下载 gnome 特性的 xremap 最新发布版本（联网）
cargo run --quiet -- 测试             # 离线单元测试：focusd + 生成器 + 引擎下载 + 卸载清理 + 安装幂等
cargo run --quiet -- 扩展测试    # 扩展契约测试（gjs + 真实 D-Bus 往返）
cargo run --quiet -- 端到端测试         # 端到端「焦点桥 → 按应用分流」（需 input 组与 /dev/uinput）
cargo run --quiet -- 全部测试         # 上面三套全跑
cargo run --quiet -- 体检           # 环境体检
```

## 改行为请只改一处

`配置/行为清单.json` 是行为的**唯一事实源**。新增/修改按键映射、冲突迁移、泛化兜底，
都改这个 JSON，然后运行：
```bash
cargo run --quiet -- 生成 --文档 "$PWD/文档/03-行为清单.md"
```

清单嵌入二进制，修改后先运行 `cargo build --locked`；部署时重新 `mackey 安装`。生成命令会同时更新 `~/.config/mackey/xremap.json`、`relocations.json` 与 `文档/03-行为清单.md`。

每条显式条目必须声明 `归属` 分类：`macos` = 本工具改写成 macOS 行为（`策略` 为 `translate`/`auto` 且 `目标组合` 非空）；
`gnome` = 保持 GNOME 默认（放行、原生等价、可选、无法模拟）。生成器会硬校验 归属 与 `策略`/`目标组合` 是否一致，
不一致直接报错。文档/03 的两张主表就是按这个分类展开的。

**不要手工编辑 `文档/03-行为清单.md`**，它是自动生成物。

## 提交与 PR

- 提交信息使用**简体中文类型前缀**（格式沿用 Conventional Commits：`类型: 描述`，半角冒号 + 空格）：

  | 类型 | 用途 | 对应英文约定 |
  | --- | --- | --- |
  | `新增:` | 新功能 / 新行为 | feat |
  | `修复:` | 修 bug、修行为偏差 | fix |
  | `文档:` | 只改文档 | docs |
  | `测试:` | 只改测试 | test |
  | `重构:` | 不改变行为的重构 | refactor |
  | `杂务:` | 构建、依赖、CI、清理等 | chore |
  | `初始化:` | 仓库 / 模块的初始提交 | init |

- 提交前至少跑通 `cargo run --quiet -- 检查` 与 `cargo run --quiet -- 校验`；涉及运行时行为的改动请补上
  `cargo run --quiet -- 扩展测试` / `cargo run --quiet -- 端到端测试` 的结果说明。
- PR 描述请填写仓库自带的模板，逐项确认约束检查。
- 发布：推送 `v*` 标签后由 [release.yml](.github/workflows/release.yml) 自动构建并上传
  发布；应用版本以 `Cargo.toml` 为唯一事实源，发布标签必须是 `v<版本>`。

## 自测的边界

- `测试/应用分流.sh` 使用 [`测试/虚拟键盘.rs`](测试/虚拟键盘.rs) 创建 **uinput 虚拟键盘**，
  xremap 只抓这个虚拟设备：**不会碰真实键盘**。但未被映射、需要透传的键会由 xremap 转发到
  当前焦点窗口（端到端测试里 Ctrl+C 透传正是这样验证的），跑之前先让不敏感的窗口抢焦点。
- 扩展测试通过 sed 只把 `resource:///org/gnome/shell/extensions/extension.js` 一行替换成桩，
  其余代码原样加载，保证测的是真实源码。

## 许可证

本项目以 **AGPL-3.0-or-later** 发布。提交贡献即表示同意以该许可证发布你的贡献；
引入第三方代码或资产前，先确认其许可证与 AGPL-3.0-or-later 兼容，并在
[文档/04-第三方许可证.md](文档/04-第三方许可证.md) 登记。

焦点 socket 的固定用户运行时路径是 HOME 约束的唯一 IPC 例外，见 `源码/库/路径.rs::保护套接字`。测试临时目录也必须在 HOME 下。

## 版本与日志

只固定 Rust Edition 2024，不固定 Rust 工具链版本号。应用版本只修改 Cargo.toml，
提交 Cargo.lock。`cargo run --quiet -- 变更日志` 和 `cargo run --quiet -- 发布说明`
根据完整 Git 历史及版本标签生成 builddir/ 下的文档；打包会自动收录，未提交修改不进入日志。
自有接口及卸载重装说明见 [中文接口迁移](文档/07-中文接口迁移.md)。

正式发行版只发布已提交到 `main` 的代码；其他分支或标签发布时必须标记为预发行，且不得标记 Latest。
标签工作流检查提交是否属于远程 `main` 历史，不凭版本号推断正式状态。
接口变更不添加旧别名或配置转换层；引导用户先运行旧版卸载脚本，再安装新版。
