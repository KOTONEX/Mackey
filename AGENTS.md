# Mackey 代理入口

先读 [项目规范](文档/项目规范.md)，按其中的业务、写入、工具链和交付边界执行。
使用说明见 [README](README.md)，开发操作见 [贡献指南](CONTRIBUTING.md)。

## 稳定约束

- 行为唯一事实源为 `配置/行为清单.json`，保留终端/IDE Ctrl+C；不全局交换 Ctrl/Super。
- 所有持久写入与测试临时目录在 HOME，经过 `源码/库/路径.rs`；拒绝 root/sudo。唯一 IPC 例外 `/run/user/$UID/mackey-focus.sock`（0600）；设备测试仅显式使用 `/dev/uinput`。
- 自有接口与文档采用中文，保留外部规定的标识。旧接口拒绝并提示卸载重装；AGPL-3.0-or-later 保持。
- 工具链跟随最新稳定版，不锁数字；Edition 随稳定版迁移验收更新，Cargo.lock 保留。原生 ELF tsc 编译 TS，项目与 CI 不引入 Node/npm。
- 只有属于远程 main 历史的标签提交可正式发布，其余预发行且非 Latest；不覆盖公开附件或移动公开标签。
- 当前已授权正常提交并直接推送 main、自主标签；不再保留 rust 分支。审查不得自动安装或打断当前键盘服务。
- 支持 GNOME Wayland；X11 不支持仅写文档，不增加检测或拦截。

## 事实位置

| 内容 | 位置 |
| --- | --- |
| 版本、依赖与 Edition | Cargo.toml、Cargo.lock |
| CLI、业务与路径边界 | 源码/程序/主程序.rs、源码/库/ |
| 行为与生成文档 | 配置/行为清单.json、文档/03-行为清单.md |
| TypeScript 及嵌入构建 | 扩展/、测试/、类型声明/、源码/构建.rs |
| 独立安装、还原与运行契约 | 测试/安装卸载.rs、测试/命令行.rs、测试/扩展契约.ts |
| CI、发行与编译器准备 | .github/workflows/、.github/actions/原生编译器/action.yml |
| 许可全文与登记 | LICENSE、文档/04-第三方许可证.md、文档/06-Rust依赖许可证.tsv |

## 验证命令

- `cargo 检查`：格式、静态/类型检查、ShellCheck、JSON、115 差分、全部离线 Rust 与真实 GJS/D-Bus 契约。
- `cargo 打包`：宿主包、原始许可证、发行信息与独立解压执行验证。
- `cargo 测试 --端到端`：按需设备测试；没有授权停全局引擎时保留现有服务，报告未运行。
- 改工作流运行 actionlint，改文档核验本地链接；交付从干净副本构建。实际准备版本、测试、打包、推送和公开发布分别报告。
