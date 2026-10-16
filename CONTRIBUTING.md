# 贡献指南

完整约束集中在 [项目规范](文档/项目规范.md)，本文提供开发操作。
使用与配置从 [文档索引](文档/README.md) 进入，归档和发布细节见 [构建与发行](文档/05-构建与发行.md)。

## 开发环境

准备最新稳定工具链，核对真实版本；普通构建不自动升级全局环境。

| 工具 | 用途 |
| --- | --- |
| Jujutsu（`jj`）与 Git | 日常提交、GitHub 互操作、发行日志与源码归档 |
| Rust/Cargo、rustfmt、Clippy、C 编译器 | 原生业务、静态检查及虚拟键盘程序 |
| 原生 ELF TypeScript 编译器 `tsc` | 扩展、契约和基类桩的严格编译 |
| `gjs`、`dbus-run-session` | 真实扩展的隔离 D-Bus 契约 |
| `gsettings`、systemd 用户服务 | GNOME 探测与运行集成；安装卸载离线测试使用命令桩 |
| `gh`、归档工具、rustup | CI 工具准备和发布；普通生成与独立运行不需要 |
| actionlint | 修改工作流时检查 YAML 和平台接口 |
| xremap、可用的 input/uinput 权限 | 可选的真实设备端到端测试 |

将原生 `tsc` 所在目录加入 PATH；构建拒绝脚本启动器，无需 Node/npm。
CI 的原生获取方式及嵌入资源见 [构建与发行](文档/05-构建与发行.md#typescript-与版本资源)。
本地检查以普通用户运行，临时目录放在 HOME，不安装 udev 规则或调用 sudo。
临时 CI runner 的系统开发依赖安装例外见项目规范。

## 版本控制

日常使用 Jujutsu 的共置 Git 后端，普通贡献者和 CI 仍可只用 Git 构建。
获取新副本：

```bash
jj git clone --colocate https://github.com/KOTONEX/Mackey.git Mackey
cd Mackey
```

已有 Git 副本只需初始化一次：

```bash
jj git init --colocate
jj bookmark track '"主线"@origin'
```

用 `jj config get user.name` 和 `jj config get user.email` 检查身份；缺失时通过
`jj config set --repo user.name "你的名字"` 与对应 `user.email` 设置仓库级值。
不要为了本项目修改全局身份。

```bash
jj status
jj diff
jj git fetch --remote origin
# 编辑、检查；无关改动先隔离
cargo 检查
jj commit -m "文档: 说明本次修改"
jj show -r @-
jj bookmark set 主线 -r @-
jj git push --remote origin --bookmark 主线
```

Jujutsu 自动快照，没有 `git add`。`jj commit` 后 `@` 为新的空工作副本，完成提交在 `@-`。
核对内容后才移动书签；远程有并发更新时先处理差异，不强推公开历史。
交付时工作副本无差异，`主线`、远程书签与 Git `HEAD` 对应同一完成提交。

## 检查与测试

| 命令 | 覆盖范围 |
| --- | --- |
| `cargo 检查` | 格式、Clippy、Rust/TS 类型、JSON、115 映射差分、全部离线 Rust 与真实 GJS/D-Bus 契约 |
| `cargo 测试` | 差分、Rust 单元与隔离安装卸载、真实 GJS 契约 |
| `cargo 格式化` | Rust 格式化 |
| `cargo 运行 <子命令>` | 构建并执行当前源码，参数直接跟子命令，无需额外 `--` |
| `cargo 打包` | 宿主发行包、原始许可及解压后独立执行校验 |

别名见 [.cargo/config.toml](.cargo/config.toml)，在仓库及其子目录可用。
`tsc -p tsconfig.json --noEmit` 可单独检查类型；普通 `tsc -p tsconfig.json` 输出到被忽略的
`builddir/类型脚本/`，Cargo 则在自己的 HOME 内 `OUT_DIR` 编译嵌入资源，不维护手写 JS 副本。

服务恢复回归需要正在运行的用户 systemd，单独执行：

```bash
cargo test --locked --test 服务恢复 -- --ignored --nocapture
```

该测试使用独立服务名和 HOME 套接字，模拟来源延迟及桥接故障；不停止现有 Mackey 服务，
不访问键盘，不启用测试服务自启。普通离线检查明确将其标为忽略。

### 真实设备测试

`cargo 测试 --端到端` 额外创建 uinput 虚拟键盘并调用 xremap，只抓取本次测试设备。
未映射的透传键仍可能进入当前焦点窗口；先选择可接受透传的窗口，检查设备权限，
并在得到授权后停止已有全局引擎。测试不会替你停用户服务。

可用 `cargo 运行 获取引擎` 获取引擎，默认请求 `gnome`、允许回退 `full`；
也可自行构建或提供二进制；自定义引擎路径见 [配置说明](文档/07-配置说明.md)。
测试等待输入节点可读、引擎持有输入 FD 且输出设备出现，再放行注入；
独立进程组和 RAII 只回收本次进程与设备。没有运行时按实际原因记录，不称为通过。

真实 GJS 契约只替换 Shell 基类导入，加载其余原始编译产物；不以纯 Rust 模拟代替扩展集成。

## 修改行为与生成文档

只在 [配置/行为清单.json](配置/行为清单.json) 修改映射、归属、迁移及例外。
然后在仓库根目录执行：

```bash
cargo 运行 生成 --不探测 --用户配置 测试/基准/用户配置.json --输出目录 "$HOME/.cache/mackey-文档生成" --文档 "$PWD/文档/03-行为清单.md"
```

配置写入 HOME 内临时目录，文档写入仓库，不覆盖运行配置。
`cargo 运行` 会先构建并嵌入当前清单；部署需另行安装新二进制，不从文档修改推断安装授权。
**不要手工编辑 `文档/03-行为清单.md`。** 若行为确实改变，核对并有意更新差分基准，
不能为了通过检查直接覆盖预期结果；来源及显示名边界见 [基准说明](测试/基准/README.md)。

显式条目声明 `归属`：`苹果风格` 为本工具转换，`系统默认` 为放行、原生等价、可选或无法模拟。
生成器校验归属与策略、目标组合的一致性。中文字段与外部协议边界见 [配置说明](文档/07-配置说明.md)。

## 文档、提交与 PR

README 保留快速入口；当前指南位于 `文档/`，逐版本报告与实测数据位于 `文档/历史/`。
移动文件同步相对链接与锚点，历史数据保留环境和时间边界；不要把历史验证写成当前验收。
规则只在项目规范维护，其他入口引用，避免重复产生冲突。

提交说明采用简体中文前缀，如 `新增:`、`修复:`、`文档:`、`测试:`、`重构:`、`杂务:`。
提交前完成 `cargo 检查`；按修改补充文档链接、actionlint、干净副本打包或设备验收。
PR 使用 [仓库模板](.github/pull_request_template.md)，说明行为、检查结果及未覆盖边界。
应用版本、日志、主线正式/其他预发行和公开产物不可覆盖的规则见 [构建与发行](文档/05-构建与发行.md#版本与公开边界)。

## 许可证

贡献按 AGPL-3.0-or-later 发布，保留第三方原有许可及署名。
引入代码、资产或依赖前核对兼容性，并按 [第三方许可证](文档/04-第三方许可证.md) 登记；
包中原始许可文本与仓库登记的边界见构建指南。
