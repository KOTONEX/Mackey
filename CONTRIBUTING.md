# 贡献指南

感谢参与 Mackey。这个项目面向「同时使用 macOS 与 Linux」的人，它的价值来自两条硬约束，
所以对改动的要求比对一般脚本项目更严格。

## 两条不可协商的约束

1. **不污染系统目录**：所有落点都在 `$HOME` 内；不写 `/etc`、`/usr`；不装 udev 规则；
   不调用 `sudo`；拒绝以 root 运行。任何新的写入点都必须经过 `bin/mackey` 里的
   `guard_home_path` 检查。
2. **尽量保证 GNOME 功能正常**：不做 Ctrl/⌘ 全局交换；只接管清单里明确列出的键
   （`swallow_ctrl` 属于清单内的点状替换，实现里必须用 `application.not` 排除终端配置档
   与 `except_apps`，否则会毁掉 SIGINT 与内嵌终端）；
   与 GNOME 冲突时优先「把功能迁移到新组合」而不是删除，且只在真的冲突时才改 dconf，
   改前备份、卸载还原。

破坏其中任何一条的改动都不会被合并。

## 开发环境

| 依赖 | 用途 |
|---|---|
| Python 3.10+ | 生成器、焦点桥、键盘识别 |
| `jq` | 生成器报告解析、迁移计划 |
| `gjs` | 扩展契约自测 |
| `gsettings` / `busctl` / `dbus-run-session` | 探测 GNOME 键位、D-Bus |
| `xremap`（见下） | 端到端自测 |

获取引擎（任一即可）：

```bash
./bin/mackey fetch-engine       # 按指令集架构下载 xremap 最新发布版本（固定 gnome 特性），装到 XDG 数据目录
cargo install xremap --features gnome
# 或把发布页下载的二进制放到仓库的 .vendor/xremap（该目录已被 .gitignore 忽略）
```

## 常用命令

```bash
make lint             # bash -n / py_compile / shellcheck / checklist.json 校验 + typecheck
make typecheck        # 类型注解：mypy 查 tools/，tsc --checkJs 查 extension.js 与契约测试
make check            # 离线跑生成器，校验键名与配置组装
make generate         # 依据本机 dconf 重新生成配置与行为清单
make fetch-engine     # 下载 gnome 特性的 xremap 最新发布版本（联网）
make test             # 离线单元测试：focusd + 生成器 + 引擎下载 + 卸载清理 + 安装幂等
make test-contract    # 扩展契约测试（gjs + 真实 D-Bus 往返）
make test-e2e         # 端到端「焦点桥 → 按应用分流」（需 input 组与 /dev/uinput）
make test-all         # 上面三套全跑
make doctor           # 环境体检
```

## 改行为请只改一处

`config/checklist.json` 是行为的**唯一事实源**。新增/修改按键映射、冲突迁移、泛化兜底，
都改这个 JSON，然后运行：
```bash
make generate
```

它会同时更新 `~/.config/mackey/xremap.json`、`relocations.json` 与 `docs/03-行为清单.md`。

每条显式条目必须声明 `basis` 分类：`macos` = 本工具改写成 macOS 行为（`policy` 为 `translate`/`auto` 且 `targets` 非空）；
`gnome` = 保持 GNOME 默认（放行、原生等价、可选、无法模拟）。生成器会硬校验 basis 与 `policy`/`targets` 是否一致，
不一致直接报错。docs/03 的两张主表就是按这个分类展开的。

**不要手工编辑 `docs/03-行为清单.md`**，它是自动生成物。

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

- 提交前至少跑通 `make lint` 与 `make check`；涉及运行时行为的改动请补上
  `make test-contract` / `make test-e2e` 的结果说明。
- PR 描述请填写仓库自带的模板，逐项确认约束检查。
- 发布：推送 `v*` 标签后由 [release.yml](.github/workflows/release.yml) 自动构建并上传
  发布；版本号唯一事实源是仓库根目录的 `VERSION`，标签必须与它一致。

## 自测的边界

- `tests/e2e-app-match.sh` 使用 [`tests/fake_keyboard.py`](tests/fake_keyboard.py) 创建 **uinput 虚拟键盘**，
  xremap 只抓这个虚拟设备：**不会碰真实键盘**。但未被映射、需要透传的键会由 xremap 转发到
  当前焦点窗口（端到端测试里 Ctrl+C 透传正是这样验证的），跑之前先让不敏感的窗口抢焦点。
- 扩展测试通过 sed 只把 `resource:///org/gnome/shell/extensions/extension.js` 一行替换成桩，
  其余代码原样加载，保证测的是真实源码。

## 许可证

本项目以 **AGPL-3.0-or-later** 发布。提交贡献即表示同意以该许可证发布你的贡献；
引入第三方代码或资产前，先确认其许可证与 AGPL-3.0-or-later 兼容，并在
[docs/04-第三方许可证.md](docs/04-第三方许可证.md) 登记。
