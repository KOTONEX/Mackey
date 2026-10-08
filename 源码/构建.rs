// SPDX-License-Identifier: AGPL-3.0-or-later
use std::{
    env, fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
fn 构建() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { geteuid() } == 0 {
        return Err("不要用 root/sudo 构建 Mackey".into());
    }
    let 根 = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("缺少源码目录")?);
    let 主目录 = fs::canonicalize(env::var_os("HOME").ok_or("缺少 HOME")?)?;
    let 输出 = fs::canonicalize(env::var_os("OUT_DIR").ok_or("缺少 OUT_DIR")?)?;
    if !fs::canonicalize(&根)?.starts_with(&主目录) || !输出.starts_with(&主目录) {
        return Err("源码与编译输出必须位于 HOME 下".into());
    }
    for 文件 in [
        "tsconfig.json",
        "扩展",
        "测试/扩展契约.ts",
        "测试/基类桩.ts",
        "类型声明",
    ] {
        println!("cargo:rerun-if-changed={文件}");
    }
    println!("cargo:rerun-if-env-changed=HOME");
    println!("cargo:rerun-if-env-changed=PATH");
    let 编译器 = env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|目录| 目录.join("tsc"))
        .find(|文件| {
            fs::metadata(文件)
                .is_ok_and(|属性| 属性.is_file() && 属性.permissions().mode() & 0o111 != 0)
        })
        .ok_or("缺少原生 TypeScript 编译器 tsc，请将其目录加入 PATH")?;
    let mut 标识 = [0; 4];
    fs::File::open(&编译器)?.read_exact(&mut 标识)?;
    if 标识 != *b"\x7fELF" {
        return Err("tsc 必须是 Linux 原生编译器，不能使用脚本启动器".into());
    }
    let 产物 = 输出.join("类型脚本");
    let 状态 = Command::new(&编译器)
        .arg("-p")
        .arg(根.join("tsconfig.json"))
        .arg("--outDir")
        .arg(&产物)
        .current_dir(&根)
        .status()
        .map_err(|错误| format!("无法运行 TypeScript 编译器 tsc，请安装构建依赖：{错误}"))?;
    if !状态.success() {
        return Err("TypeScript 编译失败；拒绝嵌入旧 JavaScript 产物".into());
    }
    for 文件 in [
        "扩展/mackey-focus@kotonex/扩展.js",
        "测试/扩展契约.js",
        "测试/基类桩.js",
    ] {
        if !产物.join(Path::new(文件)).is_file() {
            return Err(format!("缺少编译产物：{文件}").into());
        }
    }
    Ok(())
}
unsafe extern "C" {
    fn geteuid() -> u32;
}
fn main() {
    if let Err(错误) = 构建() {
        panic!("{错误}");
    }
}
