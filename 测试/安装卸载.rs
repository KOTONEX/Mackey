// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};
const 程序: &str = env!("CARGO_BIN_EXE_mackey");
const 桩: &str = env!("CARGO_BIN_EXE_mackey-测试桩");
struct 沙箱 {
    临时: tempfile::TempDir,
    主目录: PathBuf,
}
fn 写入(路径: impl AsRef<Path>, 内容: impl AsRef<[u8]>) {
    let 路径 = 路径.as_ref();
    fs::create_dir_all(路径.parent().unwrap()).unwrap();
    fs::write(路径, 内容).unwrap();
}
impl 沙箱 {
    fn 新建() -> Self {
        let 临时 = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let 主目录 = 临时.path().join("home");
        fs::create_dir_all(&主目录).unwrap();
        let 桩目录 = 临时.path().join("stubs");
        fs::create_dir(&桩目录).unwrap();
        for 名称 in ["systemctl", "gsettings", "gnome-extensions"] {
            symlink(桩, 桩目录.join(名称)).unwrap();
        }
        Self { 临时, 主目录 }
    }
    fn 路径(&self, 相对: &str) -> PathBuf {
        self.主目录.join(相对)
    }
    fn 命令(&self) -> Command {
        let mut 命令 = Command::new(程序);
        命令
            .env("HOME", &self.主目录)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.临时.path().join("stubs").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("XDG_CONFIG_HOME", self.路径(".config"))
            .env("XDG_DATA_HOME", self.路径(".local/share"))
            .env("XDG_CACHE_HOME", self.路径(".cache"))
            .env("XDG_STATE_HOME", self.路径(".local/state"))
            .env("XDG_BIN_HOME", self.路径(".local/bin"))
            .env("XDG_RUNTIME_DIR", self.路径("run"))
            .env("MACKEY_DOCS", self.路径("文档/行为清单.md"))
            .env("XDG_CURRENT_DESKTOP", "GNOME")
            .env("XDG_SESSION_TYPE", "wayland")
            .env("MACKEY_TEST_LOG", self.临时.path().join("调用.jsonl"))
            .env("MACKEY_TEST_SEED", self.临时.path().join("种子.json"))
            .env_remove("MACKEY_ENGINE")
            .env_remove("MACKEY_TEST_FAIL_RESTORE")
            .env_remove("MACKEY_YES");
        命令
    }
    fn 执行(&self, 参数: &[&str]) -> Output {
        let 输出 = self.命令().args(参数).output().unwrap();
        assert!(
            输出.status.success(),
            "{}",
            String::from_utf8_lossy(&输出.stderr)
        );
        输出
    }
    fn 调用(&self) -> Vec<Value> {
        fs::read_to_string(self.临时.path().join("调用.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|行| serde_json::from_str(行).unwrap())
            .collect()
    }
    fn 种子(&self) {
        写入(self.临时.path().join("种子.json"),json!({"org.gnome.shell enabled-extensions":format!("['{}', '{}', 'other@example.com']",mackey::扩展标识,mackey::旧扩展标识)}).to_string());
    }
    fn 安装(&self) {
        写入(self.路径(".local/share/mackey/bin/xremap"), b"OLD-ENGINE");
        fs::set_permissions(
            self.路径(".local/share/mackey/bin/xremap"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::create_dir_all(self.路径("run")).unwrap();
        self.执行(&["安装", "--不下载"]);
    }
}
fn 快照(根: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn 遍历(根: &Path, 当前: &Path, 结果: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        for 条目 in fs::read_dir(当前).unwrap() {
            let 路径 = 条目.unwrap().path();
            let 属性 = fs::symlink_metadata(&路径).unwrap();
            if 属性.is_dir() {
                遍历(根, &路径, 结果);
            } else {
                let 内容 = if 属性.file_type().is_symlink() {
                    fs::read_link(&路径)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    fs::read(&路径).unwrap()
                };
                结果.insert(
                    路径.strip_prefix(根).unwrap().into(),
                    (属性.permissions().mode() & 0o777, 内容),
                );
            }
        }
    }
    let mut 结果 = BTreeMap::new();
    遍历(根, 根, &mut 结果);
    结果
}
#[test]
fn 所有安装入口预检失败时零写入() {
    for 入口 in ["原生命令", "脚本"] {
        for (桌面, 会话, 提示) in [
            ("KDE", "wayland", "仅支持 GNOME"),
            ("KDE", "x11", "仅支持 GNOME"),
        ] {
            let 沙箱 = 沙箱::新建();
            let mut 命令 = 沙箱.命令();
            if 入口 == "脚本" {
                // 只验证保留的最薄入口转发；实际业务逻辑均由 Rust 执行。
                命令 = Command::new("bash");
                let 环境 = 沙箱.命令();
                命令.envs(环境.get_envs().filter_map(|(键, 值)| 值.map(|值| (键, 值))));
                for (键, 值) in 环境.get_envs() {
                    if 值.is_none() {
                        命令.env_remove(键);
                    }
                }
                命令.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("安装.sh"));
            } else {
                命令.arg("安装");
            }
            let 输出 = 命令
                .arg("--不下载")
                .env("XDG_CURRENT_DESKTOP", 桌面)
                .env("XDG_SESSION_TYPE", 会话)
                .output()
                .unwrap();
            assert!(!输出.status.success());
            assert!(String::from_utf8_lossy(&输出.stderr).contains(提示));
            assert!(快照(&沙箱.主目录).is_empty());
            assert!(沙箱.调用().is_empty());
        }
    }
}
#[test]
fn 首次安装及仅有焦点服务时不禁用缺失单元() {
    for 有焦点服务 in [false, true] {
        let 沙箱 = 沙箱::新建();
        if 有焦点服务 {
            写入(
                沙箱.路径(".config/systemd/user/mackey-focusd.service"),
                "OLD",
            );
        }
        沙箱.安装();
        let 禁用: Vec<_> = 沙箱
            .调用()
            .into_iter()
            .filter(|调用| 调用["程序"] == "systemctl" && 调用["参数"][1] == "disable")
            .collect();
        assert_eq!(禁用.len(), usize::from(有焦点服务));
        if 有焦点服务 {
            assert_eq!(
                禁用[0]["参数"],
                json!(["--user", "disable", "--now", "mackey-focusd.service"])
            );
        }
        assert!(
            沙箱
                .路径(".config/systemd/user/mackey-engine.service")
                .is_file()
        );
        assert!(
            沙箱
                .路径(".config/systemd/user/mackey-focusd.service")
                .is_file()
        );
    }
}
#[test]
fn 单次原生安装收敛缺失字段和陈旧产物() {
    let 沙箱 = 沙箱::新建();
    沙箱.安装();
    let 扩展目录 = 沙箱.路径(&format!(
        ".local/share/gnome-shell/extensions/{}",
        mackey::扩展标识
    ));
    assert_eq!(
        fs::read(扩展目录.join("extension.js")).unwrap(),
        mackey::扩展脚本
    );
    let mut 文件: Vec<_> = fs::read_dir(&扩展目录)
        .unwrap()
        .map(|项| 项.unwrap().file_name())
        .collect();
    文件.sort();
    assert_eq!(
        文件,
        vec![
            std::ffi::OsString::from("extension.js"),
            std::ffi::OsString::from("metadata.json")
        ]
    );
    let 基准 = 快照(&沙箱.主目录);
    let 配置路径 = 沙箱.路径(".config/mackey/config.json");
    let mut 配置: Value = serde_json::from_slice(&fs::read(&配置路径).unwrap()).unwrap();
    for 键 in [
        "keypress_delay_ms",
        "_keypress_delay_ms_说明",
        "引擎",
        "_引擎_说明",
    ] {
        配置.as_object_mut().unwrap().remove(键);
    }
    写入(&配置路径, serde_json::to_vec_pretty(&配置).unwrap());
    for 名称 in ["mackey-engine.service", "mackey-focusd.service"] {
        写入(沙箱.路径(&format!(".config/systemd/user/{名称}")), "STALE");
        let 链接 = 沙箱.路径(&format!(".config/systemd/user/default.target.wants/{名称}"));
        fs::create_dir_all(链接.parent().unwrap()).unwrap();
        symlink(format!("../{名称}"), 链接).unwrap();
    }
    写入(沙箱.路径("run/mackey-focus.sock"), "");
    写入(
        沙箱.路径(&format!(
            ".local/share/gnome-shell/extensions/{}/stale.js",
            mackey::扩展标识
        )),
        "stale",
    );
    写入(
        沙箱.路径(&format!(
            ".local/share/gnome-shell/extensions/{}/extension.js",
            mackey::旧扩展标识
        )),
        "old",
    );
    沙箱.执行(&["安装", "--不下载"]);
    assert_eq!(快照(&沙箱.主目录), 基准);
    let 单元 = fs::read_to_string(沙箱.路径(".config/systemd/user/mackey-engine.service")).unwrap();
    assert!(
        单元.contains(
            沙箱
                .路径(".local/share/mackey/bin/xremap")
                .to_str()
                .unwrap()
        )
    );
    assert!(!单元.contains("/.vendor/"));
}
#[test]
fn 损坏配置安装时原文备份并重建() {
    let 沙箱 = 沙箱::新建();
    沙箱.安装();
    let 配置 = 沙箱.路径(".config/mackey/config.json");
    let 原配置 = fs::read(&配置).unwrap();
    写入(&配置, "{ not json");
    沙箱.执行(&["安装", "--不下载"]);
    assert_eq!(fs::read(配置).unwrap(), 原配置);
    let 备份: Vec<_> = fs::read_dir(沙箱.路径(".config/mackey"))
        .unwrap()
        .map(|条目| 条目.unwrap().path())
        .filter(|路径| {
            路径
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("config.json.bak-")
        })
        .collect();
    assert_eq!(备份.len(), 1);
    assert_eq!(fs::read(&备份[0]).unwrap(), b"{ not json");
}
#[test]
fn 旧配置安装失败且原文与产物不变() {
    for 旧配置 in [
        r#"{"modifier_layout":"pc-swap","engine":""}"#,
        r#"{"修饰键布局":"电脑换位"}"#,
    ] {
        let 沙箱 = 沙箱::新建();
        写入(沙箱.路径(".config/mackey/config.json"), 旧配置);
        let 原状 = 快照(&沙箱.主目录);
        let 输出 = 沙箱.命令().args(["安装", "--不下载"]).output().unwrap();
        assert!(!输出.status.success());
        assert!(String::from_utf8_lossy(&输出.stderr).contains("卸载"));
        assert_eq!(快照(&沙箱.主目录), 原状);
        assert!(沙箱.调用().is_empty());
    }
}
#[test]
fn 安装保留用户字段且保持运行选项不停止服务() {
    let 沙箱 = 沙箱::新建();
    写入(
        沙箱.路径(".config/mackey/config.json"),
        json!({"修饰键布局":"微软","device":{"only":["testkbd"]},"个人字段":"保留"}).to_string(),
    );
    沙箱.执行(&["安装", "--不下载", "--保持运行"]);
    let 配置: Value =
        serde_json::from_slice(&fs::read(沙箱.路径(".config/mackey/config.json")).unwrap())
            .unwrap();
    assert_eq!(配置["个人字段"], "保留");
    assert_eq!(配置["device"]["only"], json!(["testkbd"]));
    assert_eq!(配置["修饰键布局"], "微软");
    assert!(!沙箱.调用().iter().any(|调用| {
        调用["参数"]
            .as_array()
            .unwrap()
            .iter()
            .any(|值| 值 == "--now")
    }));
}
#[test]
fn 默认卸载预演零变更并还原全部备份() {
    let 沙箱 = 沙箱::新建();
    沙箱.安装();
    沙箱.种子();
    let 备份 = json!({"org.gnome.desktop.wm.keybindings switch-applications":["<Super>Tab"],"org.gnome.shell.keybindings toggle-overview":["<Super>s"]});
    写入(
        沙箱.路径(".config/mackey/backup/gsettings.json"),
        备份.to_string(),
    );
    写入(沙箱.路径(".cache/mackey/downloads/xremap.zip"), "zip");
    写入(沙箱.路径(".local/state/mackey/engine.json"), "{}");
    写入(沙箱.路径("run/mackey-focus.sock"), "");
    写入(
        沙箱.路径(&format!(
            ".local/share/gnome-shell/extensions/{}/extension.js",
            mackey::旧扩展标识
        )),
        "old",
    );
    let 原状 = 快照(&沙箱.主目录);
    let 调用数 = 沙箱.调用().len();
    沙箱.执行(&["卸载", "--预演"]);
    assert_eq!(快照(&沙箱.主目录), 原状);
    assert_eq!(沙箱.调用().len(), 调用数);
    沙箱.执行(&["卸载", "--确认执行"]);
    for 目标 in [
        ".config/mackey",
        ".local/share/mackey",
        ".cache/mackey",
        ".local/state/mackey",
        ".local/bin/mackey",
        "run/mackey-focus.sock",
        ".config/systemd/user/mackey-engine.service",
        ".config/systemd/user/mackey-focusd.service",
    ] {
        assert!(!沙箱.路径(目标).exists(), "{目标}");
    }
    for 扩展 in [mackey::扩展标识, mackey::旧扩展标识] {
        assert!(
            !沙箱
                .路径(&format!(".local/share/gnome-shell/extensions/{扩展}"))
                .exists()
        );
    }
    let 调用 = 沙箱.调用();
    for (键, 值) in 备份.as_object().unwrap() {
        let (方案, 键) = 键.split_once(' ').unwrap();
        let 值 = mackey::配置生成::转为加速键列表(&mackey::配置生成::字符串列表(值));
        assert!(调用.iter().any(
            |调用| 调用["程序"] == "gsettings" && 调用["参数"] == json!(["set", 方案, 键, 值])
        ));
    }
    let 种子: Value =
        serde_json::from_slice(&fs::read(沙箱.临时.path().join("种子.json")).unwrap()).unwrap();
    assert_eq!(
        种子["org.gnome.shell enabled-extensions"],
        "['other@example.com']"
    );
}
#[test]
fn 卸载还原失败保留原始配置和备份() {
    let 沙箱 = 沙箱::新建();
    沙箱.安装();
    let 备份 = 沙箱.路径(".config/mackey/backup/gsettings.json");
    写入(
        &备份,
        r#"{"org.gnome.desktop.wm.keybindings switch-applications":["<Super>Tab"]}"#,
    );
    let 原文 = fs::read(&备份).unwrap();
    let 输出 = 沙箱
        .命令()
        .args(["卸载", "--确认执行"])
        .env("MACKEY_TEST_FAIL_RESTORE", "1")
        .output()
        .unwrap();
    assert!(!输出.status.success());
    assert_eq!(fs::read(备份).unwrap(), 原文);
    assert!(沙箱.路径(".config/mackey/config.json").exists());
}
#[test]
fn 卸载可显式保留配置() {
    let 沙箱 = 沙箱::新建();
    沙箱.安装();
    let 配置 = 沙箱.路径(".config/mackey/config.json");
    let 原文 = fs::read(&配置).unwrap();
    沙箱.执行(&["卸载", "--保留配置", "--确认执行"]);
    assert_eq!(fs::read(配置).unwrap(), 原文);
    assert!(!沙箱.路径(".local/share/mackey").exists());
}

#[test]
fn 会话类型环境变量不触发安装拦截() {
    let 沙箱 = 沙箱::新建();
    let 输出 = 沙箱
        .命令()
        .args(["安装", "--不下载"])
        .env("XDG_SESSION_TYPE", "x11")
        .output()
        .unwrap();
    assert!(
        输出.status.success(),
        "{}",
        String::from_utf8_lossy(&输出.stderr)
    );
    assert!(沙箱.路径(".config/mackey/config.json").exists());
}
