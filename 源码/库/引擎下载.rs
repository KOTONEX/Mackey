// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::{写入结构数据, 原子写入, 读取结构数据, 路径集合};
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use reqwest::blocking::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const 压缩包大小上限: u64 = 256 * 1024 * 1024;
pub const 成员大小上限: u64 = 128 * 1024 * 1024;
const 下载来源: &str = "https://github.com/xremap/xremap/releases";
#[derive(Debug, Args)]
pub struct 选项 {
    #[arg(long)]
    pub 目标路径: Option<PathBuf>,
    #[arg(long)]
    pub 缓存: Option<PathBuf>,
    #[arg(long)]
    pub 状态目录: Option<PathBuf>,
    #[arg(long)]
    pub 标签: Option<String>,
    #[arg(long)]
    pub 架构: Option<String>,
    #[arg(long, default_value = "gnome")]
    pub 特性: String,
    #[arg(long)]
    pub 强制: bool,
    #[arg(long)]
    pub 输出计划: bool,
    #[arg(long, default_value_t = 60.0)]
    pub 超时: f64,
}
impl Default for 选项 {
    fn default() -> Self {
        Self {
            目标路径: None,
            缓存: None,
            状态目录: None,
            标签: None,
            架构: None,
            特性: "gnome".into(),
            强制: false,
            输出计划: false,
            超时: 60.0,
        }
    }
}
pub fn 识别架构(machine: &str) -> Result<&'static str> {
    match machine.trim().to_lowercase().as_str() {
        "x86_64" | "amd64" => Ok("x86_64"),
        "aarch64" | "arm64" => Ok("aarch64"),
        _ => bail!("不支持的指令集架构：{machine}"),
    }
}
pub fn 合法标签(标签: &str) -> bool {
    regex::Regex::new(r"^v[0-9][A-Za-z0-9_.-]*$")
        .unwrap()
        .is_match(标签)
}
pub fn 计算摘要(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn 二进制架构(bytes: &[u8]) -> Result<u16> {
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"\x7fELF",
        "ELF 头不完整或不是 ELF 文件"
    );
    match bytes[5] {
        1 => Ok(u16::from_le_bytes([bytes[18], bytes[19]])),
        2 => Ok(u16::from_be_bytes([bytes[18], bytes[19]])),
        _ => bail!("ELF 头 EI_DATA 非法"),
    }
}
pub fn 提取引擎(bytes: &[u8], 架构: &str) -> Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    ensure!(zip.len() == 1, "zip 必须只包含 xremap 成员");
    let mut member = zip.by_index(0)?;
    ensure!(
        member.name() == "xremap" && !member.is_dir(),
        "zip 里没有预期的 xremap 成员"
    );
    ensure!(
        member
            .unix_mode()
            .is_none_or(|m| m & libc::S_IFMT != libc::S_IFLNK),
        "拒绝 zip 软链成员"
    );
    ensure!(member.size() <= 成员大小上限, "xremap 成员超过大小上限");
    let mut 数据 = Vec::new();
    (&mut member)
        .take(成员大小上限 + 1)
        .read_to_end(&mut 数据)?;
    ensure!(数据.len() as u64 <= 成员大小上限, "xremap 实际大小超过上限");
    let want = match 架构 {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => bail!("未知 ELF 架构"),
    };
    ensure!(二进制架构(&数据)? == want, "ELF 架构不匹配，期望 {架构}");
    Ok(数据)
}
pub fn 需要安装(
    目标路径: &Path, 状态目录: &Path, 标签: &str, 架构: &str, 特性: &str
) -> bool {
    let Ok(record) = 读取结构数据(状态目录) else {
        return true;
    };
    if record["标签"] != 标签 || record["架构"] != 架构 || record["特性"] != 特性 {
        return true;
    }
    use std::os::unix::fs::PermissionsExt;
    if !fs::metadata(目标路径).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    {
        return true;
    }
    fs::read(目标路径)
        .map(|bytes| record["sha256"] != 计算摘要(&bytes))
        .unwrap_or(true)
}
fn 创建下载客户端(超时: f64) -> Result<Client> {
    ensure!(超时.is_finite() && 超时 > 0.0, "超时必须为正数");
    Ok(Client::builder()
        .https_only(true)
        .timeout(Duration::try_from_secs_f64(超时).context("超时超出可用范围")?)
        .user_agent("mackey-fetch-engine/2")
        .build()?)
}
pub fn 执行(路径: &路径集合, options: &选项) -> Result<()> {
    let 目标路径 = options
        .目标路径
        .clone()
        .unwrap_or(路径.数据.join("bin/xremap"));
    let 缓存 = options.缓存.clone().unwrap_or(路径.缓存.clone());
    let 状态目录 = options
        .状态目录
        .clone()
        .unwrap_or(路径.状态目录.join("engine.json"));
    for path in [&目标路径, &缓存, &状态目录] {
        路径.保护(path)?;
    }
    let 架构 = 识别架构(options.架构.as_deref().unwrap_or(std::env::consts::ARCH))?;
    let 特性 = if options.特性.is_empty() {
        "gnome"
    } else {
        &options.特性
    };
    ensure!(
        ["gnome", "full"].contains(&特性),
        "Mackey 仅支持 gnome / full 引擎特性"
    );
    let requested = options.标签.clone().or_else(|| {
        std::env::var("MACKEY_ENGINE_TAG")
            .ok()
            .filter(|s| !s.is_empty())
    });
    let 创建下载客户端 = 创建下载客户端(options.超时)?;
    let 标签 = if let Some(标签) = requested {
        标签
    } else {
        let 响应 = 创建下载客户端
            .get(format!("{下载来源}/latest"))
            .send()?
            .error_for_status()?;
        响应
            .url()
            .path_segments()
            .and_then(|mut s| s.next_back())
            .context("无法解析 latest 标签")?
            .to_owned()
    };
    ensure!(合法标签(&标签), "非法发布标签: {标签}");
    let features = if 特性 == "full" {
        vec!["full"]
    } else {
        vec!["gnome", "full"]
    };
    let candidates: Vec<_> = features
        .iter()
        .map(|f| format!("{下载来源}/download/{标签}/xremap-linux-{架构}-{f}.zip"))
        .collect();
    if options.输出计划 {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"标签":标签,"架构":架构,"特性":特性,"说明":"Mackey 仅支持 GNOME Wayland","候选地址":candidates,"目标路径":目标路径,"缓存":缓存,"状态文件":状态目录})
            )?
        );
        return Ok(());
    }
    if !options.强制 && !需要安装(&目标路径, &状态目录, &标签, 架构, 特性) {
        println!(
            "✓ 已是最新：{}（{标签} / {架构} / {特性}）",
            目标路径.display()
        );
        return Ok(());
    }
    let downloads = 缓存.join("downloads");
    路径.保护(&downloads)?;
    fs::create_dir_all(&downloads)?;
    let mut last = String::new();
    for (url, actual_feature) in candidates.iter().zip(features) {
        let attempt = || -> Result<()> {
            println!("→ 下载：{url}");
            let mut 响应 = 创建下载客户端.get(url).send()?.error_for_status()?;
            ensure!(响应.url().scheme() == "https", "下载最终地址不是 HTTPS");
            if let Some(length) = 响应.headers().get(reqwest::header::CONTENT_LENGTH) {
                let length: u64 = length.to_str()?.parse().context("Content-Length 非法")?;
                ensure!(length <= 压缩包大小上限, "资产声明大小超过上限");
            }
            let mut temp = tempfile::NamedTempFile::new_in(&downloads)?;
            let size = std::io::copy(&mut (&mut 响应).take(压缩包大小上限 + 1), &mut temp)?;
            ensure!(size <= 压缩包大小上限, "资产实际大小超过上限");
            temp.flush()?;
            temp.as_file().sync_all()?;
            let archive = fs::read(temp.path())?;
            let payload = 提取引擎(&archive, 架构)?;
            let asset = downloads.join(url.rsplit('/').next().unwrap());
            路径.保护(&asset)?;
            temp.persist(&asset).map_err(|e| e.error)?;
            原子写入(路径, &目标路径, &payload, 0o755)?;
            // 分别记录请求特性与实际特性，让 full 回退仍可幂等，
            // 同时准确记录下载的资产。
            写入结构数据(
                路径,
                &状态目录,
                &json!({"标签":标签,"架构":架构,"特性":特性,"实际特性":actual_feature,"资产":asset.file_name(),"地址":url,"sha256":计算摘要(&payload),"大小":size,"安装时间戳":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),"目标路径":目标路径}),
            )?;
            for entry in fs::read_dir(&downloads)? {
                let entry = entry?;
                let path = entry.path();
                if path != asset && entry.file_type()?.is_file() {
                    路径.保护(&path)?;
                    fs::remove_file(path)?;
                }
            }
            println!("✓ 已安装：{}", 目标路径.display());
            Ok(())
        };
        match attempt() {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = format!("{err:#}");
                eprintln!("! {last}");
            }
        }
    }
    bail!("全部候选都失败：{last}")
}
#[cfg(test)]
mod 测试 {
    use super::*;
    fn 测试程序内容(machine: u16) -> Vec<u8> {
        let mut b = vec![0; 64];
        b[..4].copy_from_slice(b"\x7fELF");
        b[5] = 1;
        b[18..20].copy_from_slice(&machine.to_le_bytes());
        b
    }
    fn 测试压缩包(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(payload).unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn 验证架构与标签() {
        assert_eq!(识别架构("AMD64").unwrap(), "x86_64");
        assert_eq!(识别架构("arm64").unwrap(), "aarch64");
        assert!(识别架构("riscv64").is_err());
        assert!(合法标签("v0.14.0"));
        assert!(!合法标签("v1/../../bad"));
    }
    #[test]
    fn 验证压缩成员与程序架构() {
        assert!(提取引擎(&测试压缩包("xremap", &测试程序内容(62)), "x86_64").is_ok());
        assert!(提取引擎(&测试压缩包("../xremap", &测试程序内容(62)), "x86_64").is_err());
        assert!(提取引擎(&测试压缩包("xremap", &测试程序内容(183)), "x86_64").is_err());
        assert!(提取引擎(&测试压缩包("xremap", b"not an ELF"), "x86_64").is_err());
        let mut be = 测试程序内容(62);
        be[5] = 2;
        be[18..20].copy_from_slice(&183u16.to_be_bytes());
        assert_eq!(二进制架构(&be).unwrap(), 183);
    }
    #[test]
    fn 安装幂等检查记录权限及内容() {
        use std::os::unix::fs::PermissionsExt;
        let 主目录 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let 目标路径 = 主目录.path().join("xremap");
        let 状态目录 = 主目录.path().join("engine.json");
        let payload = 测试程序内容(62);
        fs::write(&目标路径, &payload).unwrap();
        fs::set_permissions(&目标路径, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&状态目录, json!({"标签":"v0.15.0","架构":"x86_64","特性":"gnome","实际特性":"full","sha256":计算摘要(&payload)}).to_string()).unwrap();
        assert!(!需要安装(
            &目标路径,
            &状态目录,
            "v0.15.0",
            "x86_64",
            "gnome"
        ));
        assert!(需要安装(
            &目标路径,
            &状态目录,
            "v0.16.0",
            "x86_64",
            "gnome"
        ));
        fs::set_permissions(&目标路径, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(需要安装(
            &目标路径,
            &状态目录,
            "v0.15.0",
            "x86_64",
            "gnome"
        ));
        fs::set_permissions(&目标路径, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&目标路径, b"changed").unwrap();
        assert!(需要安装(
            &目标路径,
            &状态目录,
            "v0.15.0",
            "x86_64",
            "gnome"
        ));
        assert!(创建下载客户端(0.0).is_err());
        assert!(创建下载客户端(f64::INFINITY).is_err());
        assert!(创建下载客户端(f64::MAX).is_err());
    }
}
