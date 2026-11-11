// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::路径集合;
use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
    process::Command,
    time::Duration,
};

/// 只接受 GitHub 的正式发行；预发行、草稿与任意标签都不属于自动升级范围。
const 最新发行页: &str = "https://github.com/KOTONEX/Mackey/releases/latest";
const 标签前缀: &str = "https://github.com/KOTONEX/Mackey/releases/tag/";
const 下载前缀: &str = "https://github.com/KOTONEX/Mackey/releases/download/";
const 压缩包大小上限: u64 = 256 * 1024 * 1024;
const 校验清单大小上限: u64 = 1024 * 1024;
const 归档成员大小上限: u64 = 128 * 1024 * 1024;

pub fn 识别架构(machine: &str) -> Result<&'static str> {
    match machine.trim().to_lowercase().as_str() {
        "x86_64" | "amd64" => Ok("x86_64"),
        "aarch64" | "arm64" => Ok("aarch64"),
        _ => bail!("不支持的指令集架构：{machine}"),
    }
}

pub fn 发行资产名(版本: &str, 架构: &str) -> (String, String) {
    (
        format!("mackey-{版本}-linux-{架构}.tar.gz"),
        format!("SHA256SUMS-{版本}.txt"),
    )
}

pub fn 版本序(版本: &str) -> Result<Vec<u64>> {
    ensure!(!版本.is_empty(), "版本号为空");
    版本
        .split('.')
        .map(|段| 段.parse::<u64>().context("版本号必须由数字段组成"))
        .collect()
}

fn 比较版本(左: &str, 右: &str) -> Result<Ordering> {
    Ok(版本序(左)?.cmp(&版本序(右)?))
}

/// 从 `sha256sum` 兼容的校验清单中取指定文件的摘要。
pub fn 校验和(清单: &str, 文件名: &str) -> Result<String> {
    for 行 in 清单.lines() {
        let mut 部分 = 行.split_whitespace();
        let (Some(摘要), Some(名称)) = (部分.next(), 部分.next()) else {
            continue;
        };
        let 名称 = 名称.trim_start_matches('*').trim_start_matches("./");
        if 名称 != 文件名 {
            continue;
        }
        ensure!(
            摘要.len() == 64 && 摘要.bytes().all(|字节| 字节.is_ascii_hexdigit()),
            "校验清单摘要非法：{摘要}"
        );
        return Ok(摘要.to_ascii_lowercase());
    }
    bail!("校验清单里没有 {文件名}")
}

/// 从 /releases/latest 的最终跳转地址解析标签；只接受标签页，不接受查询参数或子路径。
fn 解析标签(地址: &str) -> Result<String> {
    let 标签 = 地址
        .strip_prefix(标签前缀)
        .with_context(|| format!("最新发行未跳转到标签页：{地址}"))?
        .trim_end_matches('/');
    ensure!(
        标签.starts_with('v') && 标签.len() > 1,
        "发行标签缺少 v 前缀：{标签}"
    );
    ensure!(
        标签.bytes().all(|字节| 字节.is_ascii_alphanumeric()
            || 字节 == b'.'
            || 字节 == b'-'
            || 字节 == b'_'),
        "发行标签包含非法字符：{标签}"
    );
    版本序(&标签[1..])?;
    Ok(标签.to_string())
}

fn 下载地址(标签: &str, 文件名: &str) -> String {
    format!("{下载前缀}{标签}/{文件名}")
}

fn 校验程序(数据: &[u8], 架构: &str) -> Result<()> {
    ensure!(
        数据.len() >= 20 && &数据[..4] == b"\x7fELF",
        "下载的 mackey 不是 ELF 文件"
    );
    ensure!(是64位小端elf(数据), "下载的 mackey 必须是 64 位小端 ELF");
    let 机器 = u16::from_le_bytes([数据[18], 数据[19]]);
    let 期望 = match 架构 {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => bail!("未知架构：{架构}"),
    };
    ensure!(机器 == 期望, "ELF 架构不匹配：期望 {架构}");
    Ok(())
}

