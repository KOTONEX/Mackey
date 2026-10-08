// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::路径集合;
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const 后端列表: [(&str, &str, &str, &str); 2] = [
    (
        "k0kubun",
        "/com/k0kubun/Xremap",
        "com.k0kubun.Xremap",
        "ActiveWindow",
    ),
    (
        "focused-window",
        "/org/gnome/shell/extensions/FocusedWindow",
        "org.gnome.shell.extensions.FocusedWindow",
        "Get",
    ),
];
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct 焦点信息 {
    #[serde(default)]
    #[serde(rename = "wm_class")]
    pub 应用标识: String,
    #[serde(default)]
    #[serde(rename = "title")]
    pub 标题: String,
}
#[derive(Debug, Args)]
pub struct 选项 {
    #[arg(long,default_value="自动",value_parser=["自动","固定","k0kubun","focused-window"])]
    pub 后端: String,
    #[arg(long)]
    pub 套接字: Option<PathBuf>,
    #[arg(long, default_value_t = 40)]
    pub 缓存毫秒: u64,
    #[arg(long, default_value = "mackey-e2e-static")]
    pub 固定应用: String,
    #[arg(long)]
    pub 测试: bool,
    #[arg(long)]
    pub 列出后端: bool,
}
pub struct 焦点来源 {
    后端: String,
    connection: Option<zbus::blocking::Connection>,
    固定应用: String,
    缓存毫秒: u64,
    缓存: Option<(Instant, 焦点信息)>,
}
impl 焦点来源 {
    pub fn 连接(preferred: &str, 缓存毫秒: u64, 固定应用: &str) -> Result<Self> {
        let mut src = Self {
            后端: preferred.into(),
            connection: None,
            固定应用: 固定应用.into(),
            缓存毫秒,
            缓存: None,
        };
        if preferred == "固定" {
            return Ok(src);
        }
        src.connection = Some(
            zbus::blocking::connection::Builder::session()?
                .method_timeout(Duration::from_secs(2))
                .build()?,
        );
        for (name, _, _, _) in 后端列表 {
            if preferred != "自动" && preferred != name {
                continue;
            }
            src.后端 = name.into();
            match src.查询() {
                Ok(info) => {
                    eprintln!("[focusd] 后端 {name} 可用（当前焦点：{}）", info.应用标识);
                    src.缓存 = Some((Instant::now(), info));
                    return Ok(src);
                }
                Err(err) => eprintln!("[focusd] 后端 {name} 不可用：{err}"),
            }
        }
        bail!("没有可用的焦点来源；请安装焦点扩展并重新登录")
    }
    fn 查询(&self) -> Result<焦点信息> {
        if self.后端 == "固定" {
            return Ok(焦点信息 {
                应用标识: self.固定应用.clone(),
                标题: "固定".into(),
            });
        }
        let (_, path, iface, method) = 后端列表
            .iter()
            .find(|b| b.0 == self.后端)
            .context("未知焦点后端")?;
        let proxy = zbus::blocking::Proxy::new(
            self.connection.as_ref().context("D-Bus 未连接")?,
            "org.gnome.Shell",
            *path,
            *iface,
        )?;
        let payload: String = proxy.call(*method, &())?;
        let v: Value = serde_json::from_str(&payload)?;
        ensure!(v.is_object(), "焦点响应必须为对象");
        Ok(焦点信息 {
            应用标识: v["wm_class"].as_str().unwrap_or("").to_owned(),
            标题: v["title"].as_str().unwrap_or("").to_owned(),
        })
    }
    pub fn 获取(&mut self) -> Result<焦点信息> {
        if let Some((time, info)) = &self.缓存
            && time.elapsed() < Duration::from_millis(self.缓存毫秒)
        {
            return Ok(info.clone());
        }
        match self.查询() {
            Ok(info) => {
                self.缓存 = Some((Instant::now(), info.clone()));
                Ok(info)
            }
            Err(err) => {
                if let Some((_, last)) = &self.缓存 {
                    eprintln!("[focusd] 取焦点失败，沿用最后一次结果：{err}");
                    Ok(last.clone())
                } else {
                    Err(err)
                }
            }
        }
    }
}
pub fn 验证焦点(preferred: &str) -> Result<焦点信息> {
    焦点来源::连接(preferred, 0, "mackey-e2e-static")?.获取()
}
pub fn 响应(request: &Value, source: &mut 焦点来源) -> Result<Value> {
    if request == "ActiveWindow"
        || request
            .as_object()
            .is_some_and(|o| o.contains_key("ActiveWindow"))
    {
        return Ok(serde_json::to_value(source.获取()?)?);
    }
    if let Some(执行) = request.get("Run") {
        let args = 执行.as_array().context("Run 必须是字符串数组")?;
        let args: Vec<&str> = args
            .iter()
            .map(|v| v.as_str().context("Run 参数必须是字符串"))
            .collect::<Result<_>>()?;
        let (program, args) = args.split_first().context("Run 不能为空")?;
        let mut child = std::process::Command::new(program).args(args).spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(json!("Ok"));
    }
    bail!("不支持的 socket 请求")
}
fn 处理请求(mut stream: UnixStream, source: &Mutex<焦点来源>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    let mut reader = BufReader::new((&stream).take(65537));
    reader.read_line(&mut line)?;
    ensure!(
        line.len() <= 65536 && line.ends_with('\n'),
        "无效或过长的 socket 请求"
    );
    let request: Value = serde_json::from_str(line.trim())?;
    let mut locked = source.lock().map_err(|_| anyhow::anyhow!("焦点锁损坏"))?;
    let value = 响应(&request, &mut locked)?;
    writeln!(stream, "{}", serde_json::to_string(&value)?)?;
    Ok(())
}
struct 套接字清理 {
    path: PathBuf,
    ino: u64,
}
impl Drop for 套接字清理 {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.ino) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub fn 执行(路径: &路径集合, options: &选项) -> Result<()> {
    if options.列出后端 {
        for (name, path, iface, method) in 后端列表 {
            println!("{name:16} org.gnome.Shell {path} {iface}.{method}");
        }
        println!("固定             测试专用");
        return Ok(());
    }
    let source = 焦点来源::连接(&options.后端, options.缓存毫秒, &options.固定应用)?;
    if options.测试 {
        println!("{}", serde_json::to_string_pretty(&source.查询()?)?);
        return Ok(());
    }
    let path = options.套接字.clone().unwrap_or(路径.套接字.clone());
    路径.保护套接字(&path)?;
    if let Ok(元信息) = fs::symlink_metadata(&path) {
        ensure!(
            元信息.file_type().is_socket(),
            "拒绝替换非 socket 文件：{}",
            path.display()
        );
        ensure!(
            UnixStream::connect(&path).is_err(),
            "{} 已有 focusd 在监听",
            path.display()
        );
        fs::remove_file(&path)?;
    }
    // 运行时目录由会话提供；不创建系统目录。
    let parent = path.parent().context("socket 没有父目录")?;
    if !parent.exists() {
        路径.保护(parent)?;
        fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(&path)?;
    let _cleanup = 套接字清理 {
        path: path.clone(),
        ino: fs::symlink_metadata(&path)?.ino(),
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let source = Arc::new(Mutex::new(source));
    let 运行中 = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    eprintln!("[focusd] 监听 {}（后端 {}）", path.display(), options.后端);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if 运行中.load(Ordering::Relaxed) >= 32 {
                    continue;
                }
                运行中.fetch_add(1, Ordering::Relaxed);
                let 运行中 = 运行中.clone();
                let source = source.clone();
                std::thread::spawn(move || {
                    if let Err(err) = 处理请求(stream, &source) {
                        eprintln!("[focusd] 请求失败：{err}");
                    }
                    运行中.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 验证套接字协议与非法命令() {
        let mut source = 焦点来源::连接("固定", 40, "org.gnome.Terminal").unwrap();
        assert_eq!(
            响应(&json!("ActiveWindow"), &mut source).unwrap()["wm_class"],
            "org.gnome.Terminal"
        );
        assert_eq!(
            响应(&json!({"ActiveWindow":null}), &mut source).unwrap()["title"],
            "固定"
        );
        assert!(响应(&json!({"Run":"bad"}), &mut source).is_err());
        assert!(响应(&json!({"Run":[]}), &mut source).is_err());
        assert!(响应(&json!("unknown"), &mut source).is_err());
    }
}
