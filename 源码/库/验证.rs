// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::{原子写入, 路径集合};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};
pub fn 构建目录(根: &Path) -> Result<PathBuf> {
    let 输出 = Command::new("cargo")
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .current_dir(根)
        .output()?;
    ensure!(输出.status.success(), "读取 Cargo 构建目录失败");
    let 数据: serde_json::Value = serde_json::from_slice(&输出.stdout)?;
    Ok(PathBuf::from(
        数据["target_directory"].as_str().context("缺少构建目录")?,
    ))
}
pub fn 扩展(根: &Path) -> Result<()> {
    let 路径 = 路径集合::发现()?;
    let 临时 = tempfile::Builder::new()
        .prefix(".mackey-扩展-")
        .tempdir_in(&路径.主目录)?;
    let 原文 = fs::read_to_string(根.join("扩展").join(crate::扩展标识).join("extension.js"))?;
    let 导入 = "resource:///org/gnome/shell/extensions/extension.js";
    ensure!(
        原文.matches(导入).count() == 1,
        "扩展中必须恰好有一处 Shell 基类导入"
    );
    let 改写 = 原文.replacen(导入, "./shell-stub.js", 1);
    let 差异: Vec<_> = 原文
        .lines()
        .zip(改写.lines())
        .filter(|(前, 后)| 前 != 后)
        .collect();
    ensure!(
        原文.lines().count() == 改写.lines().count() && 差异.len() == 1 && 差异[0].0.contains(导入),
        "扩展夹具只能替换基类导入"
    );
    let 文件 = 临时.path().join("extension-undertest.js");
    原子写入(&路径, &文件, 改写.as_bytes(), 0o644)?;
    原子写入(
        &路径,
        &临时.path().join("shell-stub.js"),
        b"export class Extension { constructor(metadata) { this.metadata = metadata; } }\n",
        0o644,
    )?;
    let 程序 = 构建目录(根)?.join("debug/mackey");
    let 状态 = Command::new("dbus-run-session")
        .args(["--", "gjs", "-m"])
        .arg(根.join("测试/扩展契约.mjs"))
        .arg(&文件)
        .arg(程序)
        .env("GIO_USE_VFS", "local")
        .status()?;
    ensure!(状态.success(), "真实扩展 D-Bus 契约失败");
    Ok(())
}
fn 解包(包: &Path, 目标: &Path) -> Result<()> {
    let mut 归档 = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(包)?));
    for 项 in 归档.entries()? {
        let mut 项 = 项?;
        let 名称 = 项.path()?;
        ensure!(
            名称
                .components()
                .all(|项| matches!(项, Component::Normal(_))),
            "归档包含不安全路径"
        );
        ensure!(
            项.header().entry_type().is_file() || 项.header().entry_type().is_dir(),
            "发行包包含非普通文件或目录：{}（类型 {:?}）",
            名称.display(),
            项.header().entry_type()
        );
        ensure!(项.unpack_in(目标)?, "归档不能写入解压目录之外");
    }
    Ok(())
}
pub fn 检查归档架构(包: &Path, 架构: &str) -> Result<()> {
    let mut 归档 = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(包)?));
    let 目标 = PathBuf::from(format!("mackey-{}-linux-{架构}/mackey", crate::版本));
    for 项 in 归档.entries()? {
        let mut 项 = 项?;
        if 项.path()? == 目标 {
            let mut 头 = [0; 20];
            项.read_exact(&mut 头)?;
            let 机器 = if 架构 == "x86_64" { 62 } else { 183 };
            ensure!(
                &头[..4] == b"\x7fELF"
                    && 头[4] == 2
                    && 头[5] == 1
                    && u16::from_le_bytes([头[18], 头[19]]) == 机器,
                "发行包 ELF 架构与名称不符"
            );
            return Ok(());
        }
    }
    anyhow::bail!("归档中缺少目标二进制：{}", 目标.display())
}
pub fn 发行包(根: &Path, 包: &Path) -> Result<()> {
    let 路径 = 路径集合::发现()?;
    let 临时 = tempfile::Builder::new()
        .prefix(".mackey-发行验证-")
        .tempdir_in(&路径.主目录)?;
    let 解压 = 临时.path().join("解压");
    fs::create_dir(&解压)?;
    解包(包, &解压)?;
    let 架构 = crate::引擎下载::识别架构(std::env::consts::ARCH)?;
    检查归档架构(包, 架构)?;
    let 目录 = 解压.join(format!("mackey-{}-linux-{架构}", crate::版本));
    for 文件 in [
        "LICENSE",
        "LICENSES/依赖许可证.tsv",
        "CHANGELOG.md",
        "发布说明.md",
    ] {
        ensure!(
            fs::metadata(目录.join(文件))?.len() > 0,
            "缺少非空发行文件：{文件}"
        );
    }
    let 程序 = 目录.join("mackey");
    let 版本 = Command::new(&程序).arg("--version").output()?;
    ensure!(
        版本.status.success()
            && String::from_utf8(版本.stdout)?.trim() == format!("mackey {}", crate::版本),
        "发行包版本不符"
    );
    let 主目录 = 临时.path().join("home");
    fs::create_dir(&主目录)?;
    let mut 命令 = Command::new(程序);
    命令.env("HOME", &主目录);
    for (环境, 目录) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_STATE_HOME", ".local/state"),
        ("XDG_BIN_HOME", ".local/bin"),
        ("XDG_RUNTIME_DIR", "run"),
    ] {
        命令.env(环境, 主目录.join(目录));
    }
    let 状态 = 命令
        .args(["生成", "--不探测", "--用户配置"])
        .arg(根.join("测试/基准/用户配置.json"))
        .arg("--输出目录")
        .arg(主目录.join("配置"))
        .arg("--文档")
        .arg(主目录.join("行为清单.md"))
        .status()?;
    ensure!(状态.success(), "发行包独立生成失败");
    let 基准 = crate::路径::读取结构数据(&根.join("测试/基准/xremap.json"))?;
    ensure!(
        crate::路径::读取结构数据(&主目录.join("配置/xremap.json"))? == 基准,
        "发行包嵌入清单与旧版映射不一致"
    );
    ensure!(
        fs::metadata(主目录.join("行为清单.md"))?.len() > 0,
        "发行包缺少生成文档"
    );
    println!("✓ Rust 发行包解压验证通过");
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 拒绝带软链的归档() {
        let 临时 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let 包 = 临时.path().join("非法.tar.gz");
        let mut 写入器 = tar::Builder::new(flate2::write::GzEncoder::new(
            fs::File::create(&包).unwrap(),
            flate2::Compression::default(),
        ));
        let mut 头 = tar::Header::new_gnu();
        头.set_entry_type(tar::EntryType::Symlink);
        头.set_size(0);
        头.set_mode(0o777);
        头.set_link_name("/etc").unwrap();
        头.set_cksum();
        写入器
            .append_data(&mut 头, "逃逸", std::io::empty())
            .unwrap();
        写入器.into_inner().unwrap().finish().unwrap();
        assert!(解包(&包, 临时.path()).is_err());
        assert!(!临时.path().join("逃逸").exists());
    }
}
