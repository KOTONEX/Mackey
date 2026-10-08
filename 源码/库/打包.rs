// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    开发::{仓库, 运行},
    路径::{原子写入, 路径集合},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn 复制许可(源: &Path, 目标: &Path, 深度: usize) -> Result<usize> {
    let mut 数量 = 0;
    for 条目 in fs::read_dir(源)? {
        let 条目 = 条目?;
        let 类型 = 条目.file_type()?;
        let 名称 = 条目.file_name();
        if 类型.is_dir() && 深度 > 0 {
            数量 += 复制许可(&条目.path(), &目标.join(&名称), 深度 - 1)?;
        } else if 类型.is_file() {
            let 小写 = 名称.to_string_lossy().to_lowercase();
            if ["license", "copying", "notice"]
                .iter()
                .any(|片段| 小写.contains(片段))
            {
                fs::create_dir_all(目标)?;
                fs::copy(条目.path(), 目标.join(&名称))?;
                数量 += 1;
            }
        }
    }
    Ok(数量)
}
pub fn 执行() -> Result<std::path::PathBuf> {
    let 根 = 仓库()?;
    let 路径 = 路径集合::发现()?;
    let 日志 = crate::变更日志::生成(false)?;
    let 发布说明 = crate::变更日志::生成(true)?;
    let 临时 = tempfile::Builder::new()
        .prefix(".mackey-打包-")
        .tempdir_in(&路径.主目录)?;
    let 架构 = crate::引擎下载::识别架构(std::env::consts::ARCH)?;
    let 名称 = format!("mackey-{}-linux-{架构}", crate::版本);
    let 包目录 = 临时.path().join(&名称);
    let 许可目录 = 包目录.join("LICENSES");
    fs::create_dir_all(&许可目录)?;
    for 文件 in ["LICENSE", "README.md"] {
        fs::copy(根.join(文件), 包目录.join(文件))?;
    }
    fs::copy(日志, 包目录.join("CHANGELOG.md"))?;
    fs::copy(发布说明, 包目录.join("发布说明.md"))?;
    let 信息 = Command::new("rustc").arg("-vV").output()?;
    ensure!(信息.status.success(), "读取 rustc 宿主架构失败");
    let 信息 = String::from_utf8(信息.stdout)?;
    let 宿主 = 信息
        .lines()
        .find_map(|行| 行.strip_prefix("host: "))
        .context("rustc 没有 host 信息")?;
    运行(
        &根,
        "cargo",
        &["build", "--locked", "--release", "--target", 宿主],
    )?;
    let 元信息 = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            宿主,
        ])
        .current_dir(&根)
        .output()?;
    ensure!(元信息.status.success(), "cargo metadata 失败");
    let 元信息: Value = serde_json::from_slice(&元信息.stdout)?;
    let 构建目录 = Path::new(
        元信息["target_directory"]
            .as_str()
            .context("缺少构建目录")?,
    );
    fs::copy(
        构建目录.join(宿主).join("release/mackey"),
        包目录.join("mackey"),
    )?;
    let 节点: BTreeSet<_> = 元信息["resolve"]["nodes"]
        .as_array()
        .context("缺少依赖图")?
        .iter()
        .filter_map(|节点| 节点["id"].as_str())
        .collect();
    let mut 登记 = vec!["依赖\t版本\t许可证".to_owned()];
    for 包 in 元信息["packages"].as_array().context("缺少依赖清单")? {
        let 名 = 包["name"].as_str().context("依赖缺少名称")?;
        if 名 == "mackey" || !节点.contains(包["id"].as_str().unwrap_or_default()) {
            continue;
        }
        let 版本 = 包["version"].as_str().context("依赖缺少版本")?;
        let 许可 = 包["license"]
            .as_str()
            .or_else(|| 包["license_file"].as_str())
            .context("依赖缺少许可信息")?;
        登记.push(format!("{名}\t{版本}\t{许可}"));
        let 清单 = Path::new(包["manifest_path"].as_str().context("依赖缺少清单路径")?);
        let 源 = 清单.parent().context("清单没有父目录")?;
        let 目标 = 许可目录.join(format!("{名}-{版本}"));
        let mut 数量 = 复制许可(源, &目标, 1)?;
        if let Some(许可文件) = 包["license_file"].as_str() {
            let 许可源 = 源.join(许可文件);
            fs::create_dir_all(&目标)?;
            fs::copy(
                &许可源,
                目标.join(许可源.file_name().context("许可文件名缺失")?),
            )?;
            数量 += 1;
        }
        ensure!(数量 > 0, "{名}-{版本} 缺少可分发的许可证原文");
    }
    登记[1..].sort();
    原子写入(
        &路径,
        &许可目录.join("依赖许可证.tsv"),
        (登记.join("\n") + "\n").as_bytes(),
        0o644,
    )?;
    let 临时包 = 临时.path().join("发行包.tar.gz");
    let 状态 = Command::new("tar")
        .args(["-czf"])
        .arg(&临时包)
        .arg("-C")
        .arg(临时.path())
        .arg(&名称)
        .status()?;
    ensure!(状态.success(), "tar 打包失败");
    let 产物 = 根.join("builddir/发行").join(format!("{名称}.tar.gz"));
    原子写入(&路径, &产物, &fs::read(临时包)?, 0o644)?;
    运行(
        &根,
        "bash",
        &[
            "测试/发行包.sh",
            产物.to_str().context("发行包路径不是 UTF-8")?,
        ],
    )?;
    println!("✓ {}", 产物.display());
    Ok(产物)
}
