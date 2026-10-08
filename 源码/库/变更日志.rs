// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    开发::仓库,
    路径::{原子写入, 路径集合},
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, path::Path, process::Command};

fn 读取提交信息(根: &Path, 参数: &[&str]) -> Result<String> {
    let 输出 = Command::new("git").args(参数).current_dir(根).output()?;
    ensure!(
        输出.status.success(),
        "Git 失败：{}",
        String::from_utf8_lossy(&输出.stderr)
    );
    Ok(String::from_utf8(输出.stdout)?.trim().to_owned())
}
fn 分类(标题: &str) -> (&str, &str) {
    if let Some((类型, 内容)) = 标题.split_once(':') {
        let 分组 = match 类型.trim() {
            "新增" | "feat" => "新增",
            "修复" | "fix" => "修复",
            "文档" | "docs" => "文档",
            "测试" | "test" => "测试",
            "重构" | "refactor" => "重构",
            "杂务" | "chore" => "杂务",
            "初始化" | "init" => "初始化",
            _ => "其他",
        };
        if 分组 != "其他" {
            return (分组, 内容.trim());
        }
    }
    ("其他", 标题)
}
fn 范围说明(根: &Path, 范围: &str) -> Result<String> {
    let 日志 = 读取提交信息(根, &["log", "--no-merges", "--format=%s", 范围, "--"])?;
    let mut 分组: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for 标题 in 日志.lines() {
        let (类型, 内容) = 分类(标题);
        分组.entry(类型).or_default().push(内容);
    }
    let mut 文本 = String::new();
    for 类型 in [
        "新增",
        "修复",
        "重构",
        "文档",
        "测试",
        "杂务",
        "初始化",
        "其他",
    ] {
        if let Some(列表) = 分组.get(类型) {
            文本 += &format!("### {类型}\n\n");
            for 内容 in 列表 {
                文本 += &format!("- {内容}\n");
            }
            文本.push('\n');
        }
    }
    Ok(文本)
}
pub fn 生成(仅发布说明: bool) -> Result<std::path::PathBuf> {
    let 根 = 仓库()?;
    ensure!(
        读取提交信息(&根, &["rev-parse", "--is-shallow-repository"])? == "false",
        "变更日志需要完整 Git 历史和版本标签；请先补全检出历史"
    );
    let 标签文本 = 读取提交信息(
        &根,
        &[
            "tag",
            "--merged",
            "HEAD",
            "--list",
            "v*",
            "--sort=-version:refname",
        ],
    )?;
    let 标签: Vec<_> = 标签文本.lines().collect();
    let mut 文本 = if 仅发布说明 {
        format!("# Mackey {}\n\n", crate::版本)
    } else {
        "# 变更日志\n\n> 根据 Git 提交和版本标签生成；未提交的修改不会列入。\n\n".to_owned()
    };
    let 当前标签 = 格式标签();
    if 仅发布说明 {
        let 前一标签 = 标签.iter().find(|标签| **标签 != 当前标签);
        let 范围 = 前一标签
            .map(|标签| format!("{标签}..HEAD"))
            .unwrap_or("HEAD".into());
        文本 += &范围说明(&根, &范围)?;
    } else {
        let 范围 = 标签
            .first()
            .map(|标签| format!("{标签}..HEAD"))
            .unwrap_or("HEAD".into());
        let 未发布 = 范围说明(&根, &范围)?;
        if !未发布.is_empty() {
            文本 += &format!("## 未发布\n\n{未发布}");
        }
        for (序号, 标签名) in 标签.iter().enumerate() {
            let 范围 = 标签
                .get(序号 + 1)
                .map(|前一| format!("{前一}..{标签名}"))
                .unwrap_or((*标签名).into());
            文本 += &format!("## {标签名}\n\n{}", 范围说明(&根, &范围)?);
        }
    }
    let 文件 = 根.join(if 仅发布说明 {
        "builddir/发布说明.md"
    } else {
        "builddir/CHANGELOG.md"
    });
    原子写入(&路径集合::发现()?, &文件, 文本.as_bytes(), 0o644)?;
    println!("✓ {}", 文件.display());
    Ok(文件)
}
fn 格式标签() -> String {
    format!("v{}", crate::版本)
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 中文提交与历史标题分类() {
        assert_eq!(分类("重构: 原生 Rust 焦点桥"), ("重构", "原生 Rust 焦点桥"));
        assert_eq!(分类("fix: socket cleanup"), ("修复", "socket cleanup"));
        assert_eq!(分类("旧提交"), ("其他", "旧提交"));
    }
}
