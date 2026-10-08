// SPDX-License-Identifier: AGPL-3.0-or-later
// 测试专用的外部命令桩；发行包不包含此程序。
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::Path};
fn 执行() -> anyhow::Result<()> {
    let mut 参数 = env::args();
    let 入口 = 参数.next().unwrap_or_default();
    let 名称 = Path::new(&入口).file_name().unwrap_or_default();
    let 参数: Vec<_> = 参数.collect();
    if 参数.first().map(String::as_str) == Some("动作探针") {
        let 路径 = mackey::路径::路径集合::发现()?;
        let 文件 = 参数
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("缺少动作探针路径"))?;
        return mackey::路径::原子写入(&路径, Path::new(文件), b"", 0o600);
    }
    if let Some(路径) = env::var_os("MACKEY_TEST_LOG") {
        let mut 日志 = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(路径)?;
        writeln!(
            日志,
            "{}",
            json!({"程序":名称.to_string_lossy(),"参数":参数})
        )?;
    }
    if 名称 == "systemctl" {
        let 单元目录 =
            std::path::PathBuf::from(env::var_os("XDG_CONFIG_HOME").unwrap()).join("systemd/user");
        if 参数.get(1).map(String::as_str) == Some("show") {
            let 单元 = 参数.last().unwrap();
            println!(
                "{}",
                if 单元目录.join(单元).exists() {
                    "loaded"
                } else {
                    "not-found"
                }
            );
        }
        if 参数.get(1).map(String::as_str) == Some("disable") {
            for 单元 in 参数.iter().filter(|参数| 参数.ends_with(".service")) {
                anyhow::ensure!(
                    单元目录.join(单元).exists(),
                    "Failed to disable unit: Unit {单元} does not exist"
                );
            }
        }
    }
    if 名称 == "gsettings" {
        if 参数.first().map(String::as_str) == Some("set")
            && 参数.get(1).map(String::as_str) == Some("org.gnome.desktop.wm.keybindings")
            && env::var("MACKEY_TEST_FAIL_RESTORE").as_deref() == Ok("1")
        {
            anyhow::bail!("模拟 GSettings 还原失败");
        }
        if 参数.first().map(String::as_str) == Some("get") {
            let 种子: Value = env::var_os("MACKEY_TEST_SEED")
                .and_then(|路径| fs::read(路径).ok())
                .and_then(|内容| serde_json::from_slice(&内容).ok())
                .unwrap_or(json!({}));
            let 键 = 参数.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
            println!("{}", 种子[键].as_str().unwrap_or("[]"));
        }
        if 参数.first().map(String::as_str) == Some("set")
            && let Some(路径) = env::var_os("MACKEY_TEST_SEED")
        {
            let mut 种子: Value = fs::read(&路径)
                .ok()
                .and_then(|内容| serde_json::from_slice(&内容).ok())
                .unwrap_or(json!({}));
            let 键 = 参数
                .iter()
                .skip(1)
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            种子[键] = json!(参数.get(3).cloned().unwrap_or_default());
            fs::write(路径, serde_json::to_vec(&种子)?)?;
        }
    }
    Ok(())
}
fn main() {
    if let Err(错误) = 执行() {
        eprintln!("{错误:#}");
        std::process::exit(1);
    }
}
