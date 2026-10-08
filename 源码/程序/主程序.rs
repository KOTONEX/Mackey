// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use mackey::{引擎下载, 服务, 焦点桥, 路径::路径集合, 配置生成};
#[derive(Parser)]
#[command(name="mackey",about="GNOME Wayland 上的 macOS 键位模拟",version=mackey::版本.trim())]
struct 命令行 {
    #[command(subcommand)]
    子命令: Option<子命令>,
}
#[derive(Subcommand)]
enum 子命令 {
    /// 初始化配置并识别键盘布局
    初始化,
    /// 下载 xremap 引擎（gnome 特性）
    获取引擎(引擎下载::选项),
    /// 安装 Rust 二进制、扩展和用户服务（不自动启动）
    安装 {
        #[arg(long)]
        不下载: bool,
        #[arg(long)]
        保持运行: bool,
    },
    /// 启动焦点桥和键盘引擎
    启用,
    /// 停用服务，恢复 Linux 按键
    停用,
    /// 查看状态
    状态,
    /// 环境体检
    体检,
    /// 只读探测冲突
    #[command(name = "探测")]
    探测结果,
    /// 生成配置、迁移计划与行为清单
    生成(配置生成::选项),
    /// 备份、迁移冲突键位并重启服务
    应用 {
        #[arg(short = 'y', long)]
        确认执行: bool,
    },
    /// 还原 GNOME 键位
    还原,
    /// 焦点上报桥（D-Bus → Unix socket）
    焦点桥(焦点桥::选项),
    /// 输出键盘识别结果
    识别键盘,
    /// 查看服务日志
    日志 {
        #[arg(default_value_t = 50)]
        行数: u32,
    },
    /// 查看版本
    版本,
    /// 完整离线检查（需要源码仓库）
    检查,
    /// 格式化 Rust 源码
    格式化,
    /// Rust 与扩展严格类型检查
    类型检查,
    /// 离线生成与旧版行为差分校验
    校验,
    /// Rust 和隔离安装、卸载测试
    测试,
    /// 隔离 GNOME 扩展与 Rust 的 D-Bus 契约测试
    扩展测试,
    /// 虚拟键盘端到端测试（需要设备权限）
    端到端测试,
    /// 包含虚拟键盘的全部测试
    全部测试,
    /// 本机架构发行包及许可证
    打包,
    /// 汇总双架构发行包、源码、扩展与校验和
    发布附件,
    /// 根据 Git 历史生成变更日志
    变更日志,
    /// 根据 Git 历史生成当前版本发布说明
    发布说明,
    /// 卸载并还原键位
    卸载 {
        #[arg(long)]
        保留配置: bool,
        #[arg(long)]
        预演: bool,
        #[arg(short = 'y', long)]
        确认执行: bool,
    },
}
fn 执行() -> Result<()> {
    let cli = 命令行::parse();
    let Some(cmd) = cli.子命令 else {
        use clap::CommandFactory;
        命令行::command().print_help()?;
        println!();
        return Ok(());
    };
    ensure!(
        unsafe { libc::geteuid() } != 0,
        "不要用 root/sudo 运行：本工具不碰系统目录"
    );
    let 路径 = 路径集合::发现()?;
    match cmd {
        子命令::初始化 => 服务::初始化(&路径)?,
        子命令::获取引擎(o) => {
            引擎下载::执行(&路径, &o)?;
        }
        子命令::安装 {
            不下载, 保持运行
        } => 服务::安装(&路径, 不下载, 保持运行)?,
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
        子命令::版本 => {
            println!("mackey {}", mackey::版本.trim());
            if let Some(引擎下载) = 服务::查找引擎(&路径) {
                println!("引擎：{}", 引擎下载.display());
            }
        }
        子命令::检查 => mackey::开发::检查()?,
        子命令::格式化 => mackey::开发::格式化()?,
        子命令::类型检查 => mackey::开发::类型检查()?,
        子命令::校验 => mackey::开发::校验()?,
        子命令::测试 => mackey::开发::测试()?,
        子命令::扩展测试 => mackey::开发::扩展测试()?,
        子命令::端到端测试 => mackey::开发::端到端测试()?,
        子命令::全部测试 => {
            mackey::开发::测试()?;
            mackey::开发::扩展测试()?;
            mackey::开发::端到端测试()?;
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