fn 是64位小端elf(数据: &[u8]) -> bool {
    数据.get(4) == Some(&2) && 数据.get(5) == Some(&1) && 数据.get(6) == Some(&1)
}

fn 校验成员路径(路径: &Path, 根: &str) -> Result<()> {
    let 组件: Vec<_> = 路径.components().collect();
    ensure!(
        !组件.is_empty() && 组件.iter().all(|组件| matches!(组件, Component::Normal(_))),
        "归档成员路径非法：{}",
        路径.display()
    );
    ensure!(
        组件[0].as_os_str() == std::ffi::OsStr::new(根),
        "归档成员不在预期根目录：{}",
        路径.display()
    );
    Ok(())
}

/// 只读取发行包里的 `mackey` 二进制；其余成员仅做路径与类型校验，不落盘。
pub fn 提取二进制(压缩包: &[u8], 版本: &str, 架构: &str) -> Result<Vec<u8>> {
    let 根 = format!("mackey-{版本}-linux-{架构}");
    let mut 归档 = tar::Archive::new(flate2::read::GzDecoder::new(std::io::Cursor::new(压缩包)));
    let mut 程序: Option<Vec<u8>> = None;
    for 条目 in 归档.entries()? {
        let mut 条目 = 条目?;
        let 路径 = 条目.path()?.into_owned();
        校验成员路径(&路径, &根)?;
        let 组件: Vec<_> = 路径.components().collect();
        let 类型 = 条目.header().entry_type();
        ensure!(
            类型.is_dir() || 类型.is_file(),
            "归档包含不支持的文件类型：{}",
            路径.display()
        );
        if 组件.len() == 2 && 组件[1].as_os_str() == "mackey" && 类型.is_file() {
            ensure!(程序.is_none(), "归档包含多个 mackey 二进制");
            let 大小 = 条目.header().size()?;
            ensure!(大小 <= 归档成员大小上限, "二进制超过大小上限");
            let mut 数据 = Vec::new();
            (&mut 条目)
                .take(归档成员大小上限 + 1)
                .read_to_end(&mut 数据)?;
            ensure!(数据.len() as u64 == 大小, "二进制读取长度与声明不一致");
            程序 = Some(数据);
        }
    }
    let 数据 = 程序.context("归档里没有 mackey 二进制")?;
    校验程序(&数据, 架构)?;
    Ok(数据)
}

fn 创建客户端() -> Result<Client> {
    Ok(Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(120))
        .user_agent("mackey-upgrade/1")
        .build()?)
}

/// /releases/latest 只跳转到最新正式发行；无需 GitHub API，也不受未认证限流影响。
fn 最新标签(客户端: &Client) -> Result<String> {
    let 响应 = 客户端
        .get(最新发行页)
        .send()
        .context("无法查询最新发行")?
        .error_for_status()
        .context("最新发行查询失败")?;
    解析标签(响应.url().as_str())
}

fn 下载字节(客户端: &Client, 地址: &str, 上限: u64) -> Result<Vec<u8>> {
    let mut 响应 = 客户端
        .get(地址)
        .send()?
        .error_for_status()
        .with_context(|| format!("下载失败：{地址}"))?;
    ensure!(响应.url().scheme() == "https", "下载最终地址不是 HTTPS");
    if let Some(长度) = 响应.headers().get(reqwest::header::CONTENT_LENGTH) {
        let 长度: u64 = 长度.to_str()?.parse().context("Content-Length 非法")?;
        ensure!(长度 <= 上限, "下载内容声明大小超过上限");
    }
    let mut 数据 = Vec::new();
    (&mut 响应).take(上限 + 1).read_to_end(&mut 数据)?;
    ensure!(数据.len() as u64 <= 上限, "下载内容实际大小超过上限");
    Ok(数据)
}

fn 下载文本(客户端: &Client, 地址: &str, 上限: u64) -> Result<String> {
    let 字节 = 下载字节(客户端, 地址, 上限)?;
    String::from_utf8(字节).context("下载内容不是 UTF-8")
}

