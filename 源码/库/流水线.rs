// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn 校验标签(引用: &str, 标签: &str) -> Result<()> {
    ensure!(
        !引用.starts_with("refs/tags/") || 标签 == format!("v{}", crate::版本),
        "发行标签必须与 Cargo 项目版本一致"
    );
    Ok(())
}
fn 属于主线(根: &Path, 提交: &str) -> Result<bool> {
    let 引用 = "refs/remotes/origin/主线";
    ensure!(
        Command::new("git")
            .args(["show-ref", "--verify", "--quiet", 引用])
            .current_dir(根)
            .status()?
            .success(),
        "缺少远程主线，拒绝判定正式发行"
    );
    let 状态 = Command::new("git")
        .args(["merge-base", "--is-ancestor", 提交, 引用])
        .current_dir(根)
        .status()?;
    ensure!(
        matches!(状态.code(), Some(0 | 1)),
        "无法判断发行提交是否属于主线"
    );
    Ok(状态.success())
}
fn 校验草稿(信息: &[u8]) -> Result<()> {
    let 信息: Value = serde_json::from_slice(信息)?;
    ensure!(
        信息["isDraft"] == true,
        "该版本已公开，拒绝覆盖附件；修正应发布新版本。"
    );
    Ok(())
}
fn 公开(根: &Path, 标签: &str) -> Result<()> {
    let 仓库 = std::env::var("GITHUB_REPOSITORY").context("缺少 GitHub 仓库")?;
    let 提交 = std::env::var("GITHUB_SHA").context("缺少发行提交")?;
    let 正式 = 属于主线(根, &提交)?;
    let 调用 = |参数: Vec<String>| {
        Command::new("gh")
            .arg("release")
            .args(参数)
            .args(["--repo", &仓库])
            .current_dir(根)
            .output()
    };
    let 检查 = 调用(vec![
        "view".into(),
        标签.into(),
        "--json".into(),
        "isDraft".into(),
    ])?;
    // 网络错误不会触发覆盖：新建同标签发行会由 GitHub 拒绝，上传只对已确认草稿执行。
    let 已有草稿 = 检查.status.success();
    if 已有草稿 {
        校验草稿(&检查.stdout)?;
    }
    let 发行目录 = 根.join("builddir/发行");
    let mut 附件 = Vec::new();
    for 名称 in [
        format!("mackey-{}-linux-x86_64.tar.gz", crate::版本),
        format!("mackey-{}-linux-aarch64.tar.gz", crate::版本),
        format!("mackey-{}-source.tar.gz", crate::版本),
        format!("mackey-gnome-extension-{}.zip", crate::版本),
        format!("SHA256SUMS-{}.txt", crate::版本),
    ] {
        let 路径 = 发行目录.join(名称);
        ensure!(
            fs::symlink_metadata(&路径)?.is_file(),
            "发行附件必须是普通文件"
        );
        附件.push(路径.to_str().context("附件路径必须是 UTF-8")?.to_owned());
    }
    let 运行 = |参数: Vec<String>| -> Result<()> {
        let 输出 = 调用(参数)?;
        ensure!(
            输出.status.success(),
            "GitHub 发行操作失败：{}",
            String::from_utf8_lossy(&输出.stderr)
        );
        print!("{}", String::from_utf8_lossy(&输出.stdout));
        Ok(())
    };
    let 说明 = 根.join("builddir/发布说明.md");
    let 说明 = 说明.to_str().context("发布说明路径必须是 UTF-8")?;
    if 已有草稿 {
        let mut 参数 = vec!["upload".into(), 标签.into(), "--clobber".into()];
        参数.extend(附件);
        运行(参数)?;
        运行(vec![
            "edit".into(),
            标签.into(),
            "--notes-file".into(),
            说明.into(),
        ])?;
    } else {
        let mut 参数 = vec![
            "create".into(),
            标签.into(),
            "--verify-tag".into(),
            "--draft".into(),
            "--prerelease".into(),
            "--title".into(),
            format!("Mackey {标签}"),
            "--notes-file".into(),
            说明.into(),
        ];
        参数.extend(附件);
        运行(参数)?;
    }
    运行(vec![
        "edit".into(),
        标签.into(),
        "--draft=false".into(),
        format!("--prerelease={}", !正式),
        format!("--latest={正式}"),
    ])
}
pub fn 执行(操作: &str) -> Result<()> {
    ensure!(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
        "流水线命令仅供 GitHub Actions 使用"
    );
    let 引用 = std::env::var("GITHUB_REF").context("缺少 GitHub 引用")?;
    let 标签 = std::env::var("GITHUB_REF_NAME").context("缺少 GitHub 标签或分支名")?;
    校验标签(&引用, &标签)?;
    match 操作 {
        "校验标签" => Ok(()),
        "公开发行" => {
            ensure!(
                引用.starts_with("refs/tags/v"),
                "公开发行必须由版本标签触发"
            );
            公开(&crate::开发::仓库()?, &标签)
        }
        _ => anyhow::bail!("未知流水线操作：{操作}"),
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 错误标签与已公开发行不得继续() {
        校验标签("refs/heads/主线", "主线").unwrap();
        校验标签(
            &format!("refs/tags/v{}", crate::版本),
            &format!("v{}", crate::版本),
        )
        .unwrap();
        assert!(校验标签("refs/tags/v0.0.0", "v0.0.0").is_err());
        assert!(校验草稿(br#"{"isDraft":false}"#).is_err());
        assert!(校验草稿(br#"{"isDraft":"true"}"#).is_err());
        assert!(校验草稿(b"{}").is_err());
        校验草稿(br#"{"isDraft":true}"#).unwrap();
    }
    #[test]
    fn 正式发行必须属于远程主线历史() {
        let 临时 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let 根 = 临时.path();
        let git = |参数: &[&str]| {
            let 输出 = Command::new("git")
                .args(参数)
                .current_dir(根)
                .output()
                .unwrap();
            assert!(
                输出.status.success(),
                "{}",
                String::from_utf8_lossy(&输出.stderr)
            );
            String::from_utf8(输出.stdout).unwrap().trim().to_owned()
        };
        git(&["init", "--initial-branch=主线"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "first",
        ]);
        let 首次 = git(&["rev-parse", "HEAD"]);
        assert!(属于主线(根, &首次).is_err());
        git(&["update-ref", "refs/remotes/origin/主线", &首次]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "second",
        ]);
        let 新增 = git(&["rev-parse", "HEAD"]);
        assert!(属于主线(根, &首次).unwrap());
        assert!(!属于主线(根, &新增).unwrap());
        git(&["update-ref", "refs/remotes/origin/主线", &新增]);
        assert!(属于主线(根, &新增).unwrap());
        assert!(属于主线(根, &首次).unwrap());
    }
}
