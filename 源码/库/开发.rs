// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{路径::路径集合, 配置生成};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn 仓库() -> Result<PathBuf> {
    let 根 = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ensure!(
        根.join("Cargo.toml").is_file() && 根.join("源码/库/入口.rs").is_file(),
        "该开发命令需要 Mackey 源码仓库"
    );
    let 路径 = 路径集合::发现()?;
    路径.保护(&根)
}
pub fn 运行(根: &Path, 程序: &str, 参数: &[&str]) -> Result<()> {
    eprintln!("→ {程序} {}", 参数.join(" "));
    let 状态 = Command::new(程序)
        .args(参数)
        .current_dir(根)
        .status()
        .with_context(|| format!("运行 {程序}；请确认已安装开发依赖"))?;
    ensure!(状态.success(), "{程序} 检查失败：{状态}");
    Ok(())
}
pub fn 格式化() -> Result<()> {
    运行(&仓库()?, "cargo", &["fmt", "--all"])
}
pub fn 类型检查() -> Result<()> {
    let 根 = 仓库()?;
    运行(&根, "cargo", &["check", "--locked", "--all-targets"])?;
    运行(&根, "tsc", &["-p", "tsconfig.json", "--noEmit"])
}
pub fn 校验() -> Result<()> {
    let 根 = 仓库()?;
    let 路径 = 路径集合::发现()?;
    let 临时目录 = tempfile::Builder::new()
        .prefix(".mackey-校验-")
        .tempdir_in(&路径.主目录)?;
    let 结果 = 配置生成::执行(
        &路径,
        &配置生成::选项 {
            用户配置: Some(根.join("测试/基准/用户配置.json")),
            输出目录: Some(临时目录.path().join("配置")),
            文档: Some(临时目录.path().join("行为清单.md")),
            不探测: true,
            ..Default::default()
        },
    )?;
    let 基准 = crate::路径::读取结构数据(&根.join("测试/基准/xremap.json"))?;
    ensure!(结果.配置 == 基准, "生成配置与旧版差分基准不一致");
    ensure!(
        临时目录.path().join("配置/relocations.json").is_file(),
        "缺少迁移计划"
    );
    Ok(())
}
pub fn 测试() -> Result<()> {
    let 根 = 仓库()?;
    运行(&根, "cargo", &["build", "--locked"])?;
    运行(&根, "cargo", &["test", "--locked", "--all-targets"])?;
    Ok(())
}
pub fn 扩展测试() -> Result<()> {
    let 根 = 仓库()?;
    运行(&根, "cargo", &["build", "--locked"])?;
    crate::验证::扩展(&根)
}
pub fn 端到端测试() -> Result<()> {
    let 根 = 仓库()?;
    运行(
        &根,
        "cargo",
        &["build", "--locked", "--example", "虚拟键盘"],
    )?;
    let 路径 = 路径集合::发现()?;
    let 引擎 = crate::服务::查找引擎(&路径).context("没有 xremap 引擎，请先获取引擎")?;
    crate::端到端::执行(&根, &引擎)
}

pub fn 检查() -> Result<()> {
    let 根 = 仓库()?;
    运行(&根, "cargo", &["fmt", "--all", "--", "--check"])?;
    运行(
        &根,
        "cargo",
        &[
            "clippy",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    类型检查()?;
    let mut 脚本 = vec![
        "命令/mackey".to_owned(),
        "安装.sh".to_owned(),
        "卸载.sh".to_owned(),
    ];
    脚本.sort();
    for 文件 in &脚本 {
        运行(&根, "bash", &["-n", 文件])?;
    }
    let mut 参数 = vec!["-S", "warning"];
    参数.extend(脚本.iter().map(String::as_str));
    运行(&根, "shellcheck", &参数)?;
    for 文件 in [
        "配置/行为清单.json",
        "扩展/mackey-focus@kotonex/metadata.json",
        ".vscode/extensions.json",
        ".vscode/settings.json",
    ] {
        let 内容 = std::fs::read(根.join(文件))?;
        serde_json::from_slice::<serde_json::Value>(&内容)
            .with_context(|| format!("解析 {文件}"))?;
    }
    校验()?;
    测试()?;
    扩展测试()?;
    println!("✓ 完整离线检查通过");
    Ok(())
}
