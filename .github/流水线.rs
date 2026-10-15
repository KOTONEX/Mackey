// SPDX-License-Identifier: AGPL-3.0-or-later
// 仅用标准库编译，先准备原生 TypeScript，避免 Cargo 构建脚本的启动依赖环。
use std::{
    env, fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
type 结果<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn 要求(条件: bool, 提示: &str) -> 结果 {
    if !条件 {
        return Err(提示.into());
    }
    Ok(())
}
fn 字符串(路径: &Path) -> 结果<&str> {
    路径.to_str().ok_or_else(|| "流水线路径必须为 UTF-8".into())
}
fn 读取(根: &Path, 程序: &str, 参数: &[&str]) -> 结果<String> {
    let 输出 = Command::new(程序).args(参数).current_dir(根).output()?;
    if !输出.status.success() {
        return Err(format!("{程序} 失败：{}", String::from_utf8_lossy(&输出.stderr)).into());
    }
    Ok(String::from_utf8(输出.stdout)?)
}
fn 运行(根: &Path, 程序: &str, 参数: &[&str]) -> 结果 {
    println!("→ {程序} {}", 参数.join(" "));
    print!("{}", 读取(根, 程序, 参数)?);
    Ok(())
}
fn 保护(主目录: &Path, 路径: &Path) -> 结果<PathBuf> {
    要求(路径.is_absolute(), "输出必须是绝对路径")?;
    let mut 结果 = PathBuf::new();
    for 分段 in 路径.components() {
        要求(分段 != Component::ParentDir, "输出路径不能包含 ..")?;
        结果.push(分段.as_os_str());
        match fs::symlink_metadata(&结果) {
            Ok(_) => 结果 = fs::canonicalize(&结果)?,
            Err(错误) if 错误.kind() == std::io::ErrorKind::NotFound => {}
            Err(错误) => return Err(错误.into()),
        }
    }
    要求(结果.starts_with(主目录), "流水线输出必须位于 HOME 下")?;
    Ok(结果)
}
fn 复制树(源: &Path, 目标: &Path) -> 结果 {
    let 属性 = fs::symlink_metadata(源)?;
    要求(!属性.file_type().is_symlink(), "源码副本不接受软链")?;
    if 属性.is_dir() {
        fs::create_dir(目标)?;
        for 项 in fs::read_dir(源)? {
            let 项 = 项?;
            复制树(&项.path(), &目标.join(项.file_name()))?;
        }
    } else {
        要求(属性.is_file(), "源码副本只接受普通文件和目录")?;
        fs::copy(源, 目标)?;
    }
    Ok(())
}
fn 平台架构(架构: &str) -> 结果<&'static str> {
    match 架构 {
        "X64" => Ok("x64"),
        "ARM64" => Ok("arm64"),
        _ => Err(format!("不支持的 runner 架构：{架构}").into()),
    }
}
fn 校验摘要(期望: &str, 实际: &str) -> 结果 {
    要求(
        期望.len() == 64 && 期望.bytes().all(|字节| 字节.is_ascii_hexdigit()),
        "官方发行缺少有效的 SHA-256 摘要",
    )?;
    要求(
        期望.eq_ignore_ascii_case(实际),
        "原生编译器 SHA-256 校验失败",
    )
}
fn 校验归档路径(名称: &str) -> 结果 {
    let 路径 = Path::new(名称);
    要求(
        路径.starts_with("package")
            && 路径
                .components()
                .all(|分段| matches!(分段, Component::Normal(_))),
        "编译器归档包含越界路径",
    )
}
fn 平台输出(变量: &str, 文本: &str) -> 结果 {
    let 路径 = env::var_os(变量).ok_or_else(|| format!("缺少平台输出 {变量}"))?;
    // GITHUB_PATH/GITHUB_ENV 是 Actions 提供的临时命令文件，不是用户持久配置。
    writeln!(fs::OpenOptions::new().append(true).open(路径)?, "{文本}")?;
    Ok(())
}
fn 编译器(主目录: &Path) -> 结果 {
    let 架构 = 平台架构(&env::var("RUNNER_ARCH")?)?;
    let 资产 = format!("typescript-linux-{架构}.tgz");
    let 查询 = format!(
        "select(.draft == false and .prerelease == false) | [.tag_name, (.assets[] | select(.name == \"{资产}\") | .digest)] | @tsv"
    );
    let 发行 = 读取(
        主目录,
        "gh",
        &[
            "api",
            "repos/microsoft/TypeScript/releases/latest",
            "--jq",
            &查询,
        ],
    )?;
    let (标签, 摘要) = 发行
        .trim()
        .split_once('\t')
        .ok_or("官方正式发行缺少架构资产")?;
    要求(
        !标签.is_empty() && !标签.starts_with('-'),
        "官方发行标签无效",
    )?;
    let 摘要 = 摘要
        .strip_prefix("sha256:")
        .ok_or("官方资产未提供 SHA-256")?;
    let 时间 = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let 目录 = 保护(
        主目录,
        &主目录.join(format!(
            ".local/share/mackey-ci-tools/tsc-{}-{时间}",
            std::process::id()
        )),
    )?;
    fs::create_dir_all(&目录)?;
    运行(
        主目录,
        "gh",
        &[
            "release",
            "download",
            标签,
            "--repo",
            "microsoft/TypeScript",
            "--pattern",
            &资产,
            "--dir",
            字符串(&目录)?,
        ],
    )?;
    let 归档 = 目录.join(&资产);
    let 实际 = 读取(主目录, "sha256sum", &[字符串(&归档)?])?;
    校验摘要(摘要, 实际.split_whitespace().next().unwrap_or_default())?;
    let 名称 = 读取(主目录, "tar", &["-tzf", 字符串(&归档)?])?;
    for 项 in 名称.lines() {
        校验归档路径(项)?;
    }
    let 类型 = 读取(主目录, "tar", &["-tvzf", 字符串(&归档)?])?;
    要求(
        !类型.is_empty() && 类型.lines().all(|行| 行.starts_with(['-', 'd'])),
        "编译器归档只接受普通文件和目录",
    )?;
    运行(
        主目录,
        "tar",
        &[
            "-xzf",
            字符串(&归档)?,
            "--no-same-owner",
            "--no-same-permissions",
            "-C",
            字符串(&目录)?,
        ],
    )?;
    let 目录 = 保护(主目录, &目录.join("package/lib"))?;
    let 程序 = 目录.join("tsc");
    let mut 标识 = [0; 4];
    fs::File::open(&程序)?.read_exact(&mut 标识)?;
    要求(标识 == *b"\x7fELF", "tsc 必须为原生 ELF 编译器")?;
    println!("TypeScript 官方稳定发行：{标签}；架构：{架构}");
    运行(主目录, 字符串(&程序)?, &["--version"])?;
    运行(主目录, "rustc", &["--version"])?;
    平台输出("GITHUB_PATH", 字符串(&目录)?)
}
fn 准备(主目录: &Path, 模式: &str) -> 结果 {
    let 名称 = match 模式 {
        "构建" => "mackey-ci",
        "发行" => "mackey-release",
        _ => return Err("准备模式必须为构建或发行".into()),
    };
    let 源 = PathBuf::from(env::var_os("GITHUB_WORKSPACE").ok_or("缺少源码工作区")?);
    let 目标 = 保护(主目录, &主目录.join(名称))?;
    复制树(&源, &目标)?;
    fs::create_dir_all(目标.join("builddir/发行"))?;
    if 模式 == "构建" {
        // 只有 GitHub 临时 runner 准备阶段允许安装系统开发依赖。
        运行(主目录, "sudo", &["apt-get", "update"])?;
        运行(
            主目录,
            "sudo",
            &[
                "apt-get",
                "install",
                "-y",
                "gjs",
                "dbus-x11",
                "build-essential",
                "cmake",
            ],
        )?;
    }
    运行(
        主目录,
        "rustup",
        &[
            "toolchain",
            "install",
            "stable",
            "--profile",
            "minimal",
            "--component",
            "rustfmt",
            "--component",
            "clippy",
        ],
    )?;
    运行(主目录, "rustup", &["default", "stable"])?;
    编译器(主目录)?;
    let 工作区 = 字符串(&目标)?;
    要求(!工作区.contains(['\r', '\n']), "工作区不能包含换行")?;
    平台输出("GITHUB_ENV", &format!("MACKEY_WORKSPACE={工作区}"))
}
fn 执行() -> 结果 {
    要求(unsafe { geteuid() } != 0, "不要用 root/sudo 运行流水线")?;
    要求(
        env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
        "该工具仅供 GitHub Actions 临时 runner 使用",
    )?;
    let 主目录 = fs::canonicalize(env::var_os("HOME").ok_or("缺少 HOME")?)?;
    let 参数: Vec<_> = env::args().skip(1).collect();
    match 参数
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["准备", 模式] => 准备(&主目录, 模式),
        ["编译器"] => 编译器(&主目录),
        _ => Err("用法：流水线 准备 构建|发行，或 流水线 编译器".into()),
    }
}
unsafe extern "C" {
    fn geteuid() -> u32;
}
fn main() {
    if let Err(错误) = 执行() {
        eprintln!("✗ {错误}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 发行摘要与架构不匹配时拒绝继续() {
        assert_eq!(平台架构("X64").unwrap(), "x64");
        assert_eq!(平台架构("ARM64").unwrap(), "arm64");
        assert!(平台架构("ARM").is_err());
        assert!(校验摘要(&"a".repeat(64), &"b".repeat(64)).is_err());
        assert!(校验摘要("a", "a").is_err());
        assert!(校验摘要(&"a".repeat(64), &"A".repeat(64)).is_ok());
    }
    #[test]
    fn 编译器归档拒绝逃逸路径() {
        for 名称 in [
            "/etc/passwd",
            "package/../../escaped",
            "other/tsc",
            "./package/tsc",
        ] {
            assert!(校验归档路径(名称).is_err(), "不应接受 {名称}");
        }
        校验归档路径("package/lib/tsc").unwrap();
    }
    #[test]
    fn 保护真实主目录并拒绝源码软链() {
        let 主目录 = fs::canonicalize(env::var_os("HOME").unwrap()).unwrap();
        let 路径 = 主目录.join(format!(
            ".mackey-pipeline-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&路径).unwrap();
        struct 清理(PathBuf);
        impl Drop for 清理 {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _清理 = 清理(路径.clone());
        let 链接 = 路径.join("outside");
        std::os::unix::fs::symlink("/tmp", &链接).unwrap();
        assert!(保护(&主目录, &链接.join("escape")).is_err());
        assert!(保护(&主目录, &路径.join("../escape")).is_err());
        assert!(复制树(&链接, &路径.join("copy")).is_err());
        assert!(!路径.join("copy").exists());
    }
}
