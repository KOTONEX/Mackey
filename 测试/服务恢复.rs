// SPDX-License-Identifier: AGPL-3.0-or-later
// 显式运行：cargo test --locked --test 服务恢复 -- --ignored --nocapture
// 使用真实用户 systemd 和隔离服务名，不打开输入设备、不停止用户键盘服务。
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn 控制(参数: &[&str]) -> String {
    let 输出 = Command::new("systemctl")
        .arg("--user")
        .args(参数)
        .output()
        .unwrap();
    assert!(
        输出.status.success(),
        "systemctl {参数:?}: {}",
        String::from_utf8_lossy(&输出.stderr)
    );
    String::from_utf8(输出.stdout).unwrap().trim().to_owned()
}
fn 等待(说明: &str, 条件: impl Fn() -> bool) {
    let 截止 = Instant::now() + Duration::from_secs(20);
    while Instant::now() < 截止 {
        if 条件() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("等待超时：{说明}");
}
struct 隔离服务 {
    名称: [String; 2],
}
impl Drop for 隔离服务 {
    fn drop(&mut self) {
        for 参数 in [
            vec!["stop", &self.名称[0], &self.名称[1]],
            vec!["disable", &self.名称[0], &self.名称[1]],
            vec!["reset-failed", &self.名称[0], &self.名称[1]],
            vec!["daemon-reload"],
        ] {
            let _ = Command::new("systemctl").arg("--user").args(参数).output();
        }
    }
}
fn 写入(路径: &Path, 内容: impl AsRef<[u8]>) {
    fs::create_dir_all(路径.parent().unwrap()).unwrap();
    fs::write(路径, 内容).unwrap();
}

#[test]
#[ignore = "需要正在运行的用户 systemd；按需显式执行，使用独立服务及焦点套接字"]
fn 登录来源延迟及焦点桥故障后引擎自动恢复() {
    let 临时 = mackey::开发::临时目录().unwrap();
    let 家 = 临时.path().join("home");
    let 桩目录 = 临时.path().join("stubs");
    fs::create_dir_all(&桩目录).unwrap();
    for 名称 in ["systemctl", "gsettings", "gnome-extensions"] {
        symlink(env!("CARGO_BIN_EXE_mackey-测试桩"), 桩目录.join(名称)).unwrap();
    }
    let 假引擎 = 家.join(".local/share/mackey/bin/xremap");
    写入(&假引擎, b"test");
    fs::set_permissions(&假引擎, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir_all(家.join("run")).unwrap();
    let mut 命令 = Command::new(env!("CARGO_BIN_EXE_mackey"));
    命令
        .args(["安装", "--不下载"])
        .env("HOME", &家)
        .env("XDG_CURRENT_DESKTOP", "GNOME")
        .env(
            "PATH",
            format!("{}:{}", 桩目录.display(), std::env::var("PATH").unwrap()),
        )
        .env_remove("MACKEY_ENGINE")
        .env_remove("MACKEY_TEST_LOG")
        .env_remove("MACKEY_TEST_SEED")
        .env_remove("MACKEY_TEST_FAIL_RESTORE")
        .env_remove("MACKEY_TEST_FAIL_ONCE");
    for (键, 值) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_STATE_HOME", ".local/state"),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_BIN_HOME", ".local/bin"),
        ("XDG_RUNTIME_DIR", "run"),
    ] {
        命令.env(键, 家.join(值));
    }
    命令.env("MACKEY_DOCS", 家.join("行为清单.md"));
    let 输出 = 命令.output().unwrap();
    assert!(
        输出.status.success(),
        "{}",
        String::from_utf8_lossy(&输出.stderr)
    );
    let 编号 = std::process::id();
    let 服务 = 隔离服务 {
        名称: [
            format!("mackey-test-recovery-{编号}-focus.service"),
            format!("mackey-test-recovery-{编号}-engine.service"),
        ],
    };
    let 单元目录 = 家.join(".config/systemd/user");
    let 重命名 = |文本: String| {
        文本
            .replace("mackey-focusd.service", &服务.名称[0])
            .replace("mackey-engine.service", &服务.名称[1])
    };
    let 模板 = 重命名(fs::read_to_string(单元目录.join("mackey-focusd.service")).unwrap());
    let 焦点文件 = 单元目录.join(&服务.名称[0]);
    let 写焦点 = |可用: bool| {
        let 内容 = 模板
            .lines()
            .map(|行| {
                if 行.starts_with("ExecStart=") {
                    if 可用 {
                        format!("{行} --后端 固定 --固定应用 mackey-test-ready")
                    } else {
                        format!(
                            "ExecStart=env \"DBUS_SESSION_BUS_ADDRESS=unix:path={}/不存在总线\" {}",
                            临时.path().display(),
                            行.trim_start_matches("ExecStart=")
                        )
                    }
                } else {
                    行.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        写入(&焦点文件, 内容);
    };
    写焦点(false);
    let 记录 = 临时.path().join("引擎就绪记录");
    let 引擎文件 = 单元目录.join(&服务.名称[1]);
    let 内容 = 重命名(fs::read_to_string(单元目录.join("mackey-engine.service")).unwrap())
        .replace("Type=simple", "Type=oneshot\nRemainAfterExit=yes")
        .lines()
        .map(|行| {
            if 行.starts_with("ExecStart=") {
                format!(
                    "ExecStart=\"{}\" 焦点就绪探针 \"{}\"",
                    env!("CARGO_BIN_EXE_mackey-测试桩"),
                    记录.display()
                )
            } else {
                行.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    写入(&引擎文件, 内容);
    控制(&[
        "link",
        焦点文件.to_str().unwrap(),
        引擎文件.to_str().unwrap(),
    ]);
    控制(&["daemon-reload"]);
    let 开始 = Instant::now();
    控制(&["start", "--no-block", &服务.名称[0], &服务.名称[1]]);
    等待("缺少 GNOME 来源时持续重试", || {
        控制(&["show", "--property=NRestarts", "--value", &服务.名称[0]])
            .parse::<u64>()
            .unwrap_or(0)
            >= 6
    });
    assert!(
        开始.elapsed() >= Duration::from_secs(10),
        "重试必须遵守两秒间隔，不能因依赖关系高速循环"
    );
    assert!(!记录.exists(), "来源不可用期间引擎不应启动");
    写焦点(true);
    控制(&["daemon-reload"]);
    let 次数 = || {
        fs::read_to_string(&记录)
            .unwrap_or_default()
            .lines()
            .count()
    };
    等待("来源就绪后自动启动引擎", || {
        次数() == 1
            && 控制(&["show", "--property=ActiveState", "--value", &服务.名称[1]]) == "active"
    });
    写焦点(false);
    控制(&["daemon-reload"]);
    控制(&["kill", "--signal=KILL", &服务.名称[0]]);
    等待("桥接故障自动停用引擎", || {
        控制(&["show", "--property=ActiveState", "--value", &服务.名称[1]]) == "inactive"
    });
    assert_eq!(次数(), 1);
    写焦点(true);
    控制(&["daemon-reload"]);
    等待("桥接恢复自动重新启动引擎", || {
        次数() == 2
            && 控制(&["show", "--property=ActiveState", "--value", &服务.名称[1]]) == "active"
    });
    控制(&["stop", &服务.名称[0]]);
    等待("显式停用保持引擎关闭", || {
        控制(&["show", "--property=ActiveState", "--value", &服务.名称[1]]) == "inactive"
    });
    std::thread::sleep(Duration::from_millis(2200));
    assert_eq!(次数(), 2);
    assert_eq!(
        控制(&["show", "--property=ActiveState", "--value", &服务.名称[0]]),
        "inactive"
    );
    println!("✓ 延迟来源、超启动限流窗口、故障联停、自动恢复和显式停用均通过（无键盘事件）");
}
