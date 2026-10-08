// SPDX-License-Identifier: AGPL-3.0-or-later
// 测试专用 uinput 注入器；xremap 必须仅抓取其设备名。
use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    os::fd::{AsRawFd, RawFd},
    path::PathBuf,
    time::Duration,
};
#[derive(Parser)]
struct 虚拟键盘参数 {
    #[arg(long, default_value = "")]
    事件: String,
    #[arg(long)]
    等待文件: Option<PathBuf>,
    #[arg(long, default_value_t = 0.5)]
    存活秒数: f64,
    #[arg(long, default_value_t = 0.03)]
    间隔秒数: f64,
    #[arg(long, default_value = "mackey-test-kbd")]
    名称: String,
}
#[repr(C)]
struct 键盘设置 {
    id: [u16; 4],
    名称: [u8; 80],
    ff_effects_max: u32,
}
#[repr(C)]
struct 输入事件 {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}
const 键码表: &[(&str, u16)] = &[
    ("ESC", 1),
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("MINUS", 12),
    ("EQUAL", 13),
    ("BACKSPACE", 14),
    ("TAB", 15),
    ("Q", 16),
    ("W", 17),
    ("E", 18),
    ("R", 19),
    ("T", 20),
    ("Y", 21),
    ("U", 22),
    ("I", 23),
    ("O", 24),
    ("P", 25),
    ("LEFTBRACE", 26),
    ("RIGHTBRACE", 27),
    ("ENTER", 28),
    ("LEFTCTRL", 29),
    ("A", 30),
    ("S", 31),
    ("D", 32),
    ("F", 33),
    ("G", 34),
    ("H", 35),
    ("J", 36),
    ("K", 37),
    ("L", 38),
    ("SEMICOLON", 39),
    ("APOSTROPHE", 40),
    ("GRAVE", 41),
    ("LEFTSHIFT", 42),
    ("BACKSLASH", 43),
    ("Z", 44),
    ("X", 45),
    ("C", 46),
    ("V", 47),
    ("B", 48),
    ("N", 49),
    ("M", 50),
    ("COMMA", 51),
    ("DOT", 52),
    ("SLASH", 53),
    ("RIGHTSHIFT", 54),
    ("LEFTALT", 56),
    ("SPACE", 57),
    ("CAPSLOCK", 58),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
    ("RIGHTCTRL", 97),
    ("RIGHTALT", 100),
    ("LEFTMETA", 125),
    ("RIGHTMETA", 126),
    ("F13", 183),
    ("F14", 184),
    ("F15", 185),
    ("UP", 103),
    ("DOWN", 108),
    ("LEFT", 105),
    ("RIGHT", 106),
    ("HOME", 102),
    ("END", 107),
    ("DELETE", 111),
    ("INSERT", 110),
    ("PAGEUP", 104),
    ("PAGEDOWN", 109),
];
fn 解析事件(send: &str) -> Result<Vec<(u16, i32)>> {
    send.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (key, value) = item.split_once(':').context("按键格式必须为 KEY:VALUE")?;
            let code = 键码表
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key.trim()))
                .context("未知按键")?
                .1;
            let value = value.trim().parse()?;
            ensure!((0..=2).contains(&value), "按键值必须为 0/1/2");
            Ok((code, value))
        })
        .collect()
}
fn 调用设备控制(fd: RawFd, request: libc::c_ulong, arg: libc::c_int) -> Result<()> {
    // fd 是打开的 /dev/uinput 描述符；下列请求采用标量整数参数。
    let rc = unsafe { libc::ioctl(fd, request, arg) };
    if rc < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
struct 虚拟设备(std::fs::File);
impl Drop for 虚拟设备 {
    fn drop(&mut self) {
        let _ = 调用设备控制(self.0.as_raw_fd(), 0x5502, 0);
    }
}
fn 发送事件(file: &mut std::fs::File, kind: u16, code: u16, value: i32) -> Result<()> {
    let event = 输入事件 {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        kind,
        code,
        value,
    };
    // 事件使用 repr(C)，完整初始化，匹配 Linux input_event 的原生布局，
    // 支持 x86_64/aarch64，包含 timeval ABI。
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (&event as *const 输入事件).cast::<u8>(),
            std::mem::size_of::<输入事件>(),
        )
    };
    file.write_all(bytes)?;
    Ok(())
}
fn 执行() -> Result<()> {
    let args = 虚拟键盘参数::parse();
    ensure!(unsafe { libc::geteuid() } != 0, "不要用 root/sudo 运行");
    ensure!(
        args.名称.len() < 80 && !args.名称.contains('\0'),
        "设备名太长或包含 NUL"
    );
    ensure!(
        args.间隔秒数.is_finite()
            && args.间隔秒数 >= 0.0
            && args.存活秒数.is_finite()
            && args.存活秒数 >= 0.0,
        "延迟必须为有限非负数"
    );
    let events = 解析事件(&args.事件)?;
    // 显式的测试设备 I/O；安装过程不会调用。
    let file = OpenOptions::new().write(true).open("/dev/uinput")?;
    let fd = file.as_raw_fd();
    for kind in [1, 0, 20] {
        调用设备控制(fd, 0x40045564, kind)?;
    }
    for code in events
        .iter()
        .map(|e| e.0)
        .collect::<std::collections::BTreeSet<_>>()
    {
        调用设备控制(fd, 0x40045565, code.into())?;
    }
    let mut setup = 键盘设置 {
        id: [3, 0x1234, 0x5678, 1],
        名称: [0; 80],
        ff_effects_max: 0,
    };
    setup.名称[..args.名称.len()].copy_from_slice(args.名称.as_bytes());
    // UI_DEV_SETUP 接受指向 92 字节原生 uinput_setup 结构的指针。
    let rc = unsafe { libc::ioctl(fd, 0x405c5503 as libc::c_ulong, &setup) };
    ensure!(
        rc >= 0,
        "UI_DEV_SETUP failed: {}",
        io::Error::last_os_error()
    );
    调用设备控制(fd, 0x5501, 0)?;
    let mut device = 虚拟设备(file);
    println!("READY {}", args.名称);
    io::stdout().flush()?;
    if let Some(path) = args.等待文件 {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !path.exists() {
            ensure!(
                std::time::Instant::now() < deadline,
                "TIMEOUT waiting for go-file"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    for (code, value) in events {
        发送事件(&mut device.0, 1, code, value)?;
        发送事件(&mut device.0, 0, 0, 0)?;
        std::thread::sleep(Duration::from_secs_f64(args.间隔秒数));
    }
    println!("SENT");
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_secs_f64(args.存活秒数));
    Ok(())
}
fn main() {
    if let Err(err) = 执行() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 验证键码与内核布局() {
        assert_eq!(
            解析事件("LEFTMETA:1,C:1,C:0,LEFTMETA:0").unwrap(),
            vec![(125, 1), (46, 1), (46, 0), (125, 0)]
        );
        assert!(解析事件("BAD:1").is_err());
        assert!(解析事件("C:3").is_err());
        assert_eq!(std::mem::size_of::<键盘设置>(), 92);
        assert_eq!(std::mem::size_of::<输入事件>(), 24);
    }
}
