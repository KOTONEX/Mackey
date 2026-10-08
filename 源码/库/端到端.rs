// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::{原子写入, 路径集合};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct 测试进程 {
    子进程: Child,
    日志: PathBuf,
}
impl 测试进程 {
    fn 启动(命令: &mut Command, 日志: &Path) -> Result<Self> {
        let 文件 = fs::File::create(日志)?;
        let 子进程 = 命令
            .process_group(0)
            .stdout(Stdio::from(文件.try_clone()?))
            .stderr(Stdio::from(文件))
            .spawn()?;
        Ok(Self {
            子进程,
            日志: 日志.into(),
        })
    }
    fn 等待(&mut self, 描述: &str, mut 就绪: impl FnMut() -> bool) -> Result<()> {
        let 截止 = Instant::now() + Duration::from_secs(8);
        loop {
            if 就绪() {
                return Ok(());
            }
            if let Some(状态) = self.子进程.try_wait()? {
                anyhow::bail!(
                    "{描述}前进程退出：{状态}\n{}",
                    fs::read_to_string(&self.日志).unwrap_or_default()
                );
            }
            ensure!(
                Instant::now() < 截止,
                "等待{描述}超时\n{}",
                fs::read_to_string(&self.日志).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn 结束(&mut self) -> Result<()> {
        let 截止 = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(状态) = self.子进程.try_wait()? {
                ensure!(状态.success(), "测试进程失败：{状态}");
                return Ok(());
            }
            ensure!(Instant::now() < 截止, "测试进程退出超时");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for 测试进程 {
    fn drop(&mut self) {
        // 每个测试进程拥有独立进程组；只清理本测试启动的进程与其子进程。
        let 进程组 = self.子进程.id() as i32;
        unsafe {
            libc::kill(-进程组, libc::SIGTERM);
        }
        let 截止 = Instant::now() + Duration::from_millis(300);
        while self.子进程.try_wait().ok().flatten().is_none() && Instant::now() < 截止 {
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            libc::kill(-进程组, libc::SIGKILL);
        }
        let _ = self.子进程.wait();
    }
}
fn 确保引擎隔离() -> Result<()> {
    for 进程 in fs::read_dir("/proc")? {
        let 进程 = 进程?;
        let 命令 = fs::read(进程.path().join("cmdline")).unwrap_or_default();
        let 参数: Vec<_> = 命令
            .split(|字节| *字节 == 0)
            .filter(|项| !项.is_empty())
            .collect();
        if let Some(入口) = 参数.first()
            && Path::new(std::ffi::OsStr::from_bytes(入口))
                .file_name()
                .is_some_and(|名称| 名称 == "xremap")
        {
            ensure!(
                参数
                    .windows(2)
                    .any(|组| 组[0] == b"--device" && 组[1].starts_with(b"mackey-test-")),
                "已有全局或非测试 xremap 运行；先手动停止它，再运行端到端测试"
            );
        }
    }
    Ok(())
}
use std::os::unix::ffi::OsStrExt;
fn 设备节点(名称: &str) -> Option<PathBuf> {
    fs::read_dir("/sys/class/input")
        .ok()?
        .filter_map(|项| 项.ok())
        .find_map(|项| {
            let 节点 = 项.file_name();
            if !节点.to_string_lossy().starts_with("event") {
                return None;
            }
            (fs::read_to_string(项.path().join("device/name"))
                .ok()?
                .trim()
                == 名称)
                .then(|| Path::new("/dev/input").join(节点))
        })
}
fn 已打开设备(进程: u32, 设备: &Path) -> bool {
    fs::read_dir(format!("/proc/{进程}/fd"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|项| 项.ok())
        .any(|项| fs::read_link(项.path()).is_ok_and(|目标| 目标 == 设备))
}
struct 用例<'a> {
    名称: &'a str,
    应用: Option<&'a str>,
    映射: Value,
    事件: &'a str,
    命中: &'a [&'a str],
    排除: &'a [&'a str],
}
fn 运行用例(
    目录: &Path,
    程序: &Path,
    注入器: &Path,
    引擎: &Path,
    用例: 用例<'_>,
) -> Result<()> {
    let 路径 = 路径集合::发现()?;
    let 工作目录 = 目录.join(用例.名称);
    路径.保护(&工作目录)?;
    fs::create_dir(&工作目录)?;
    let 套接字 = 工作目录.join("focus.sock");
    let 放行 = 工作目录.join("开始");
    let 映射 = 工作目录.join("映射.json");
    let mut 焦点命令 = Command::new(程序);
    焦点命令.args(["焦点桥", "--套接字"]).arg(&套接字);
    if let Some(应用) = 用例.应用 {
        焦点命令.args(["--后端", "固定", "--固定应用", 应用]);
    }
    let mut 焦点 = 测试进程::启动(&mut 焦点命令, &工作目录.join("焦点.log"))?;
    焦点.等待("焦点套接字", || 套接字.exists())?;
    fn 替换(值: &mut Value, 工作目录: &str, 探针: &str) {
        match 值 {
            Value::String(文本) => {
                *文本 = 文本.replace("@探针@", 工作目录).replace("@动作探针@", 探针)
            }
            Value::Array(值) => 值.iter_mut().for_each(|值| 替换(值, 工作目录, 探针)),
            Value::Object(值) => 值.values_mut().for_each(|值| 替换(值, 工作目录, 探针)),
            _ => {}
        }
    }
    let 探针 = 程序.with_file_name("mackey-测试桩");
    let mut 映射内容 = 用例.映射;
    替换(
        &mut 映射内容,
        工作目录.to_str().context("测试路径不是 UTF-8")?,
        探针.to_str().context("探针路径不是 UTF-8")?,
    );
    原子写入(&路径, &映射, &serde_json::to_vec(&映射内容)?, 0o600)?;
    let 名称 = format!(
        "mackey-test-{}-{}",
        目录.file_name().unwrap().to_string_lossy(),
        用例.名称
    );
    let mut 注入命令 = Command::new(注入器);
    注入命令
        .args(["--名称", &名称, "--事件", 用例.事件, "--等待文件"])
        .arg(&放行)
        .args(["--存活秒数", "0.5"]);
    let mut 键盘 = 测试进程::启动(&mut 注入命令, &工作目录.join("键盘.log"))?;
    键盘.等待("虚拟键盘设备及读取权限", || {
        设备节点(&名称).is_some_and(|节点| fs::File::open(节点).is_ok())
    })?;
    let 节点 = 设备节点(&名称).context("虚拟键盘设备消失")?;
    let mut 引擎命令 = Command::new(引擎);
    let 输出名称 = format!("mackey-test-output-{}-{}", std::process::id(), 用例.名称);
    引擎命令
        .arg("--device")
        .arg(&名称)
        .arg("--output-device-name")
        .arg(&输出名称)
        .arg(&映射)
        .env("GNOME_SOCKET", &套接字)
        .env("RUST_LOG", "xremap=debug");
    let mut 引擎 = 测试进程::启动(&mut 引擎命令, &工作目录.join("引擎.log"))?;
    let 编号 = 引擎.子进程.id();
    // 引擎列举设备时也会短暂打开输入节点。输出设备创建在正式抓取与
    // 事件处理器初始化之后，必须同时验证这两个条件才能放行按键。
    引擎.等待("引擎抓取输入并创建输出设备", || {
        已打开设备(编号, &节点) && 设备节点(&输出名称).is_some()
    })?;
    原子写入(&路径, &放行, b"", 0o600)?;
    键盘.结束()?;
    引擎.等待("全部预期动作", || {
        用例.命中.iter().all(|文件| 工作目录.join(文件).exists())
    })?;
    for 文件 in 用例.排除 {
        ensure!(!工作目录.join(文件).exists(), "{}不应触发{文件}", 用例.名称);
    }
    println!("✓ {}", 用例.名称);
    Ok(())
}
pub fn 执行(根: &Path, 引擎: &Path) -> Result<()> {
    确保引擎隔离()?;
    ensure!(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .is_ok(),
        "/dev/uinput 不可写"
    );
    let 路径 = 路径集合::发现()?;
    let 临时 = tempfile::Builder::new()
        .prefix(".mackey-e2e-")
        .tempdir_in(&路径.主目录)?;
    let 构建 = crate::验证::构建目录(根)?;
    let 程序 = 构建.join("debug/mackey");
    let 注入器 = 构建.join("debug/examples/虚拟键盘");
    let 应用 = if crate::焦点桥::验证焦点("自动").is_ok() {
        None
    } else {
        Some("TestTerminal")
    };
    for (名称, 匹配, 命中, 排除) in [
        ("应用正例", "/.+/", &["专属"][..], &["兜底"][..]),
        (
            "应用反例",
            "/this-app-does-not-exist/",
            &["兜底"][..],
            &["专属"][..],
        ),
    ] {
        let 映射 = json!({"keymap":[{"name":"app-specific","application":{"only":[匹配]},"remap":{"Super-c":{"launch":["@动作探针@","动作探针","@探针@/专属"]}}},{"name":"generic","remap":{"Super-c":{"launch":["@动作探针@","动作探针","@探针@/兜底"]}}}]});
        运行用例(
            临时.path(),
            &程序,
            &注入器,
            引擎,
            用例 {
                名称,
                应用,
                映射,
                事件: "LEFTMETA:1,C:1,C:0,LEFTMETA:0",
                命中,
                排除,
            },
        )?;
    }
    println!(
        "应用分流焦点来源：{}",
        if 应用.is_none() {
            "真实 D-Bus"
        } else {
            "固定来源；真实扩展由独立契约测试验证"
        }
    );
    for (名称, 应用, 命中, 排除) in [
        (
            "终端例外",
            "kitty",
            &["显式", "通用"][..],
            &["泛化", "吞键"][..],
        ),
        (
            "IDE例外",
            "code",
            &["通用", "泛化"][..],
            &["显式", "吞键"][..],
        ),
        (
            "其它应用",
            "some-app",
            &["通用", "泛化", "吞键"][..],
            &["显式"][..],
        ),
    ] {
        let 映射 = json!({"keymap":[{"name":"terminal-like","application":{"only":["/^kitty$/"]},"remap":{"Super-c":{"launch":["@动作探针@","动作探针","@探针@/显式"]}}},{"name":"generic","remap":{"Super-l":{"launch":["@动作探针@","动作探针","@探针@/通用"]}}},{"name":"generic-sweep","application":{"not":["/^kitty$/"]},"remap":{"Super-b":{"launch":["@动作探针@","动作探针","@探针@/泛化"]}}},{"name":"swallow","application":{"not":["/^kitty$/","/^code$/"]},"remap":{"C-c":{"launch":["@动作探针@","动作探针","@探针@/吞键"]}}}]});
        运行用例(
            临时.path(),
            &程序,
            &注入器,
            引擎,
            用例 {
                名称,
                应用: Some(应用),
                映射,
                事件: "LEFTMETA:1,C:1,C:0,LEFTMETA:0,LEFTMETA:1,L:1,L:0,LEFTMETA:0,LEFTMETA:1,B:1,B:0,LEFTMETA:0,LEFTCTRL:1,C:1,C:0,LEFTCTRL:0",
                命中,
                排除,
            },
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 进程组退出时子进程被回收() {
        let 临时 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let mut 命令 = Command::new("sleep");
        命令.arg("60");
        let 进程 = 测试进程::启动(&mut 命令, &临时.path().join("日志")).unwrap();
        let 编号 = 进程.子进程.id();
        drop(进程);
        assert!(!Path::new(&format!("/proc/{编号}")).exists());
    }
}
