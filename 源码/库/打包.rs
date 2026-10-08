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
        &[
            "build",
            "--locked",
            "--release",
            "--bin",
            "mackey",
            "--target",
            宿主,
        ],
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
    let 编码器 = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut 归档 = tar::Builder::new(编码器);
    归档.follow_symlinks(false);
    // 发行包使用普通文件条目，避免把稀疏 ELF 编码为 GNU 特殊条目。
    归档.sparse(false);
    归档.append_dir_all(&名称, &包目录)?;
    let 字节 = 归档.into_inner()?.finish()?;
    let 产物 = 根.join("builddir/发行").join(format!("{名称}.tar.gz"));
    原子写入(&路径, &产物, &字节, 0o644)?;
    crate::验证::发行包(&根, &产物)?;
    println!("✓ {}", 产物.display());
    Ok(产物)
}

pub fn 发布附件() -> Result<()> {
    组装附件(&仓库()?)
}
fn 组装附件(根: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let 路径 = 路径集合::发现()?;
    let 目录 = 路径.保护(&根.join("builddir/发行"))?;
    let mut 附件 = Vec::new();
    for 架构 in ["x86_64", "aarch64"] {
        let 包 = 目录.join(format!("mackey-{}-linux-{架构}.tar.gz", crate::版本));
        ensure!(包.is_file(), "缺少 {}", 包.display());
        crate::验证::检查归档架构(&包, 架构)?;
        附件.push(包);
    }
    let 来源 = Command::new("git")
        .args([
            "archive",
            "--format=tar",
            &format!("--prefix=mackey-{}/", crate::版本),
            "HEAD",
        ])
        .current_dir(根)
        .output()?;
    ensure!(来源.status.success(), "Git 源码归档失败");
    let mut 编码器 = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    编码器.write_all(&来源.stdout)?;
    let 源码 = 目录.join(format!("mackey-{}-source.tar.gz", crate::版本));
    原子写入(&路径, &源码, &编码器.finish()?, 0o644)?;
    附件.push(源码);
    let mut 归档 = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let 扩展目录 = 根.join("扩展").join(crate::扩展标识);
    fn 添加(
        归档: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
        根: &Path,
        目录: &Path,
    ) -> Result<()> {
        use std::io::Write;
        let mut 条目: Vec<_> = fs::read_dir(目录)?.collect::<std::io::Result<_>>()?;
        条目.sort_by_key(|项| 项.file_name());
        for 项 in 条目 {
            let 源 = 项.path();
            let 类型 = 项.file_type()?;
            if 类型.is_dir() {
                添加(归档, 根, &源)?;
            } else {
                ensure!(类型.is_file(), "扩展不能包含软链或设备文件");
                归档.start_file(
                    源.strip_prefix(根)?.to_string_lossy(),
                    zip::write::SimpleFileOptions::default().unix_permissions(0o644),
                )?;
                归档.write_all(&fs::read(源)?)?;
            }
        }
        Ok(())
    }
    添加(&mut 归档, &扩展目录, &扩展目录)?;
    let 扩展包 = 目录.join(format!("mackey-gnome-extension-{}.zip", crate::版本));
    原子写入(&路径, &扩展包, &归档.finish()?.into_inner(), 0o644)?;
    附件.push(扩展包);
    附件.sort();
    let mut 清单 = String::new();
    for 附件 in 附件 {
        let mut 摘要 = Sha256::new();
        let mut 文件 = fs::File::open(&附件)?;
        let mut 缓冲 = [0; 65536];
        loop {
            let 数量 = 文件.read(&mut 缓冲)?;
            if 数量 == 0 {
                break;
            }
            摘要.update(&缓冲[..数量]);
        }
        清单.push_str(&format!(
            "{:x}  {}\n",
            摘要.finalize(),
            附件
                .file_name()
                .context("附件缺少文件名")?
                .to_string_lossy()
        ));
    }
    原子写入(
        &路径,
        &目录.join(format!("SHA256SUMS-{}.txt", crate::版本)),
        清单.as_bytes(),
        0o644,
    )?;
    println!("✓ 发布附件与校验和：{}", 目录.display());
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Read;
    #[test]
    fn 发布附件包含提交源码真实扩展与四项校验和() {
        let 临时 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let 根 = 临时.path();
        fs::write(根.join("README.md"), "源码探针").unwrap();
        let 扩展 = 根.join("扩展").join(crate::扩展标识);
        fs::create_dir_all(&扩展).unwrap();
        fs::write(扩展.join("extension.js"), "export default class 测试 {}").unwrap();
        fs::write(
            扩展.join("metadata.json"),
            "{\"uuid\":\"mackey-focus@kotonex\"}",
        )
        .unwrap();
        for 参数 in [
            vec!["init", "--quiet", "--initial-branch=测试"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=测试",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "初始化",
            ],
        ] {
            assert!(
                Command::new("git")
                    .args(参数)
                    .current_dir(根)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let 发行 = 根.join("builddir/发行");
        fs::create_dir_all(&发行).unwrap();
        for (架构, 机器) in [("x86_64", 62u16), ("aarch64", 183)] {
            let 文件 =
                fs::File::create(发行.join(format!("mackey-{}-linux-{架构}.tar.gz", crate::版本)))
                    .unwrap();
            let mut 包 = tar::Builder::new(flate2::write::GzEncoder::new(
                文件,
                flate2::Compression::default(),
            ));
            let mut 数据 = [0; 20];
            数据[..4].copy_from_slice(b"\x7fELF");
            数据[4] = 2;
            数据[5] = 1;
            数据[18..20].copy_from_slice(&机器.to_le_bytes());
            let mut 头 = tar::Header::new_gnu();
            头.set_size(20);
            头.set_mode(0o755);
            头.set_cksum();
            包.append_data(
                &mut 头,
                format!("mackey-{}-linux-{架构}/mackey", crate::版本),
                &数据[..],
            )
            .unwrap();
            包.into_inner().unwrap().finish().unwrap();
        }
        组装附件(根).unwrap();
        let 清单 =
            fs::read_to_string(发行.join(format!("SHA256SUMS-{}.txt", crate::版本))).unwrap();
        assert_eq!(清单.lines().count(), 4);
        for 行 in 清单.lines() {
            let (摘要, 文件) = 行.split_once("  ").unwrap();
            assert_eq!(
                摘要,
                format!("{:x}", Sha256::digest(fs::read(发行.join(文件)).unwrap()))
            );
        }
        let mut 扩展包 = zip::ZipArchive::new(
            fs::File::open(发行.join(format!("mackey-gnome-extension-{}.zip", crate::版本)))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(扩展包.len(), 2);
        let mut 文本 = String::new();
        扩展包
            .by_name("extension.js")
            .unwrap()
            .read_to_string(&mut 文本)
            .unwrap();
        assert_eq!(文本, "export default class 测试 {}");
        let mut 源码 = tar::Archive::new(flate2::read::GzDecoder::new(
            fs::File::open(发行.join(format!("mackey-{}-source.tar.gz", crate::版本))).unwrap(),
        ));
        let mut 找到 = false;
        for 项 in 源码.entries().unwrap() {
            let mut 项 = 项.unwrap();
            if 项.path().unwrap() == Path::new(&format!("mackey-{}/README.md", crate::版本)) {
                let mut 内容 = String::new();
                项.read_to_string(&mut 内容).unwrap();
                assert_eq!(内容, "源码探针");
                找到 = true;
            }
        }
        assert!(找到);
    }
}
