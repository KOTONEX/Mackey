// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    扩展标识, 旧扩展标识,
    路径::{写入结构数据, 原子写入, 读取结构数据, 路径集合},
    配置生成::{self, 字符串列表, 转为加速键列表},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
const 服务列表: [&str; 2] = ["mackey-focusd.service", "mackey-engine.service"];
pub fn 执行命令(program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("运行 {program}"))
}
fn 执行并检查(program: &str, args: &[&str]) -> Result<Output> {
    let out = 执行命令(program, args)?;
    ensure!(
        out.status.success(),
        "{program} 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out)
}
fn 用户服务命令(args: &[&str]) -> Result<Output> {
    let mut all = vec!["--user"];
    all.extend(args);
    执行并检查("systemctl", &all)
}
fn 停用已有服务() -> Result<()> {
    // 先停引擎，避免焦点桥先退出时旧引擎仍处理按键。
    for 服务 in 服务列表.into_iter().rev() {
        let 状态 = 用户服务命令(&["show", "--property=LoadState", "--value", 服务])?;
        if String::from_utf8_lossy(&状态.stdout).trim() != "not-found" {
            用户服务命令(&["disable", "--now", 服务])?;
        }
    }
    Ok(())
}
pub fn 运行中(服务: &str) -> bool {
    执行命令("systemctl", &["--user", "is-active", "--quiet", 服务])
        .is_ok_and(|o| o.status.success())
}
pub fn 确认(message: &str, 确认执行: bool) -> Result<bool> {
    if 确认执行 || env::var("MACKEY_YES").as_deref() == Ok("1") {
        return Ok(true);
    }
    print!("{message} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}
pub fn 检查桌面() -> Result<()> {
    let desktop = env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| env::var("DESKTOP_SESSION"))
        .unwrap_or_default();
    ensure!(
        desktop.to_lowercase().contains("gnome"),
        "Mackey 仅支持 GNOME（当前桌面：{desktop}）"
    );
    Ok(())
}
pub fn 初始化(路径: &路径集合) -> Result<()> {
    路径.校验安装路径()?;
    let path = 路径.配置.join("config.json");
    let mut user = if path.exists() {
        match 读取结构数据(&path) {
            Ok(v) if v.is_object() => v,
            _ => {
                let 备份 = path.with_file_name(format!(
                    "config.json.bak-{}",
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
                ));
                路径.保护(&备份)?;
                fs::rename(&path, &备份)?;
                eprintln!("! 损坏配置已备份：{}", 备份.display());
                json!({})
            }
        }
    } else {
        json!({})
    };
    user = crate::用户配置::校验(user)?;
    let layout = crate::键盘识别::当前键盘()["建议布局"].clone();
    let defaults = json!({"修饰键布局":layout,"_修饰键布局_说明":"苹果 = ⌘ 就是 Super 键（Apple 键盘或机械键盘的 Mac 模式）；微软 = 把物理 Alt/Win 交换，让空格旁那颗键变成 ⌘（PC 配列键盘）","device":{},"_device_说明":"只重映射指定键盘，例如 {\"only\": [\"MX Keys\"]}；留空=全部键盘","keypress_delay_ms":0,"_keypress_delay_ms_说明":"Wayland 下个别应用对合成按键不敏感时，可设 20 试试","引擎":"","_引擎_说明":"自定义 xremap 路径；留空=用 XDG 数据目录里下载的引擎"});
    for (key, value) in defaults.as_object().unwrap() {
        user.as_object_mut()
            .unwrap()
            .entry(key)
            .or_insert(value.clone());
    }
    if user["修饰键布局"] == "自动" {
        user["修饰键布局"] = layout;
    }
    写入结构数据(路径, &path, &user)?;
    配置生成::执行(
        路径,
        &配置生成::选项 {
            不生成文档: true,
            ..Default::default()
        },
    )?;
    println!("✓ 初始化完成：{}", path.display());
    Ok(())
}
fn 可执行(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
pub fn 查找引擎(路径: &路径集合) -> Option<PathBuf> {
    let user = crate::用户配置::读取(&路径.配置.join("config.json")).unwrap_or(json!({}));
    let mut candidates = vec![
        env::var_os("MACKEY_ENGINE").map(PathBuf::from),
        user["引擎"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        Some(路径.数据.join("bin/xremap")),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".vendor/xremap")),
    ];
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|p| Some(p.join("xremap"))));
    }
    candidates.into_iter().flatten().find(|p| 可执行(p))
}
pub fn 转义systemd参数(s: &str) -> Result<String> {
    Ok(转义systemd环境(s)?.replace('$', "$$"))
}
fn 转义systemd环境(s: &str) -> Result<String> {
    ensure!(
        !s.contains(['\n', '\r', '\0']),
        "systemd 参数包含换行 / NUL"
    );
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}
fn 移除文件(路径: &路径集合, path: &Path) -> Result<()> {
    // 删除末尾软链本身；先检查父目录，再读取软链元信息，
    // 避免递归删除跟随软链到目标。
    路径.保护父目录(path.parent().context("删除路径没有父目录")?)?;
    let 元信息 = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if 元信息.is_dir() {
        路径.保护(path)?;
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
fn 移除套接字(路径: &路径集合) -> Result<()> {
    路径.保护套接字(&路径.套接字)?;
    use std::os::unix::fs::FileTypeExt;
    if let Ok(元信息) = fs::symlink_metadata(&路径.套接字) {
        ensure!(
            元信息.file_type().is_socket() || 元信息.is_file(),
            "拒绝移除非 socket / 文件"
        );
        if 元信息.file_type().is_socket() {
            ensure!(
                std::os::unix::net::UnixStream::connect(&路径.套接字).is_err(),
                "焦点 socket 仍在监听，拒绝删除"
            );
        }
        fs::remove_file(&路径.套接字)?;
    }
    Ok(())
}
pub fn 写入引擎服务(路径: &路径集合) -> Result<bool> {
    let user = crate::用户配置::读取(&路径.配置.join("config.json"))?;
    let unit = 路径.服务目录.join(服务列表[1]);
    let Some(引擎下载) = 查找引擎(路径) else {
        移除文件(路径, &unit)?;
        eprintln!("! 没找到 xremap，跳过引擎服务；请运行 mackey 获取引擎");
        return Ok(false);
    };
    let mut exec = 转义systemd参数(引擎下载.to_str().context("引擎路径不是 UTF-8")?)?;
    for device in 字符串列表(&user["device"]["only"]) {
        exec += &format!(" --device {}", 转义systemd参数(&device)?);
    }
    exec += &format!(
        " {}",
        转义systemd参数(
            路径
                .配置
                .join("xremap.json")
                .to_str()
                .context("配置路径不是 UTF-8")?
        )?
    );
    let 环境 = 转义systemd环境(&format!(
        "GNOME_SOCKET={}",
        路径.套接字.to_str().context("套接字路径不是 UTF-8")?
    ))?;
    原子写入(路径,&unit,format!("[Unit]\nDescription=Mackey 键位引擎（xremap）\nAfter=mackey-focusd.service\nRequires=mackey-focusd.service\nBindsTo=mackey-focusd.service\n\n[Service]\nType=simple\nEnvironment={环境}\nExecStart={exec}\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n").as_bytes(),0o644)?;
    Ok(true)
}
fn 移除扩展登记(uuid: &str) -> Result<()> {
    for key in ["enabled-extensions", "disabled-extensions"] {
        let out = 执行并检查("gsettings", &["get", "org.gnome.shell", key])?;
        let mut list = 配置生成::解析加速键列表(&String::from_utf8_lossy(&out.stdout));
        if list.iter().any(|s| s == uuid) {
            list.retain(|s| s != uuid);
            执行并检查(
                "gsettings",
                &["set", "org.gnome.shell", key, &转为加速键列表(&list)],
            )?;
        }
    }
    Ok(())
}
fn 入口属于本项目(路径: &路径集合) -> bool {
    if fs::read_link(&路径.命令入口).is_ok_and(|p| p == 路径.数据.join("bin/mackey")) {
        return true;
    }
    let 根 = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&路径.命令入口)
        .is_ok_and(|内容| 内容.contains(根.join("命令/mackey").to_str().unwrap_or("\0")))
}
pub fn 安装(路径: &路径集合, 不下载: bool, 保持运行: bool) -> Result<()> {
    检查桌面()?;
    路径.校验安装路径()?;
    初始化(路径)?;
    if !不下载
        && let Err(err) = crate::引擎下载::执行(
            路径,
            &crate::引擎下载::选项 {
                特性: "gnome".into(),
                超时: 60.0,
                ..Default::default()
            },
        )
    {
        eprintln!("! 引擎下载失败，继续安装扩展与服务：{err:#}");
    }
    if !保持运行 {
        停用已有服务()?;
        let _ = 用户服务命令(&["reset-failed", 服务列表[0], 服务列表[1]]);
        for 服务 in 服务列表 {
            移除文件(路径, &路径.服务目录.join("default.target.wants").join(服务))?;
        }
        移除套接字(路径)?;
    }
    let ext = 路径.扩展路径(扩展标识);
    移除文件(路径, &ext)?;
    原子写入(路径, &ext.join("extension.js"), crate::扩展脚本, 0o644)?;
    原子写入(
        路径,
        &ext.join("metadata.json"),
        include_bytes!("../../扩展/mackey-focus@kotonex/metadata.json"),
        0o644,
    )?;
    let legacy = 路径.扩展路径(旧扩展标识);
    if legacy.exists() {
        let _ = 执行命令("gnome-extensions", &["disable", 旧扩展标识]);
        移除扩展登记(旧扩展标识)?;
        移除文件(路径, &legacy)?;
    }
    let binary = 路径.数据.join("bin/mackey");
    let 当前键盘 = env::current_exe()?;
    if fs::canonicalize(&binary).ok() != Some(当前键盘.clone()) {
        原子写入(路径, &binary, &fs::read(当前键盘)?, 0o755)?;
    }
    let exec = 转义systemd参数(binary.to_str().context("二进制路径不是 UTF-8")?)?;
    let 套接字 = 转义systemd参数(路径.套接字.to_str().context("套接字路径不是 UTF-8")?)?;
    原子写入(路径,&路径.服务目录.join(服务列表[0]),format!("[Unit]\nDescription=Mackey 焦点上报桥\n\n[Service]\nType=simple\nExecStart={exec} 焦点桥 --套接字 {套接字}\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n").as_bytes(),0o644)?;
    写入引擎服务(路径)?;
    用户服务命令(&["daemon-reload"])?;
    let parent = 路径.命令入口.parent().context("入口没有父目录")?;
    路径.保护父目录(parent)?;
    fs::create_dir_all(parent)?;
    if fs::symlink_metadata(&路径.命令入口).is_ok() {
        if 入口属于本项目(路径) {
            移除文件(路径, &路径.命令入口)?;
        } else {
            let 备份 = 路径.命令入口.with_file_name(format!(
                "mackey.bak-{}",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
            路径.保护(&备份)?;
            fs::rename(&路径.命令入口, &备份)?;
            eprintln!("! 同名入口已备份：{}", 备份.display());
        }
    }
    std::os::unix::fs::symlink(&binary, &路径.命令入口)?;
    println!(
        "✓ 已安装 Rust 二进制、扩展与用户服务\n重新登录以加载 GNOME 扩展及 input 组权限，再运行 mackey 体检 && mackey 应用\n若尚未加入 input 组，请在本机手动运行 sudo usermod -aG input <用户名>，然后重新登录\n彻底卸载：mackey 卸载"
    );
    Ok(())
}
fn 校验备份(值: &serde_json::Value) -> Result<Vec<(&str, &str, Vec<String>)>> {
    let 对象 = 值.as_object().context("备份必须是 JSON 对象")?;
    对象
        .iter()
        .map(|(标识, 值)| {
            let (模式, 键名) = 标识.split_once(' ').context("备份绑定标识无效")?;
            ensure!(
                !模式.is_empty()
                    && !键名.is_empty()
                    && !模式.chars().any(char::is_whitespace)
                    && !键名.chars().any(char::is_whitespace),
                "备份绑定标识无效：{标识}"
            );
            let 值 = 值.as_array().context("备份键位必须是字符串数组")?;
            let 值 = 值
                .iter()
                .map(|项| {
                    项.as_str()
                        .map(str::to_owned)
                        .context("备份键位必须是字符串数组")
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((模式, 键名, 值))
        })
        .collect()
}
pub fn 还原(路径: &路径集合) -> Result<()> {
    let 备份 = 路径.备份();
    if !备份.exists() {
        println!("没有备份，无需还原");
        return Ok(());
    }
    let old = 读取结构数据(&备份)?;
    let old = 校验备份(&old)?;
    let mut failed = Vec::new();
    for (schema, key, accels) in old {
        if let Err(err) =
            执行并检查("gsettings", &["set", schema, key, &转为加速键列表(&accels)])
        {
            failed.push(format!("{schema} {key}: {err}"));
        }
    }
    ensure!(
        failed.is_empty(),
        "GNOME 键位还原失败，备份保留：{}",
        failed.join("、")
    );
    移除文件(路径, &备份)?;
    println!("✓ GNOME 键位还原完成（备份已清除）");
    Ok(())
}
pub fn 应用(路径: &路径集合, 确认执行: bool) -> Result<()> {
    let generated = 配置生成::执行(
        路径,
        &配置生成::选项 {
            不生成文档: true,
            ..Default::default()
        },
    )?;
    if !generated.迁移计划.is_empty() {
        ensure!(
            确认(
                &format!(
                    "要迁移 {} 条 GNOME 键位（先备份，可还原）？",
                    generated.迁移计划.len()
                ),
                确认执行
            )?,
            "已取消，未启动引擎"
        );
        let 备份 = 路径.备份();
        let mut old = if 备份.exists() {
            读取结构数据(&备份)?
        } else {
            json!({})
        };
        校验备份(&old)?;
        let map = old.as_object_mut().context("备份必须为对象")?;
        for r in &generated.迁移计划 {
            map.entry(format!("{} {}", r.模式, r.键名))
                .or_insert(json!(r.原值));
        }
        写入结构数据(路径, &备份, &old)?;
        for (编号, r) in generated.迁移计划.iter().enumerate() {
            if let Err(错误) = 执行并检查(
                "gsettings",
                &["set", &r.模式, &r.键名, &转为加速键列表(&r.新值)],
            ) {
                let mut 恢复失败 = Vec::new();
                // 只回滚本次尝试的键，不能用最早备份覆盖用户后来做的改动。
                for 已改 in generated.迁移计划[..=编号].iter().rev() {
                    if let Err(恢复错误) = 执行并检查(
                        "gsettings",
                        &["set", &已改.模式, &已改.键名, &转为加速键列表(&已改.原值)],
                    ) {
                        恢复失败.push(format!("{} {}：{恢复错误}", 已改.模式, 已改.键名));
                    }
                }
                bail!(
                    "GNOME 键位迁移失败：{错误}；本次回滚失败 {} 条：{}；备份保留，可运行 mackey 还原",
                    恢复失败.len(),
                    恢复失败.join("、")
                );
            }
        }
    }
    // 运行中的 xremap 可能已加载旧配置。
    停用()?;
    启用(路径)
}
pub fn 停用() -> Result<()> {
    停用已有服务()?;
    println!("✓ 服务已停用（键盘恢复 Linux 原生行为）");
    Ok(())
}
fn 具有输入组() -> bool {
    执行命令("id", &["-nG"]).is_ok_and(|o| {
        String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .any(|g| g == "input")
    })
}
pub fn 启用(路径: &路径集合) -> Result<()> {
    crate::用户配置::读取(&路径.配置.join("config.json"))?;
    let _ = 执行命令("gnome-extensions", &["enable", 扩展标识]);
    if let Err(err) = crate::焦点桥::验证焦点("自动") {
        let _ = 停用();
        bail!("焦点来源不可用，已拒绝启动引擎：{err}；安装扩展后请重新登录");
    }
    ensure!(
        具有输入组(),
        "当前会话没有 input 组：引擎读不到 /dev/input，请重新登录使组权限生效"
    );
    ensure!(写入引擎服务(路径)?, "没有可用引擎");
    用户服务命令(&["daemon-reload"])?;
    用户服务命令(&["enable", "--now", 服务列表[0]])?;
    // 引擎的应用过滤依赖焦点桥，启动引擎前必须验证套接字，
    // 桥接失败时不得让终端退化为通用映射。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ready = false;
    while std::time::Instant::now() < deadline {
        if let Ok(mut 套接字) = std::os::unix::net::UnixStream::connect(&路径.套接字) {
            use std::io::{BufRead, BufReader};
            套接字.set_read_timeout(Some(std::time::Duration::from_millis(250)))?;
            if writeln!(套接字, "\"ActiveWindow\"").is_ok() {
                let mut line = String::new();
                if BufReader::new(套接字).read_line(&mut line).is_ok()
                    && serde_json::from_str::<crate::焦点桥::焦点信息>(&line).is_ok()
                {
                    ready = true;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !ready {
        let _ = 停用();
        bail!("焦点桥未就绪，拒绝启动引擎");
    }
    用户服务命令(&["enable", "--now", 服务列表[1]])?;
    状态(路径);
    Ok(())
}
pub fn 状态(路径: &路径集合) {
    println!("── Mackey 状态 ──");
    println!(
        "引擎：{}",
        查找引擎(路径)
            .map(|p| p.display().to_string())
            .unwrap_or("未找到".into())
    );
    for 服务 in 服务列表 {
        println!(
            "{服务}: {}",
            if 运行中(服务) {
                "运行中"
            } else {
                "未运行"
            }
        );
    }
    if let Ok(out) = 执行命令("gnome-extensions", &["info", 扩展标识]) {
        print!("{}", String::from_utf8_lossy(&out.stdout));
    }
}
pub fn 体检(路径: &路径集合) -> Result<()> {
    状态(路径);
    let mut problems = Vec::new();
    if 检查桌面().is_err() {
        problems.push("当前会话不是 GNOME".to_owned());
    }
    if !具有输入组() {
        problems.push("当前会话没有 input 组；添加组后需重新登录".into());
    }
    use std::ffi::CString;
    let uinput = CString::new("/dev/uinput")?;
    if unsafe { libc::access(uinput.as_ptr(), libc::W_OK) } != 0 {
        problems.push("/dev/uinput 不可写".into());
    }
    if 查找引擎(路径).is_none() {
        problems.push("找不到 xremap；运行 mackey 获取引擎".into());
    }
    if let Err(err) = crate::焦点桥::验证焦点("自动") {
        problems.push(format!("焦点来源不可用：{err}"));
    }
    if let Err(err) = 配置生成::执行(
        路径,
        &配置生成::选项 {
            报告: true,
            ..Default::default()
        },
    ) {
        problems.push(format!("键位探测 / 冲突检查失败：{err:#}"));
    }
    for problem in &problems {
        eprintln!("! {problem}");
    }
    println!("{} 项待处理", problems.len());
    ensure!(problems.is_empty(), "体检未通过");
    Ok(())
}
pub fn 卸载(
    路径: &路径集合, 清除配置: bool, dry: bool, 确认执行: bool
) -> Result<()> {
    let mut targets: Vec<_> = 服务列表
        .iter()
        .flat_map(|s| {
            [
                路径.服务目录.join(s),
                路径.服务目录.join("default.target.wants").join(s),
            ]
        })
        .collect();
    targets.extend([
        路径.命令入口.clone(),
        路径.扩展路径(扩展标识),
        路径.扩展路径(旧扩展标识),
        路径.数据.clone(),
        路径.缓存.clone(),
        路径.状态目录.clone(),
    ]);
    if 清除配置 {
        targets.push(路径.配置.clone());
    }
    if dry {
        for target in &targets {
            println!("会清理：{}", target.display());
        }
        println!("会还原 GNOME 键位并清理 {}", 路径.套接字.display());
        return Ok(());
    }
    路径.校验安装路径()?;
    ensure!(
        确认("卸载会停用服务、还原键位、删除扩展与引擎，继续？", 确认执行)?,
        "已取消"
    );
    if 路径.备份().exists() {
        校验备份(&读取结构数据(&路径.备份())?)?;
    }
    停用已有服务()?;
    for 服务 in 服务列表 {
        移除文件(路径, &路径.服务目录.join(服务))?;
        移除文件(路径, &路径.服务目录.join("default.target.wants").join(服务))?;
    }
    用户服务命令(&["daemon-reload"])?;
    for uuid in [扩展标识, 旧扩展标识] {
        let _ = 执行命令("gnome-extensions", &["disable", uuid]);
        移除扩展登记(uuid)?;
        移除文件(路径, &路径.扩展路径(uuid))?;
    }
    if 入口属于本项目(路径) {
        移除文件(路径, &路径.命令入口)?;
    }
    let restored = 还原(路径);
    for path in [&路径.数据, &路径.缓存, &路径.状态目录] {
        移除文件(路径, path)?;
    }
    移除套接字(路径)?;
    // 还原失败时保留备份；清除配置也不能跳过这个保护。
    restored?;
    if 清除配置 {
        移除文件(路径, &路径.配置)?;
    }
    println!("✓ 卸载完成");
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 转义服务参数的特殊字符() {
        assert_eq!(
            转义systemd参数("MX Keys % ${VAR} \"\\").unwrap(),
            "\"MX Keys %% $${VAR} \\\"\\\\\""
        );
        assert!(转义systemd参数("bad\nname").is_err());
    }
}
