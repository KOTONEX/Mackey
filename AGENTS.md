# AGENTS.md

给自动化代理（含 AI 编码助手）的项目说明。人类贡献者请看 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 项目一句话

在 GNOME（Wayland/X11）上把 macOS 的 ⌘/⌥ 组合翻译成当前应用在 Linux 下的等价按键。
引擎是 xremap（evdev→uinput），按应用分流时经 socket 询问 `tools/focusd.py`，
焦点来自 GNOME 扩展 `xremap-compat@mackey.local` 的 D-Bus。

## 目录

```
install.sh                         一键安装：下载引擎 → init → install
uninstall.sh                       一键卸载：默认 uninstall --purge
VERSION                            版本号（mackey version 读取它）
bin/mackey                         主命令（bash），所有子命令的唯一入口
config/checklist.json              行为清单 = 唯一事实源
tools/generate.py                  清单 + dconf 探测 → xremap 配置 / 迁移计划 / 行为清单文档
tools/fetch-engine.py              按指令集架构 + 桌面下载 xremap 最新发布版本（XDG 落点）
tools/focusd.py                    焦点上报桥（D-Bus → xremap socket 协议）
tools/detect-keyboard.py           识别主键盘并建议 modifier_layout
extension/xremap-compat@…/         最小 GNOME 扩展：只报焦点窗口
.github/workflows/                 CI（静态检查 + 单元测试 + 扩展契约）与发布（标签自动构建）
tests/fake_keyboard.py             uinput 虚拟键盘（自测用）
tests/test_focusd.py               focusd 离线单元测试（不连 D-Bus）
tests/test_generate.py             生成器离线单元测试（basis 分类 / 文档渲染 / 完整生成）
tests/test_fetch_engine.py         引擎下载器离线单元测试（指令集架构/桌面映射、zip 校验、原子安装）
tests/test-uninstall-cleanup.sh    隔离带验证卸载清理（沙箱 HOME + 桩 systemctl/gsettings）
tests/test-install-idempotency.sh  隔离带验证安装幂等（干净安装 vs 旧状态重装，逐字节比较）
tests/e2e-app-match.sh             端到端「焦点桥 → xremap 按应用分流」
tests/e2e-swallow-exempt.sh        端到端验证替换模式的例外（终端/IDE 保留 Ctrl+C）
tests/verify-extension.sh          隔离带验证扩展契约（真实 extension.js + 真实 D-Bus）
tests/verify-extension.mjs         上面的 GJS 本体
docs/01-方案调研.md                 现有方案横评
docs/02-选型与架构.md               选型理由、验证记录、已知限制
docs/03-行为清单.md                 自动生成，禁止手工编辑
docs/04-第三方许可证.md             引用项目的许可证登记与兼容性说明
```

## 硬约束（改代码时必须遵守）

1. 不写 `$HOME` 之外的任何路径；不调用 `sudo`；拒绝以 root 运行。
   新的写入点必须走 `bin/mackey` 的 `guard_home_path`。
2. 不做 Ctrl/⌘ 全局交换；只接管 `config/checklist.json` 列出的键
   （`swallow_ctrl` 是清单内的点状替换，实现为独立 keymap + `application.not`，
   例外必须包含终端配置档与 `except_apps`，否则会毁掉 SIGINT 与内嵌终端）；
   与 GNOME 冲突时迁移功能（改 dconf 前备份、卸载还原），而不是删除功能。

## 常用命令

```bash
make lint             # bash -n / py_compile / shellcheck（可选）/ checklist.json 校验 + typecheck
make typecheck        # 类型注解：mypy 查 tools/，tsc --checkJs 查 extension.js 与契约测试（缺工具时跳过，CI 严格）
make check            # 离线跑 tools/generate.py --no-probe，校验键名与配置组装
make generate         # 依据本机 dconf 重新生成配置与 docs/03-行为清单.md
make fetch-engine     # 下载与指令集架构/桌面匹配的 xremap 最新发布版本（联网）
make test             # 离线单元测试：focusd + 生成器 + 引擎下载 + 卸载清理 + 安装幂等（CI 可跑）
make test-contract    # bash tests/verify-extension.sh（需 gjs + D-Bus 会话）
make test-e2e         # bash tests/e2e-app-match.sh（需 input 组 + /dev/uinput）
make test-all         # ./bin/mackey test（三套全跑）
```

离线环境或没有 GNOME 会话时：用 `make check` 验证生成器；
`make lint` 不依赖会话。`make test-e2e` 只使用虚拟键盘，但需要 `input` 组读设备。

## 行为改动流程

1. 只改 `config/checklist.json`（映射、冲突迁移、泛化兜底规则）。
   每条显式条目必须声明 `basis`（`macos` = 本工具改写；`gnome` = 保持 GNOME 默认），
   生成器会校验 basis 与 `policy`/`targets` 一致，不一致直接报错。
2. 运行 `make generate`，它会重写 `xremap.json`、`relocations.json` 与 `docs/03-行为清单.md`。
3. 不要手工编辑 `docs/03-行为清单.md`。

## 已知环境前提

- 命令行入口由 `mackey install` 生成在 `${XDG_BIN_HOME:-~/.local/bin}/mackey`（指向本仓库）；
  该目录不一定在 PATH 里，`doctor` 会提示。仓库内的等价入口是 `./bin/mackey`。
- 引擎默认装在 `${XDG_DATA_HOME:-~/.local/share}/mackey/bin/xremap`（`fetch-engine` 下载）；
  `find_engine` 的查找顺序是：配置里的 `engine` → 上述路径 → 仓库 `.vendor/xremap` → `PATH`。
- 新装 GNOME 扩展在 Wayland 下**必须重新登录**才会被 shell 扫描到；
  在此之前 `gnome-extensions info` 会显示「不存在」，`focusd --test` 会失败。
  这是设计上的保护：焦点来源不可用时 `mackey enable` 会拒绝启动引擎，
  以免终端里的 ⌘C 退化成 Ctrl+C（SIGINT）。
- `input` 组是读 `/dev/input` 的唯一前提；加入后同样需要重新登录本会话。
- `/dev/uinput` 需要可写（合成按键）；部分发行版已预置 udev 规则。

## 提交

提交信息使用中文类型前缀（`新增:` / `修复:` / `文档:` / `测试:` / `重构:` / `杂务:` / `初始化:`，
见 [CONTRIBUTING.md](CONTRIBUTING.md)）；提交前跑 `make lint && make check`。
