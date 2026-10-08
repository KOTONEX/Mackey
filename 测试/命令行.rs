// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const 测试程序: &str = env!("CARGO_BIN_EXE_mackey");
fn 主目录() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap()
}
fn 执行命令(主目录: &Path) -> Command {
    let mut c = Command::new(测试程序);
    c.env("HOME", 主目录)
        .env("XDG_CONFIG_HOME", 主目录.join(".config"))
        .env("XDG_DATA_HOME", 主目录.join(".local/share"))
        .env("XDG_CACHE_HOME", 主目录.join(".cache"))
        .env("XDG_STATE_HOME", 主目录.join(".local/state"))
        .env("XDG_BIN_HOME", 主目录.join(".local/bin"))
        .env("XDG_RUNTIME_DIR", 主目录.join("run"));
    c
}
struct 测试服务(Child);
impl Drop for 测试服务 {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn 启动服务(主目录: &Path, 套接字: &Path) -> 测试服务 {
    let child = 执行命令(主目录)
        .args([
            "焦点桥",
            "--后端",
            "固定",
            "--固定应用",
            "org.gnome.Terminal",
            "--套接字",
        ])
        .arg(套接字)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut server = 测试服务(child);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !套接字.exists() {
        assert!(server.0.try_wait().unwrap().is_none(), "server exited");
        assert!(Instant::now() < deadline, "socket timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    server
}
fn 请求(套接字: &Path, value: &Value) -> Value {
    let mut stream = UnixStream::connect(套接字).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    writeln!(stream, "{value}").unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
#[test]
fn 离线生成配置与只读报告() {
    let temp = 主目录();
    let out = temp.path().join("generated");
    let doc = temp.path().join("behavior.md");
    let result = 执行命令(temp.path())
        .args(["生成", "--不探测", "--输出目录"])
        .arg(&out)
        .arg("--文档")
        .arg(&doc)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let 配置: Value = serde_json::from_slice(&fs::read(out.join("xremap.json")).unwrap()).unwrap();
    assert_eq!(
        配置,
        serde_json::from_str::<Value>(include_str!("基准/xremap.json")).unwrap()
    );
    let 文档 = fs::read_to_string(doc).unwrap();
    assert!(文档.contains("SIGINT"));
    assert!(文档.contains("## 七、"));
    let 报告 = temp.path().join("read-only");
    let result = 执行命令(temp.path())
        .args(["生成", "--不探测", "--报告", "--输出目录"])
        .arg(&报告)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(!报告.exists());
}
#[test]
fn 创建产物前拒绝主目录外写入() {
    let temp = 主目录();
    let result = 执行命令(temp.path())
        .args([
            "生成",
            "--不探测",
            "--输出目录",
            "/tmp/mackey-disallowed-test",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("不在 $HOME"));
    assert!(!temp.path().join(".config").exists());
}
#[test]
fn 离线下载计划不安装文件() {
    let temp = 主目录();
    let result = 执行命令(temp.path())
        .args([
            "获取引擎",
            "--标签",
            "v0.15.0",
            "--架构",
            "arm64",
            "--输出计划",
        ])
        .env_remove("XDG_SESSION_TYPE")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let 迁移计划: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(迁移计划["架构"], "aarch64");
    assert_eq!(迁移计划["候选地址"].as_array().unwrap().len(), 2);
    assert!(!temp.path().join(".local").exists());
}
#[test]
fn 真实套接字协议与正常退出() {
    let temp = 主目录();
    let 套接字 = temp.path().join("focus.sock");
    let mut server = 启动服务(temp.path(), &套接字);
    for value in [json!("ActiveWindow"), json!({"ActiveWindow":null})] {
        assert_eq!(请求(&套接字, &value)["wm_class"], "org.gnome.Terminal");
    }
    assert_eq!(请求(&套接字, &json!({"Run":["/usr/bin/true"]})), "Ok");
    let 元信息 = fs::metadata(&套接字).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(元信息.permissions().mode() & 0o777, 0o600);
    let duplicate = 执行命令(temp.path())
        .args(["焦点桥", "--后端", "固定", "--套接字"])
        .arg(&套接字)
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(套接字.exists());
    assert_eq!(请求(&套接字, &json!("ActiveWindow"))["title"], "固定");
    // SIGTERM 是 systemd 常规的服务停止信号。
    unsafe {
        libc::kill(server.0.id() as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while server.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!套接字.exists());
}
#[test]
fn 套接字不替换普通文件() {
    let temp = 主目录();
    let 套接字 = temp.path().join("keep.txt");
    fs::write(&套接字, "keep").unwrap();
    let result = 执行命令(temp.path())
        .args(["焦点桥", "--后端", "固定", "--套接字"])
        .arg(&套接字)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(套接字).unwrap(), "keep");
}
