# 更新日志

本项目的所有重要变更都记录在此文件。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)；
提交信息使用中文类型前缀（见 [CONTRIBUTING.md](CONTRIBUTING.md)）。

## [未发布]

（暂无）

## [1.2.0] - 2026-10-01

- 明确只支持 GNOME Wayland：`fetch-engine` 固定下载 `gnome` 特性，不再识别桌面/会话
  （`--desktop`、`--session` 参数移除）。
- `install.sh` 对非 GNOME 桌面或 X11 会话警告并中止（exit 1），`mackey install/enable/apply`
  同样拒绝 X11 会话：GNOME 50 已移除 X11 会话，且 GNOME 扩展 API 变动频繁，
  本项目不跟随更旧版本 GNOME。

## [1.1.0] - 2026-10-01

- 终端里复制/粘贴只认 `⌘C`/`⌘V`：新增 `swallow_terminal`，吞掉物理 `Ctrl+Shift+C/V`；
  ⌘ 路径走引擎合成，不受影响。
- 不带 Shift 的 `Ctrl+V` 特意不吞：readline 的 quoted-insert 等仍可用；若你的终端用
  `Ctrl+V` 直接粘贴并希望禁掉，把 `C-v` 加进 `swallow_terminal.triggers` 即可。
- 仅作用于终端配置档，其它应用里这些组合原样放行。

## [1.0.0] - 2026-09-30

- 首个正式版本，提交历史自本版本起为单个初始提交。
- 在 GNOME（Wayland/X11）上把 macOS 的 ⌘/⌥ 组合翻译成当前应用在 Linux 下的等价按键：
  引擎下载、焦点桥、按应用分流、GNOME 键位冲突迁移与泛化兜底一并就位。
- 截图键 `⌘⇧3/4/5` 分别对应 GNOME 的选区（交互式 UI）、屏幕（全屏直接落盘）、
  窗口（截当前窗口），输出均跟随本机 dconf 实测绑定推导。
- 为 `tools/` 与扩展代码补齐类型注解，并以 `types/gjs.d.ts` 固化 GJS 最小类型面。
- 新增 `make typecheck`（mypy 严格检查 `tools/`，tsc 以 `@ts-check` 检查扩展）；
  CI 以 `MACKEY_TYPECHECK_STRICT=1` 强制运行。