pub fn 执行(路径: &路径集合) -> Result<()> {
    let 架构 = 识别架构(std::env::consts::ARCH)?;
    let 客户端 = 创建客户端()?;
    println!("→ 查询最新正式发行");
    let 标签 = 最新标签(&客户端)?;
    let 最新 = 标签.strip_prefix('v').context("发行标签缺少 v 前缀")?;
    match 比较版本(最新, crate::版本)? {
        Ordering::Equal => {
            println!("✓ 已是最新正式发行：{标签}（当前 {}）", crate::版本);
            return Ok(());
        }
        Ordering::Less => {
            println!("当前版本 {} 高于最新正式发行 {标签}，未升级", crate::版本);
            return Ok(());
        }
        Ordering::Greater => {}
    }
    let (压缩包名, 清单名) = 发行资产名(最新, 架构);
    let 清单地址 = 下载地址(&标签, &清单名);
    let 压缩包地址 = 下载地址(&标签, &压缩包名);
    let 目录 = 路径.缓存.join("升级");
    路径.保护(&目录)?;
    fs::create_dir_all(&目录)?;
    println!("→ 下载校验清单：{清单地址}");
    let 清单 = 下载文本(&客户端, &清单地址, 校验清单大小上限)?;
    let 期望 = 校验和(&清单, &压缩包名)?;
    println!("→ 下载发布包：{压缩包地址}");
    let 压缩包 = 下载字节(&客户端, &压缩包地址, 压缩包大小上限)?;
    let 实际 = format!("{:x}", Sha256::digest(&压缩包));
    ensure!(
        实际.eq_ignore_ascii_case(&期望),
        "发布包 SHA-256 校验失败：期望 {期望}，实际 {实际}"
    );
    println!("✓ SHA-256 校验通过：{实际}");
    let 程序 = 提取二进制(&压缩包, 最新, 架构)?;
    let 临时 = tempfile::Builder::new()
        .prefix("mackey-升级-")
        .tempdir_in(&目录)?;
    let 新程序 = 临时.path().join("mackey");
    fs::write(&新程序, &程序)?;
    fs::set_permissions(&新程序, fs::Permissions::from_mode(0o755))?;
    println!("→ 执行新版本安装（保持运行）：{标签}");
    let 状态 = Command::new(&新程序)
        .args(["安装", "--保持运行", "--不下载"])
        .status()
        .context("无法执行新版本安装")?;
    ensure!(状态.success(), "新版本安装失败（{状态}）");
    临时.close()?;
    println!(
        "✓ 已升级：{} → {最新}；重新登录加载扩展，再运行 mackey 应用 && mackey 状态",
        crate::版本
    );
    Ok(())
}

