// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "命令帮助.rs"]
mod 命令帮助;
use anyhow::{Result, ensure};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use mackey::{升级, 引擎下载, 服务, 焦点桥, 路径::路径集合, 配置生成};
#[derive(Parser)]
#[command(
    name = "mackey",
    about = "GNOME Wayland 上的 macOS 键位模拟",
    after_help = "用 `mackey 帮助 <子命令>` 查看该命令的参数。"
)]
struct 命令行 {
    #[command(subcommand)]
    子命令: Option<子命令>,
}
#[derive(Subcommand)]
enum 子命令 {
    #[command(about = "查看命令帮助")]
    帮助 {
        #[arg(value_name = "子命令", help = "要查看的命令，省略时显示总览")]
        子命令: Option<String>,
    },
    #[command(hide = true)]
    流水线 { 操作: String },
    #[command(about = "初始化配置并识别键盘布局")]
    初始化,
    #[command(about = "下载 xremap 引擎（gnome 特性）")]
    获取引擎(引擎下载::选项),
    #[command(about = "安装 Rust 二进制、扩展和用户服务（不自动启动）")]
    安装 {
        #[arg(long, help = "使用已有引擎，跳过 xremap 下载")]
        不下载: bool,
        #[arg(long, help = "更新安装文件时保留当前运行的服务")]
        保持运行: bool,
    },
    #[command(about = "检查最新正式发行并安装（校验 SHA-256，保持运行）")]
    升级,
    #[command(about = "启动焦点桥和键盘引擎")]
    启用,
    #[command(about = "停用服务，恢复 Linux 按键")]
    停用,
    #[command(about = "查看状态")]
    状态,
    #[command(about = "环境体检")]
    体检,
    #[command(name = "探测", about = "只读探测冲突")]
    探测结果,
    #[command(about = "生成配置、迁移计划与行为清单")]
    生成(配置生成::选项),
    #[command(about = "备份、迁移冲突键位并重启服务")]
    应用 {
        #[arg(short = 'y', long, help = "跳过 GNOME 键位迁移确认")]
        确认执行: bool,
    },
    #[command(about = "还原 GNOME 键位")]
    还原,
    #[command(about = "焦点上报桥（D-Bus → Unix socket）")]
    焦点桥(焦点桥::选项),
    #[command(about = "输出键盘识别结果")]
    识别键盘,
    #[command(about = "查看服务日志")]
    日志 {
        #[arg(default_value_t = 50, help = "显示最近的日志条数")]
        行数: u32,
    },
    #[command(about = "查看版本")]
    版本,
    #[command(about = "完整离线检查（需要源码仓库）")]
    检查,
    #[command(about = "Rust、隔离安装卸载、映射差分与真实 GJS 契约测试")]
    测试 {
        #[arg(long, help = "加跑虚拟键盘测试（需要设备权限及无全局引擎占用）")]
        端到端: bool,
    },
    #[command(about = "本机架构发行包及许可证")]
    打包,
    #[command(about = "汇总双架构发行包、源码、扩展与校验和")]
    发布附件,
    #[command(about = "根据 Git 历史生成变更日志")]
    变更日志,
    #[command(about = "根据 Git 历史生成当前版本发布说明")]
    发布说明,
    #[command(about = "卸载并还原键位")]
    卸载 {
        #[arg(long, help = "卸载时保留用户配置目录")]
        保留配置: bool,
        #[arg(long, help = "只显示将清理和还原的内容")]
        预演: bool,
        #[arg(short = 'y', long, help = "跳过卸载确认")]
        确认执行: bool,
    },
}
fn 执行() -> Result<()> {
    let mut 命令 = 命令帮助::汉化(命令行::command());
    let 匹配 = match 命令.try_get_matches_from_mut(std::env::args_os()) {
        Ok(匹配) => 匹配,
        Err(错误) => {
            错误.print()?;
            eprintln!("查看命令请运行 `mackey 帮助`，查看版本请运行 `mackey 版本`。");
            std::process::exit(错误.exit_code());
        }
    };
    let cli = 命令行::from_arg_matches(&匹配)?;
    let cmd = match cli.子命令 {
        None => return 命令帮助::显示(&mut 命令, None),
        Some(子命令::帮助 { 子命令 }) => {
            return 命令帮助::显示(&mut 命令, 子命令.as_deref());
        }
        Some(子命令::版本) => {
            println!("mackey {}", mackey::版本);
            return Ok(());
        }
        Some(cmd) => cmd,
    };
    ensure!(
        unsafe { libc::geteuid() } != 0,
        "不要用 root/sudo 运行：本工具不碰系统目录"
    );
    let 路径 = 路径集合::发现()?;
    match cmd {
        子命令::流水线 { 操作 } => mackey::流水线::执行(&操作)?,
        子命令::初始化 => 服务::初始化(&路径)?,
        子命令::获取引擎(o) => {
            引擎下载::执行(&路径, &o)?;
        }
        子命令::安装 {
            不下载, 保持运行
        } => 服务::安装(&路径, 不下载, 保持运行)?,
        子命令::升级 => 升级::执行(&路径)?,
        子命令::启用 => 服务::启用(&路径)?,
        子命令::停用 => 服务::停用()?,
        子命令::状态 => 服务::状态(&路径),
        子命令::体检 => 服务::体检(&路径)?,
        子命令::探测结果 => {
            配置生成::执行(
                &路径,
                &配置生成::选项 {
                    报告: true,
                    ..Default::default()
                },
            )?;
        }
        子命令::生成(o) => {
            配置生成::执行(&路径, &o)?;
        }
        子命令::应用 { 确认执行 } => 服务::应用(&路径, 确认执行)?,
        子命令::还原 => 服务::还原(&路径)?,
        子命令::焦点桥(o) => 焦点桥::执行(&路径, &o)?,
        子命令::识别键盘 => println!(
            "{}",
            serde_json::to_string_pretty(&mackey::键盘识别::当前键盘())?
        ),
        子命令::日志 { 行数 } => {
            let out = 服务::执行命令(
                "journalctl",
                &[
                    "--user",
                    "-u",
                    "mackey-engine",
                    "-u",
                    "mackey-focusd",
                    "-n",
                    &行数.to_string(),
                    "--no-pager",
                ],
            )?;
            print!("{}", String::from_utf8_lossy(&out.stdout));
            ensure!(out.status.success(), "journalctl 失败");
        }
        子命令::帮助 { .. } | 子命令::版本 => unreachable!(),
        子命令::检查 => mackey::开发::检查()?,
        子命令::测试 { 端到端 } => {
            mackey::开发::测试()?;
            if 端到端 {
                mackey::开发::端到端测试()?;
            }
        }
        子命令::打包 => {
            mackey::打包::执行()?;
        }
        子命令::发布附件 => {
            mackey::打包::发布附件()?;
        }
        子命令::变更日志 => {
            mackey::变更日志::生成(false)?;
        }
        子命令::发布说明 => {
            mackey::变更日志::生成(true)?;
        }
        子命令::卸载 {
            保留配置,
            预演,
            确认执行,
        } => 服务::卸载(&路径, !保留配置, 预演, 确认执行)?,
    }
    Ok(())
}
fn main() {
    if let Err(err) = 执行() {
        eprintln!("✗ {err:#}");
        std::process::exit(1);
    }
}
