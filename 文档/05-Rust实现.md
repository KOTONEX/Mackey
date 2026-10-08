# Rust 实现与部署

主程序、生成器、焦点桥、键盘识别和引擎下载器由 `mackey` Rust 二进制实现，运行无需
Python、PyGObject 或 `busctl`。GNOME Shell 扩展保留 JavaScript，因为它在 Shell 的 GJS 环境运行。
源码树的 `命令/mackey`、安装与卸载脚本只负责启动和安装编排；自有命令与参数采用中文。

## 构建与运行

```bash
cargo build --locked --bin mackey          # target/debug/mackey
cargo build --locked --release --bin mackey # target/release/mackey
./target/release/mackey --help
cargo run --quiet -- 检查
dbus-run-session -- cargo run --quiet -- 扩展测试
cargo run --quiet -- 打包         # builddir/发行/ 中的二进制包与依赖许可文本
```

只固定 Rust Edition 2024，不固定 Rust 版本号或工具链；构建需要支持该 Edition 的 Rust、Cargo 与 C 编译器。应用提交 `Cargo.lock` 并使用 `--locked`。
发布工作流分别在 Ubuntu 24.04 x86_64/aarch64 runner 上原生构建；产物使用系统 glibc，
不声称是静态链接或兼容所有旧版发行版。

源码入口优先使用已存在的 debug 二进制，其次 release；两者不存在时用 Cargo 构建。
修改源码后运行 `cargo build --locked`，或直接 `cargo run --locked --bin mackey -- <子命令>`。
部署 release 时可直接执行 `target/release/mackey 安装`。

## 配置与部署

- 先使用旧版卸载脚本还原键位并清除配置，再安装新版；不自动迁移旧接口。
- 配置使用 `config.json`、`xremap.json`、`relocations.json` 和 GNOME 备份文件。
- GNOME 扩展、两份 systemd 用户服务与 xremap Unix socket JSON 行协议共同提供焦点与按键映射。
- 提供安装、卸载、启用、停用、生成、探测、应用、还原和日志子命令。
- 安装后命令和服务都使用 XDG 数据目录中的二进制，移动源码目录不影响运行。
- 版本、默认行为清单和扩展文件编译时嵌入；修改这些资源后需要重新编译和安装。
- 默认生成的行为文档写到 XDG 数据目录；`cargo run --quiet -- 生成 --文档 "$PWD/文档/03-行为清单.md"` 显式写仓库文档。
- `探测` 及 `生成 --报告` 只输出报告。普通探测失败会拒绝生成；离线使用 `--不探测`。

GNOME 键位迁移先保存原始值再写 GSettings；还原失败时保留备份，默认卸载 不删除配置。
焦点来源和 socket 未就绪时拒绝启动引擎；启用不执行任何全局 Ctrl/Super 交换。

## 路径边界

持久数据只写 HOME 下，`源码/库/路径.rs::保护主目录路径` 逐段解析软链接与 `..`，
写入采用同目录临时文件、权限设置、同步与原子替换。所有测试临时目录也位于 HOME。
协议约定的固定 `/run/user/$UID/mackey-focus.sock` 是唯一运行时 IPC 例外（0600），
不创建系统目录；自定义 socket 必须在 HOME 下。只移除不再监听的旧 socket。

虚拟键盘 example 仅在端到端测试中访问 `/dev/uinput`。它使用独立设备名，xremap 只抓该设备；
未被映射的透传按键仍可能到达当前窗口，所以该测试需要 input/uinput 权限及合适焦点窗口。
离线测试与 D-Bus 契约测试不读写真实键盘，不修改当前 GNOME 设置。
运行虚拟键盘测试前，应先停用会抓取全部键盘的现有重映射引擎，避免它同时抢占测试设备。

## 回归验证

旧 Python 生成器的离线输出保存为 fixture：89 条显式映射和 26 条泛化映射，Rust 测试比较
整个 JSON 值。另覆盖 GNOME 冲突迁移、保留原有备用组合、截图自定义绑定、PC 物理位置交换、
设备过滤、终端/IDE 的 Ctrl+C 例外、非法键名与吞键保护。

独立二进制集成测试覆盖 socket 实际请求、权限、重复监听保护、普通文件保护、SIGTERM 清理、
离线生成、只读报告和下载计划。安装、卸载测试通过隔离 HOME 与桩命令验证幂等、清理、备份还原。
扩展契约在隔离 D-Bus 会话验证真实扩展源码，并检验 Rust 客户端到扩展的互通。

参考：[clap](https://docs.rs/clap/latest/clap/)、[reqwest](https://docs.rs/reqwest/latest/reqwest/blocking/struct.ClientBuilder.html)、
[zip](https://docs.rs/zip/latest/zip/read/struct.ZipArchive.html)、[Linux uinput](https://www.kernel.org/doc/html/latest/input/uinput.html)。

安装和卸载共用原生命令：`./安装.sh` 等价于 `mackey 安装`，`./卸载.sh` 等价于 `mackey 卸载`。
卸载默认清理配置，可传 `--保留配置`；安装统一补齐缺失字段并备份损坏配置。
X11 不受支持，但程序不作会话类型检测或拦截。

隔离安装/卸载测试和命令桩、扩展测试夹具、虚拟键盘进程编排及发行包验证均由 Rust 执行。
发布附件通过 `cargo run --quiet --locked -- 发布附件` 生成；需先放齐两个架构的发行包。
源码快照仍调用标准 `git archive`，gzip、tar、扩展 ZIP 与 SHA-256 清单使用 Rust 库。
Bash 只保留三个启动壳，GNOME 扩展与真实 GJS 契约保留 JavaScript。
