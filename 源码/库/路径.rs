// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Context, Result, ensure};
use std::{
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct 路径集合 {
    pub 主目录: PathBuf,
    pub 配置: PathBuf,
    pub 数据: PathBuf,
    pub 缓存: PathBuf,
    pub 状态目录: PathBuf,
    pub 服务目录: PathBuf,
    pub 命令入口: PathBuf,
    pub 套接字: PathBuf,
}
fn 环境目录(key: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}
impl 路径集合 {
    pub fn 发现() -> Result<Self> {
        let 主目录 = PathBuf::from(env::var_os("HOME").context("HOME 未设置")?);
        ensure!(主目录.is_absolute(), "HOME 必须是绝对路径");
        let 配置 = 环境目录("XDG_CONFIG_HOME", 主目录.join(".config"));
        let 数据 = 环境目录("XDG_DATA_HOME", 主目录.join(".local/share"));
        // Unix 套接字是瞬时 IPC；默认运行时目录是唯一允许的 HOME 外写入点，
        // 并且仅限本项目的固定套接字文件名。
        let runtime = 环境目录(
            "XDG_RUNTIME_DIR",
            PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })),
        );
        Ok(Self {
            配置: 配置.join("mackey"),
            数据: 数据.join("mackey"),
            缓存: 环境目录("XDG_CACHE_HOME", 主目录.join(".cache")).join("mackey"),
            状态目录: 环境目录("XDG_STATE_HOME", 主目录.join(".local/state")).join("mackey"),
            服务目录: 配置.join("systemd/user"),
            命令入口: 环境目录("XDG_BIN_HOME", 主目录.join(".local/bin")).join("mackey"),
            套接字: runtime.join("mackey-focus.sock"),
            主目录,
        })
    }
    pub fn 扩展路径(&self, uuid: &str) -> PathBuf {
        self.数据
            .parent()
            .unwrap()
            .join("gnome-shell/extensions")
            .join(uuid)
    }
    pub fn 备份(&self) -> PathBuf {
        self.配置.join("backup/gsettings.json")
    }
    pub fn 保护(&self, path: &Path) -> Result<PathBuf> {
        保护主目录路径(&self.主目录, path)
    }
    pub fn 保护父目录(&self, path: &Path) -> Result<()> {
        if fs::canonicalize(path).ok() == Some(fs::canonicalize(&self.主目录)?) {
            return Ok(());
        }
        self.保护(path).map(|_| ())
    }
    pub fn 保护套接字(&self, path: &Path) -> Result<()> {
        if self.保护(path).is_ok() {
            return Ok(());
        }
        let expected = PathBuf::from(format!("/run/user/{}/mackey-focus.sock", unsafe {
            libc::getuid()
        }));
        ensure!(
            path == expected,
            "拒绝写入 HOME / 用户运行时目录之外的 socket: {}",
            path.display()
        );
        Ok(())
    }
    pub fn 校验安装路径(&self) -> Result<()> {
        for path in [
            &self.配置,
            &self.数据,
            &self.缓存,
            &self.状态目录,
            &self.服务目录,
            &self.命令入口,
            &self.扩展路径(crate::扩展标识),
        ] {
            self.保护(path)?;
        }
        self.保护套接字(&self.套接字)
    }
}
// 逐级解析现有祖先与 '..'，防止 HOME 内软链将写入或删除操作引向外部。
// 所有持久文件写入均应通过此保护。
pub fn 保护主目录路径(主目录: &Path, path: &Path) -> Result<PathBuf> {
    let 主目录 = fs::canonicalize(主目录).context("无法解析 HOME")?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut resolved = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            p => resolved.push(p.as_os_str()),
        }
        if resolved.try_exists()? {
            resolved = fs::canonicalize(&resolved)?;
        } else {
            ensure!(
                fs::symlink_metadata(&resolved).is_err(),
                "拒绝悬空软链: {}",
                resolved.display()
            );
        }
    }
    ensure!(
        resolved.starts_with(&主目录) && resolved != 主目录,
        "拒绝写入 {}：不在 $HOME 下",
        path.display()
    );
    Ok(resolved)
}
pub fn 原子写入(路径: &路径集合, path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let path = 路径.保护(path)?;
    let parent = path.parent().context("路径没有父目录")?;
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    use std::os::unix::fs::PermissionsExt;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn 写入结构数据(
    路径: &路径集合,
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    原子写入(路径, path, &bytes, 0o600)
}
pub fn 读取结构数据(path: &Path) -> Result<serde_json::Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("读取 {}", path.display()))?)
        .with_context(|| format!("解析 {}", path.display()))
}

#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 拒绝路径遍历与软链逃逸() {
        let temp = tempfile::tempdir_in(env::var_os("HOME").unwrap()).unwrap();
        let 主目录 = temp.path().join("home");
        fs::create_dir(&主目录).unwrap();
        std::os::unix::fs::symlink(temp.path(), 主目录.join("escape")).unwrap();
        assert!(保护主目录路径(&主目录, &主目录.join("../bad")).is_err());
        assert!(保护主目录路径(&主目录, &主目录.join("escape/bad")).is_err());
        assert!(保护主目录路径(&主目录, &主目录.join("escape/../bad")).is_err());
        assert!(保护主目录路径(&主目录, &主目录).is_err());
        assert!(保护主目录路径(&主目录, &主目录.join("good/new.json")).is_ok());
    }
}
