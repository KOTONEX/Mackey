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
/// 开发、测试和打包共用项目内的临时空间，不能通过软链转移到项目外。
pub fn 临时目录() -> Result<tempfile::TempDir> {
    let 根 = 仓库()?;
    Ok(tempfile::tempdir_in(临时根(&根)?)?)
}
fn 临时根(根: &Path) -> Result<PathBuf> {
    let 目录 = 路径集合::发现()?.保护(&根.join("builddir/临时"))?;
    ensure!(目录.starts_with(根), "临时目录必须位于项目内");
    std::fs::create_dir_all(&目录)?;
    Ok(目录)
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
fn 校验() -> Result<()> {
    let 根 = 仓库()?;
    let 路径 = 路径集合::发现()?;
    let 临时目录 = 临时目录()?;
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
    校验()?;
    let 宿主 = crate::验证::宿主()?;
    运行(&根, "cargo", &["build", "--locked", "--target", &宿主])?;
    运行(
        &根,
        "cargo",
        &["test", "--locked", "--all-targets", "--target", &宿主],
    )?;
    crate::验证::扩展(&根)?;
    println!("✓ Rust、差分与扩展契约测试通过");
    Ok(())
}
pub fn 端到端测试() -> Result<()> {
    let 根 = 仓库()?;
    let 宿主 = crate::验证::宿主()?;
    运行(
        &根,
        "cargo",
        &[
            "build",
            "--locked",
            "--example",
            "虚拟键盘",
            "--target",
            &宿主,
        ],
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
    运行(&根, "cargo", &["check", "--locked", "--all-targets"])?;
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
    测试()?;
    println!("✓ 完整离线检查通过");
    Ok(())
}

#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 临时目录留在项目内且拒绝软链转移() {
        let 临时 = 临时目录().unwrap();
        assert!(
            临时
                .path()
                .starts_with(仓库().unwrap().join("builddir/临时"))
        );
        let 项目 = 临时.path().join("项目");
        let 外部 = 临时.path().join("外部");
        std::fs::create_dir(&项目).unwrap();
        std::fs::create_dir(&外部).unwrap();
        std::os::unix::fs::symlink(&外部, 项目.join("builddir")).unwrap();
        assert!(临时根(&项目).is_err());
        assert!(!外部.join("临时").exists());
    }
}