#[cfg(test)]
mod 测试 {
    use super::*;
    use std::path::Path;

    fn 程序(机器: u16) -> Vec<u8> {
        let mut 数据 = vec![0u8; 64];
        数据[..4].copy_from_slice(b"\x7fELF");
        数据[4] = 2;
        数据[5] = 1;
        数据[6] = 1;
        数据[18..20].copy_from_slice(&机器.to_le_bytes());
        数据
    }

    fn 归档(成员: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
        let mut 构建 = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (路径, 数据, 类型) in 成员 {
            let mut 头 = tar::Header::new_gnu();
            头.set_size(数据.len() as u64);
            头.set_mode(0o644);
            头.set_entry_type(*类型);
            if *类型 == tar::EntryType::Symlink {
                头.set_link_name("target").unwrap();
            }
            头.set_cksum();
            构建.append_data(&mut 头, 路径, *数据).unwrap();
        }
        构建.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn 识别架构与发行资产名() {
        assert_eq!(识别架构("AMD64").unwrap(), "x86_64");
        assert_eq!(识别架构("arm64").unwrap(), "aarch64");
        assert!(识别架构("riscv64").is_err());
        assert_eq!(
            发行资产名("2.4.4", "x86_64"),
            (
                "mackey-2.4.4-linux-x86_64.tar.gz".into(),
                "SHA256SUMS-2.4.4.txt".into()
            )
        );
    }

    #[test]
    fn 比较版本与解析校验和() {
        assert_eq!(比较版本("2.4.10", "2.4.9").unwrap(), Ordering::Greater);
        assert_eq!(比较版本("2.4.4", "2.4.4").unwrap(), Ordering::Equal);
        assert!(版本序("2.x.0").is_err());
        let 摘要 = "a".repeat(64);
        let 清单 = format!("{摘要}  b.tgz\n{摘要}  ./a.tgz\n");
        assert_eq!(校验和(&清单, "a.tgz").unwrap(), 摘要);
        assert!(校验和(&清单, "c.tgz").is_err());
        assert!(校验和("xyz  a.tgz\n", "a.tgz").is_err());
    }

    #[test]
    fn 校验成员路径拒绝越界与绝对路径() {
        let 根 = "mackey-2.4.4-linux-x86_64";
        assert!(校验成员路径(Path::new(根), 根).is_ok());
        assert!(校验成员路径(Path::new("mackey-2.4.4-linux-x86_64/mackey"), 根).is_ok());
        assert!(校验成员路径(Path::new("../evil"), 根).is_err());
        assert!(校验成员路径(Path::new("/etc/passwd"), 根).is_err());
        assert!(校验成员路径(Path::new("other/mackey"), 根).is_err());
        assert!(校验成员路径(Path::new(""), 根).is_err());
    }

    #[test]
    fn 解析标签只接受正式发行标签页() {
        let 页 = "https://github.com/KOTONEX/Mackey/releases/tag/v2.4.4";
        assert_eq!(解析标签(页).unwrap(), "v2.4.4");
        assert_eq!(解析标签(&format!("{页}/")).unwrap(), "v2.4.4");
        assert!(解析标签("https://example.com/other").is_err());
        assert!(解析标签("https://github.com/KOTONEX/Mackey/releases/tag/2.4.4").is_err());
        assert!(解析标签("https://github.com/KOTONEX/Mackey/releases/tag/v2.4.4?x=1").is_err());
        assert!(解析标签("https://github.com/KOTONEX/Mackey/releases/tag/v../evil").is_err());
        assert_eq!(
            下载地址("v2.4.4", "SHA256SUMS-2.4.4.txt"),
            "https://github.com/KOTONEX/Mackey/releases/download/v2.4.4/SHA256SUMS-2.4.4.txt"
        );
    }

    #[test]
    fn 提取二进制只接受预期布局() {
        let 根 = "mackey-2.4.4-linux-x86_64";
        let 二进制 = 程序(62);
        let 合法 = 归档(&[
            (根, b"", tar::EntryType::Directory),
            (&format!("{根}/mackey"), &二进制, tar::EntryType::Regular),
            (&format!("{根}/LICENSE"), b"MIT", tar::EntryType::Regular),
        ]);
        assert_eq!(提取二进制(&合法, "2.4.4", "x86_64").unwrap(), 二进制);
        let 软链 = 归档(&[
            (根, b"", tar::EntryType::Directory),
            (&format!("{根}/mackey"), b"", tar::EntryType::Symlink),
        ]);
        assert!(提取二进制(&软链, "2.4.4", "x86_64").is_err());
        let 缺程序 = 归档(&[(根, b"", tar::EntryType::Directory)]);
        assert!(提取二进制(&缺程序, "2.4.4", "x86_64").is_err());
        let 根目录不符 = 归档(&[
            ("other", b"", tar::EntryType::Directory),
            ("other/mackey", &二进制, tar::EntryType::Regular),
        ]);
        assert!(提取二进制(&根目录不符, "2.4.4", "x86_64").is_err());
        let 架构不符 = 归档(&[
            (根, b"", tar::EntryType::Directory),
            (&format!("{根}/mackey"), &程序(183), tar::EntryType::Regular),
        ]);
        assert!(提取二进制(&架构不符, "2.4.4", "x86_64").is_err());
    }
}
