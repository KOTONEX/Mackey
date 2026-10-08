// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::{写入结构数据, 原子写入, 读取结构数据, 路径集合};
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
};

pub type 键位集合 = BTreeSet<String>;
pub type 键位表 = BTreeMap<String, String>;
fn 可空字符串<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct 行为条目 {
    pub 标识: String,
    #[serde(default)]
    pub 苹果按键: String,
    #[serde(default)]
    pub 描述: String,
    #[serde(default, deserialize_with = "可空字符串")]
    pub 触发键: String,
    #[serde(default)]
    pub 目标组合: BTreeMap<String, Value>,
    #[serde(default)]
    pub 状态: String,
    #[serde(default)]
    pub 归属: String,
    #[serde(default)]
    pub 策略: String,
    #[serde(default)]
    pub 说明: String,
    #[serde(default)]
    pub 冲突: String,
    #[serde(default)]
    pub 迁移引用: String,
    #[serde(default)]
    pub 自动生成: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct 行为清单 {
    pub 条目: Vec<行为条目>,
    pub 元信息: Value,
    pub 应用档: BTreeMap<String, Value>,
    #[serde(default)]
    pub 迁移规则: Vec<Value>,
    #[serde(default)]
    pub 吞控制组合: Value,
    #[serde(default)]
    pub 吞终端组合: Value,
    #[serde(default)]
    pub 泛化兜底: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct 键位迁移 {
    pub 模式: String,
    pub 键名: String,
    pub 标签: String,
    pub 原组合: String,
    pub 新组合: String,
    pub 原组合列表: Vec<String>,
    pub 目标组合: Vec<String>,
    pub 原值: Vec<String>,
    pub 新值: Vec<String>,
    pub 触发条目: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct 探测结果 {
    pub 已占用: 键位集合,
    pub 绑定表: 键位表,
    pub 绑定列表: Vec<(String, Vec<String>)>,
}
#[derive(Debug, Args, Default)]
pub struct 选项 {
    #[arg(long)]
    pub 清单: Option<PathBuf>,
    #[arg(long)]
    pub 用户配置: Option<PathBuf>,
    #[arg(long)]
    pub 输出目录: Option<PathBuf>,
    #[arg(long)]
    pub 文档: Option<PathBuf>,
    #[arg(long)]
    pub 备份: Option<PathBuf>,
    #[arg(long)]
    pub 不探测: bool,
    #[arg(long)]
    pub 不生成文档: bool,
    #[arg(long)]
    pub 报告: bool,
}
pub fn 字符串列表(v: &Value) -> Vec<String> {
    if let Some(s) = v.as_str() {
        vec![s.to_owned()]
    } else {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}
fn 修饰键顺序(s: &str) -> usize {
    match s {
        "C" => 0,
        "S" => 1,
        "A" => 2,
        "SUPER" => 3,
        "HYPER" => 4,
        _ => 9,
    }
}
pub fn 规范组合(combo: &str) -> String {
    let upper = combo.to_uppercase();
    let mut parts: Vec<&str> = upper.split('-').collect();
    let key = parts.pop().unwrap_or("");
    parts.sort_by_key(|m| (修饰键顺序(m), *m));
    parts.dedup();
    parts.push(key);
    parts.join("-")
}
fn 系统键名(key: &str) -> &str {
    match key {
        "PAGE_UP" | "PRIOR" => "PAGEUP",
        "PAGE_DOWN" | "NEXT" => "PAGEDOWN",
        "BRACKETLEFT" => "LEFTBRACE",
        "BRACKETRIGHT" => "RIGHTBRACE",
        "PERIOD" => "DOT",
        "ABOVE_TAB" => "GRAVE",
        "RETURN" => "ENTER",
        "ESCAPE" => "ESC",
        "SYSRQ" => "PRINT",
        _ => key,
    }
}
pub fn 解析加速键(accel: &str) -> Option<String> {
    let mut rest = accel.trim();
    let mut mods = Vec::new();
    while rest.starts_with('<') {
        let end = rest.find('>')?;
        let m = rest[1..end].to_uppercase();
        mods.push(
            match m.as_str() {
                "PRIMARY" | "CONTROL" | "CTRL" => "C",
                "SHIFT" => "S",
                "ALT" | "MOD1" => "A",
                "META" | "SUPER" | "MOD4" => "SUPER",
                _ => &m,
            }
            .to_owned(),
        );
        rest = &rest[end + 1..];
    }
    if rest.is_empty() {
        return None;
    }
    let key = rest.to_uppercase();
    mods.push(系统键名(key.strip_prefix("KEY_").unwrap_or(&key)).to_owned());
    Some(规范组合(&mods.join("-")))
}
pub fn 转为加速键(combo: &str) -> String {
    let normalized = 规范组合(combo);
    let mut parts: Vec<&str> = normalized.split('-').collect();
    let key = parts.pop().unwrap_or("");
    let key = match key {
        "PAGEUP" => "Page_Up",
        "PAGEDOWN" => "Page_Down",
        "LEFTBRACE" => "bracketleft",
        "RIGHTBRACE" => "bracketright",
        "DOT" => "period",
        "BACKSPACE" => "BackSpace",
        "ENTER" => "Return",
        "ESC" => "Escape",
        "PRINT" => "Print",
        "LEFT" => "Left",
        "RIGHT" => "Right",
        "UP" => "Up",
        "DOWN" => "Down",
        "HOME" => "Home",
        "END" => "End",
        "DELETE" => "Delete",
        "INSERT" => "Insert",
        "TAB" => "Tab",
        _ => "",
    };
    let raw_key = normalized.rsplit('-').next().unwrap();
    let key = if !key.is_empty() {
        key.to_owned()
    } else if raw_key.starts_with('F') && raw_key[1..].parse::<u8>().is_ok() {
        raw_key.to_owned()
    } else {
        raw_key.to_lowercase()
    };
    parts
        .iter()
        .map(|m| {
            format!(
                "<{}>",
                match *m {
                    "C" => "Control",
                    "S" => "Shift",
                    "A" => "Alt",
                    "SUPER" => "Super",
                    "HYPER" => "Hyper",
                    other => other,
                }
            )
        })
        .collect::<String>()
        + &key
}
pub fn 解析加速键列表(raw: &str) -> Vec<String> {
    // GVariant 引号内支持反斜杠转义，不执行其文本。
    let mut values = Vec::new();
    let mut 当前键盘 = String::new();
    let mut quote = None;
    let mut escape = false;
    for c in raw.chars() {
        if let Some(q) = quote {
            if escape {
                当前键盘.push(c);
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == q {
                if !当前键盘.is_empty() {
                    values.push(std::mem::take(&mut 当前键盘));
                }
                quote = None;
            } else {
                当前键盘.push(c);
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        }
    }
    values
}
pub fn 转为加速键列表(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|s| format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
pub fn 探测() -> Result<探测结果> {
    let mut p = 探测结果::default();
    for schema in [
        "org.gnome.desktop.wm.keybindings",
        "org.gnome.shell.keybindings",
        "org.gnome.mutter.keybindings",
        "org.gnome.settings-daemon.plugins.media-keys",
    ] {
        let out = Command::new("gsettings")
            .args(["list-recursively", schema])
            .output()
            .context("找不到 gsettings；离线生成请使用 --不探测")?;
        ensure!(
            out.status.success(),
            "GNOME 键位探测失败：{}: {}",
            schema,
            String::from_utf8_lossy(&out.stderr)
        );
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() != 3 {
                continue;
            }
            let ident = format!("{schema} {}", parts[1]);
            p.绑定表.insert(ident.clone(), parts[2].to_owned());
            if parts[1].ends_with("-static") || parts[1] == "custom-keybindings" {
                continue;
            }
            let accels: Vec<_> = 解析加速键列表(parts[2])
                .iter()
                .filter_map(|a| 解析加速键(a))
                .collect();
            p.已占用.extend(accels.iter().cloned());
            if !accels.is_empty() {
                p.绑定列表.push((ident, accels));
            }
        }
    }
    for path in 解析加速键列表(
        p.绑定表
            .get("org.gnome.settings-daemon.plugins.media-keys custom-keybindings")
            .map(String::as_str)
            .unwrap_or(""),
    ) {
        let schema =
            format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}");
        let out = Command::new("gsettings")
            .args(["list-recursively", &schema])
            .output()?;
        ensure!(out.status.success(), "自定义快捷键探测失败：{path}");
        let mut name = path;
        let mut accels = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<_> = line.splitn(3, ' ').collect();
            if parts.len() != 3 {
                continue;
            }
            if parts[1] == "name" {
                name = 解析加速键列表(parts[2]).first().cloned().unwrap_or(name);
            }
            if parts[1] == "binding" {
                accels = 解析加速键列表(parts[2])
                    .iter()
                    .filter_map(|a| 解析加速键(a))
                    .collect();
            }
        }
        p.已占用.extend(accels.iter().cloned());
        if !accels.is_empty() {
            p.绑定列表.push((format!("自定义快捷键：{name}"), accels));
        }
    }
    Ok(p)
}
fn 合法组合(combo: &str) -> bool {
    let parts: Vec<_> = combo.split('-').collect();
    let mods = [
        "C", "S", "A", "SUPER", "M", "W", "CTRL", "SHIFT", "ALT", "HYPER",
    ];
    let key = parts.last().unwrap();
    let keys = "HOME END PAGEUP PAGEDOWN LEFT RIGHT UP DOWN DELETE BACKSPACE INSERT ENTER TAB SPACE ESC GRAVE MINUS EQUAL LEFTBRACE RIGHTBRACE BACKSLASH SEMICOLON APOSTROPHE COMMA DOT SLASH PRINT SYSRQ SUPER_L SUPER_R LEFTCTRL LEFTALT";
    parts[..parts.len() - 1].iter().all(|m| mods.contains(m))
        && (keys.split_whitespace().any(|k| k == *key)
            || (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()))
            || key
                .strip_prefix('F')
                .and_then(|n| n.parse::<u8>().ok())
                .is_some_and(|n| (1..=15).contains(&n) || n == 24))
}
pub fn 校验(c: &行为清单) -> Result<()> {
    let ids: 键位集合 = c
        .迁移规则
        .iter()
        .filter_map(|r| r["标识"].as_str().map(str::to_owned))
        .collect();
    let mut seen = 键位集合::new();
    for e in &c.条目 {
        ensure!(seen.insert(e.标识.clone()), "重复条目: {}", e.标识);
        let expected = if ["translate", "auto"].contains(&e.策略.as_str()) && !e.目标组合.is_empty()
        {
            "macos"
        } else {
            "gnome"
        };
        ensure!(
            e.归属 == expected,
            "{}：归属与策略/目标组合 不一致（应为 {expected}）",
            e.标识
        );
        ensure!(
            e.迁移引用.is_empty() || ids.contains(&e.迁移引用),
            "{}：迁移引用不存在",
            e.标识
        );
    }
    for profile in ["终端", "文件管理器"] {
        ensure!(
            !字符串列表(&c.应用档.get(profile).context("缺少应用配置档")?["匹配"]).is_empty(),
            "{profile} 缺少匹配串"
        );
    }
    if c.吞控制组合["启用"] == true {
        ensure!(
            字符串列表(&c.吞控制组合["例外应用档"])
                .iter()
                .any(|s| s == "终端"),
            "吞控制组合 例外必须包含终端配置档"
        );
        ensure!(
            !字符串列表(&c.吞控制组合["例外应用"]).is_empty(),
            "吞控制组合 例外必须包含内嵌终端应用"
        );
    }
    Ok(())
}
pub fn 生成迁移计划(c: &行为清单, 绑定表: &键位表) -> Result<Vec<键位迁移>> {
    let mut 迁移计划 = Vec::new();
    for r in &c.迁移规则 {
        let key = r["键名"].as_str().context("迁移缺少键名")?;
        let keys: Vec<String> = if let Some((head, tail)) = key.split_once("..") {
            let prefix = head.trim_end_matches(|c: char| c.is_ascii_digit());
            let start: u32 = head[prefix.len()..].parse()?;
            let end: u32 = tail.parse()?;
            ensure!(end >= start && end - start < 100, "非法迁移范围");
            (start..=end).map(|n| format!("{prefix}{n}")).collect()
        } else {
            key.split('|').map(str::to_owned).collect()
        };
        let from = 字符串列表(&r["原组合"]);
        let to = 字符串列表(&r["新组合"]);
        ensure!(
            from.len() == to.len() && !from.is_empty(),
            "迁移定义 {}：原组合/新组合数量不一致",
            r["标识"]
        );
        let schema = r["模式"].as_str().context("迁移缺少模式")?;
        for key in keys {
            let Some(raw) = 绑定表.get(&format!("{schema} {key}")) else {
                continue;
            };
            let old = 解析加速键列表(raw);
            let canon: Vec<_> = old.iter().map(|a| 解析加速键(a)).collect();
            let mut replacements = BTreeMap::new();
            let mut moved_from = Vec::new();
            let mut moved_to = Vec::new();
            for (src, dst) in from.iter().zip(&to) {
                let direction = key.rsplit('-').next().unwrap().to_uppercase();
                let mut src = 规范组合(&src.replace("{D}", &direction));
                let mut dst = 规范组合(&dst.replace("{D}", &direction));
                if src.contains("..") {
                    let suffix = key.chars().last().unwrap();
                    src = format!(
                        "{}{suffix}",
                        src.replace("..", "")
                            .trim_end_matches(|c: char| c.is_ascii_digit())
                    );
                    dst = format!(
                        "{}{suffix}",
                        dst.replace("..", "")
                            .trim_end_matches(|c: char| c.is_ascii_digit())
                    );
                }
                if !canon.contains(&Some(src.clone())) {
                    continue;
                }
                replacements.insert(src.clone(), dst.clone());
                moved_from.push(src);
                moved_to.push(dst);
            }
            if replacements.is_empty() {
                continue;
            }
            let new = old
                .iter()
                .zip(&canon)
                .map(|(raw, canon)| {
                    canon
                        .as_ref()
                        .and_then(|c| replacements.get(c))
                        .map(|s| 转为加速键(s))
                        .unwrap_or(raw.clone())
                })
                .collect();
            迁移计划.push(键位迁移 {
                模式: schema.to_owned(),
                键名: key,
                标签: r["标签"].as_str().unwrap_or("").to_owned(),
                原组合: moved_from.join("、"),
                新组合: moved_to.join("、"),
                原组合列表: moved_from,
                目标组合: moved_to,
                原值: old,
                新值: new,
                触发条目: r["触发条目"].as_str().map(str::to_owned),
            });
        }
    }
    Ok(迁移计划)
}
fn 泛化兜底(c: &行为清单, p: &探测结果, reserved: &键位集合) -> Vec<行为条目> {
    if c.泛化兜底["启用"] != true {
        return Vec::new();
    }
    let explicit: 键位集合 = c.条目.iter().map(|e| 规范组合(&e.触发键)).collect();
    let excluded: 键位集合 = 字符串列表(&c.泛化兜底["排除触发键"])
        .iter()
        .map(|t| 规范组合(t))
        .collect();
    let kinds = 字符串列表(&c.泛化兜底["类别"]);
    let mut out = Vec::new();
    let mut add = |kind: &str, trigger: String, target: String, mac: String| {
        let trigger = 规范组合(&trigger);
        if !kinds.iter().any(|k| k == kind)
            || explicit.contains(&trigger)
            || p.已占用.contains(&trigger)
            || reserved.contains(&trigger)
            || excluded.contains(&trigger)
        {
            return;
        }
        out.push(行为条目 {
            标识: format!("泛化兜底-{trigger}"),
            苹果按键: mac,
            描述: "（泛化翻译）".into(),
            触发键: trigger,
            目标组合: BTreeMap::from([("通用".into(), json!(target))]),
            状态: "mapped".into(),
            归属: "macos".into(),
            策略: "translate".into(),
            自动生成: true,
            ..行为条目::default()
        });
    };
    for ch in 'A'..='Z' {
        add(
            "letter",
            format!("Super-{ch}"),
            format!("C-{ch}"),
            format!("⌘{ch}"),
        );
        add(
            "letter-shift",
            format!("Super-S-{ch}"),
            format!("C-S-{ch}"),
            format!("⇧⌘{ch}"),
        );
    }
    for d in '1'..='9' {
        add(
            "digit-shift",
            format!("Super-S-{d}"),
            format!("C-S-{d}"),
            format!("⌘⇧{d}"),
        );
    }
    out
}
pub fn 截图目标(
    绑定表: &键位表, blocked: &键位集合, allow: &键位集合
) -> 键位表 {
    let mut result = 键位表::new();
    for (id, keys) in [
        (
            "screenshot-region",
            vec!["org.gnome.shell.keybindings show-screenshot-ui"],
        ),
        (
            "screenshot-all",
            vec![
                "org.gnome.shell.keybindings screenshot",
                "org.gnome.settings-daemon.plugins.media-keys screenshot",
            ],
        ),
        (
            "screenshot-window",
            vec![
                "org.gnome.shell.keybindings screenshot-window",
                "org.gnome.settings-daemon.plugins.media-keys window-screenshot",
            ],
        ),
    ] {
        for key in keys {
            let Some(raw) = 绑定表.get(key) else {
                continue;
            };
            let Some(accel) = 解析加速键列表(raw).iter().find_map(|a| 解析加速键(a))
            else {
                continue;
            };
            if !合法组合(&accel) || (blocked.contains(&accel) && !allow.contains(&accel)) {
                continue;
            }
            result.insert(id.to_owned(), accel);
            break;
        }
    }
    result
}
fn 组装映射(条目: &[行为条目], profile: &str) -> Value {
    let mut map = serde_json::Map::new();
    for e in 条目 {
        let Some(target) = e.目标组合.get(profile).or_else(|| e.目标组合.get("通用"))
        else {
            continue;
        };
        let mut combos: Vec<_> = 字符串列表(target)
            .iter()
            .flat_map(|s| s.split(',').map(|s| 规范组合(s.trim())).collect::<Vec<_>>())
            .collect();
        if combos.len() == 1 {
            map.insert(规范组合(&e.触发键), json!(combos.remove(0)));
        } else if !combos.is_empty() {
            map.insert(规范组合(&e.触发键), json!(combos));
        }
    }
    Value::Object(map)
}
pub struct 生成结果 {
    pub 配置: Value,
    pub 迁移计划: Vec<键位迁移>,
    pub 条目: Vec<行为条目>,
}
pub fn 编译配置(c: &行为清单, user: &Value, p: &探测结果) -> Result<生成结果> {
    校验(c)?;
    let 迁移计划 = 生成迁移计划(c, &p.绑定表)?;
    let reserved: 键位集合 = 迁移计划
        .iter()
        .flat_map(|r| r.目标组合.iter().cloned())
        .collect();
    let mut 条目 = c.条目.clone();
    条目.extend(泛化兜底(c, p, &reserved));
    let mut consuming: 键位集合 = 条目
        .iter()
        .filter(|e| !e.目标组合.is_empty())
        .map(|e| 规范组合(&e.触发键))
        .collect();
    for spec in [&c.吞控制组合, &c.吞终端组合] {
        if spec["启用"] == true {
            consuming.extend(字符串列表(&spec["触发组合"]).iter().map(|s| 规范组合(s)));
        }
    }
    let allow: 键位集合 = 条目
        .iter()
        .filter(|e| e.标识.starts_with("screenshot-"))
        .map(|e| 规范组合(&e.触发键))
        .collect();
    let derived = 截图目标(&p.绑定表, &consuming, &allow);
    for e in &mut 条目 {
        if let Some(target) = derived.get(&e.标识) {
            e.目标组合.insert("通用".into(), json!(target));
        }
    }
    for e in &条目 {
        if !e.触发键.is_empty() {
            ensure!(合法组合(&规范组合(&e.触发键)), "非法键名: {}", e.触发键);
        }
        for v in e.目标组合.values() {
            for s in 字符串列表(v) {
                for combo in s.split(',') {
                    ensure!(合法组合(&规范组合(combo.trim())), "非法键名: {combo}");
                }
            }
        }
    }
    let swallow = c.元信息["吞键输出"].as_str().context("缺少吞键输出")?;
    ensure!(合法组合(&规范组合(swallow)), "非法吞键键名");
    for spec in [&c.吞控制组合, &c.吞终端组合] {
        for t in 字符串列表(&spec["触发组合"]) {
            ensure!(合法组合(&规范组合(&t)), "非法吞键组合: {t}");
        }
    }
    let clashes: Vec<_> = reserved.intersection(&consuming).collect();
    ensure!(clashes.is_empty(), "迁移目标与清单触发键冲突: {clashes:?}");
    let planned: 键位集合 = 迁移计划
        .iter()
        .flat_map(|r| r.原组合列表.iter().cloned())
        .collect();
    let unplanned: Vec<_> = consuming
        .intersection(&p.已占用)
        .filter(|k| !planned.contains(*k))
        .collect();
    ensure!(
        unplanned.is_empty(),
        "以下触发键与 GNOME 已占用键位冲突，但没有迁移计划: {unplanned:?}"
    );
    let layout = user["修饰键布局"].as_str().unwrap_or("苹果");
    let mut 配置 = json!({});
    match layout {
        "电脑换位" => {
            配置["modmap"] = json!([{"name":"pc-position-swap: 把 ⌘/⌥ 摆到 macOS 的物理位置", "remap":{"ALT_L":"SUPER_L","SUPER_L":"ALT_L","ALT_R":"SUPER_R","SUPER_R":"ALT_R"}}])
        }
        "苹果" => {}
        _ => bail!("未知的修饰键布局: {layout}"),
    }
    for key in ["keypress_delay_ms", "throttle_ms"] {
        if user[key].as_u64().is_some_and(|n| n > 0) {
            配置[key] = user[key].clone();
        }
    }
    if user["notifications"] == true {
        配置["notifications"] = json!(true);
    }
    let mut maps = Vec::new();
    let mut push = |name: &str, application: Option<Value>, remap: Value| {
        let mut node = json!({"name":name,"remap":remap});
        if let Some(app) = application {
            node["application"] = app;
        }
        if !字符串列表(&user["device"]["only"]).is_empty()
            || !字符串列表(&user["device"]["not"]).is_empty()
        {
            node["device"] = user["device"].clone();
        }
        maps.push(node);
    };
    let terminal: Vec<_> = 条目
        .iter()
        .filter(|e| e.目标组合.contains_key("终端"))
        .cloned()
        .collect();
    let files: Vec<_> = 条目
        .iter()
        .filter(|e| e.目标组合.contains_key("文件管理器"))
        .cloned()
        .collect();
    let explicit: Vec<_> = 条目.iter().filter(|e| !e.自动生成).cloned().collect();
    let generated: Vec<_> = 条目.iter().filter(|e| e.自动生成).cloned().collect();
    let term_match = &c.应用档["终端"]["匹配"];
    let swallow_map = |spec: &Value| -> Value {
        字符串列表(&spec["触发组合"])
            .iter()
            .map(|t| (规范组合(t), json!(规范组合(swallow))))
            .collect::<serde_json::Map<_, _>>()
            .into()
    };
    push(
        "terminal：终端里 Ctrl+C/Z/D 是信号与 EOF，必须单独一套",
        Some(json!({"only":term_match})),
        组装映射(&terminal, "终端"),
    );
    if c.吞终端组合["启用"] == true && !字符串列表(&c.吞终端组合["触发组合"]).is_empty()
    {
        push(
            "swallow-terminal：终端里吞掉 Ctrl+Shift+C/V（复制/粘贴只认 ⌘C/⌘V）",
            Some(json!({"only":term_match})),
            swallow_map(&c.吞终端组合),
        );
    }
    push(
        "files：文件管理器语义不同（⌘⌫=废纸篓、⌘I=属性）",
        Some(json!({"only":c.应用档["文件管理器"]["匹配"]})),
        组装映射(&files, "文件管理器"),
    );
    push(
        "generic：其余应用的兜底（只含显式条目）",
        None,
        组装映射(&explicit, "通用"),
    );
    if !generated.is_empty() {
        push(
            "generic-sweep：泛化兜底（终端不参与）",
            Some(json!({"not":term_match})),
            组装映射(&generated, "通用"),
        );
    }
    if c.吞控制组合["启用"] == true && !字符串列表(&c.吞控制组合["触发组合"]).is_empty()
    {
        let mut except = Vec::new();
        for profile in 字符串列表(&c.吞控制组合["例外应用档"]) {
            except.extend(字符串列表(
                &c.应用档.get(&profile).context("例外配置档不存在")?["匹配"],
            ));
        }
        except.extend(字符串列表(&c.吞控制组合["例外应用"]));
        push(
            "swallow：替换模式（终端与内嵌终端应用除外）",
            Some(json!({"not":except})),
            swallow_map(&c.吞控制组合),
        );
    }
    配置["keymap"] = json!(maps);
    Ok(生成结果 {
        配置,
        迁移计划,
        条目,
    })
}
pub fn 执行(路径: &路径集合, options: &选项) -> Result<生成结果> {
    let c: 行为清单 = if let Some(path) = &options.清单 {
        serde_json::from_value(读取结构数据(path)?)?
    } else {
        serde_json::from_str(crate::嵌入清单)?
    };
    let user_path = options
        .用户配置
        .clone()
        .unwrap_or(路径.配置.join("config.json"));
    let user = if user_path.exists() {
        crate::用户配置::读取(&user_path)?
    } else {
        json!({})
    };
    let p = if options.不探测 {
        探测结果::default()
    } else {
        探测()?
    };
    let result = 编译配置(&c, &user, &p)?;
    println!(
        "Mackey 生成报告\n布局           : {}\n清单条目       : {} 条显式 + {} 条泛化兜底\nGNOME 已占用   : {} 个加速键\n需要迁移 GNOME 键位 {} 条",
        user["修饰键布局"].as_str().unwrap_or("苹果"),
        c.条目.len(),
        result.条目.len() - c.条目.len(),
        p.已占用.len(),
        result.迁移计划.len()
    );
    if !options.报告 {
        let out = options.输出目录.clone().unwrap_or(路径.配置.clone());
        let 文档 = options
            .文档
            .clone()
            .or_else(|| std::env::var_os("MACKEY_DOCS").map(PathBuf::from))
            .unwrap_or(路径.数据.join("docs/03-行为清单.md"));
        路径.保护(&out)?;
        if !options.不生成文档 {
            路径.保护(&文档)?;
        }
        写入结构数据(路径, &out.join("xremap.json"), &result.配置)?;
        写入结构数据(路径, &out.join("relocations.json"), &result.迁移计划)?;
        if !options.不生成文档 {
            let 备份 = options.备份.clone().unwrap_or(路径.备份());
            原子写入(
                路径,
                &文档,
                生成行为文档(&c, &result, &p, &备份).as_bytes(),
                0o600,
            )?;
        }
        println!("已写入：{}", out.display());
    }
    Ok(result)
}
pub fn 显示组合(combo: &Value) -> String {
    if combo.is_null() {
        return "—".into();
    }
    if let Some(a) = combo.as_array() {
        return a.iter().map(显示组合).collect::<Vec<_>>().join(" → ");
    }
    let s = combo.as_str().unwrap_or("");
    if s.contains(',') {
        return s
            .split(',')
            .map(|s| 显示组合(&json!(s.trim())))
            .collect::<Vec<_>>()
            .join(" → ");
    }
    let s = s.to_uppercase();
    let mut parts: Vec<_> = s.split('-').collect();
    let key = parts.pop().unwrap();
    parts.sort_by_key(|m| match *m {
        "C" => 0,
        "A" | "M" => 1,
        "HYPER" => 2,
        "S" => 3,
        "SUPER" => 4,
        _ => 9,
    });
    let key = match key {
        "LEFT" => "←",
        "RIGHT" => "→",
        "UP" => "↑",
        "DOWN" => "↓",
        "HOME" => "Home",
        "END" => "End",
        "PAGEUP" => "PgUp",
        "PAGEDOWN" => "PgDn",
        "BACKSPACE" => "⌫",
        "DELETE" => "⌦",
        "ENTER" => "↩",
        "SPACE" => "Space",
        "TAB" => "⇥",
        "LEFTBRACE" => "[",
        "RIGHTBRACE" => "]",
        "EQUAL" => "＋",
        "MINUS" => "-",
        "COMMA" => ",",
        "DOT" => ".",
        "GRAVE" => "`",
        "SUPER_L" => "Super(单击)",
        "SLASH" => "/",
        "BACKSLASH" => "\\",
        "PRINT" => "Print",
        "F24" => "F24(吞掉)",
        k => k,
    };
    parts
        .iter()
        .map(|m| match *m {
            "SUPER" => "⌘".into(),
            "S" => "⇧".into(),
            "A" | "M" => "⌥".into(),
            "C" => "⌃".into(),
            "HYPER" => "✥".into(),
            m => format!("{m}+"),
        })
        .collect::<String>()
        + key
}
fn 生成行为文档(
    c: &行为清单, g: &生成结果, p: &探测结果, 备份: &std::path::Path
) -> String {
    let macos: Vec<_> = c.条目.iter().filter(|e| e.归属 == "macos").collect();
    let gnome: Vec<_> = c.条目.iter().filter(|e| e.归属 == "gnome").collect();
    let generated: Vec<_> = g.条目.iter().filter(|e| e.自动生成).collect();
    let mut doc = format!(
        "# 行为清单（自动生成）\n\n> 由 `配置/行为清单.json` + GNOME 键位探测结果生成；修改 JSON 后运行 `cargo run --quiet -- 生成 --文档 文档/03-行为清单.md`。\n\n- GNOME 已占用加速键：**{}** 个\n- 需要迁移的 GNOME 键位：**{}** 条\n- 归属分类：遵循 macOS **{}** 条 · 遵循 GNOME 默认 **{}** 条 · 泛化兜底 **{}** 条\n\n## 一、遵循 macOS 的键位（由 Mackey 改写）\n\n| macOS | 行为 | 触发 | 默认输出 | 终端输出 | 文件管理器 | 状态 | 冲突 / 备注 |\n|---|---|---|---|---|---|---|---|\n",
        p.已占用.len(),
        g.迁移计划.len(),
        macos.len(),
        gnome.len(),
        generated.len()
    );
    for e in g.条目.iter().filter(|e| !e.自动生成 && e.归属 == "macos") {
        let 状态 = match e.状态.as_str() {
            "native" => "🟢 原生等价",
            "mapped" => "🔵 需映射",
            "relocated" => "🟠 需迁移 GNOME 键位",
            "approx" => "🟡 近似",
            "optin" => "⚪ 可选（默认关）",
            _ => "🔴 无法模拟",
        };
        doc += &format!(
            "| {} | {} | {} | {} | {} | {} | {} | {}{} |\n",
            e.苹果按键,
            e.描述,
            显示组合(&json!(e.触发键)),
            显示组合(e.目标组合.get("通用").unwrap_or(&Value::Null)),
            显示组合(e.目标组合.get("终端").unwrap_or(&Value::Null)),
            显示组合(e.目标组合.get("文件管理器").unwrap_or(&Value::Null)),
            状态,
            if e.冲突.is_empty() {
                String::new()
            } else {
                format!("**冲突**：{}。", e.冲突)
            },
            e.说明
        );
    }
    doc += "\n## 二、遵循 GNOME 默认的键位（Mackey 不接管）\n\n| 键位 | 行为 | 归因 | 说明 |\n|---|---|---|---|\n";
    for e in gnome {
        doc += &format!(
            "| {} | {} | {} | {} |\n",
            if e.触发键.is_empty() {
                e.苹果按键.clone()
            } else {
                显示组合(&json!(e.触发键))
            },
            e.描述,
            e.状态,
            e.说明
        );
    }
    doc += "\n## 三、GNOME 已占用键位的归属（探测自 dconf）\n\n| GNOME 绑定 | 加速键 | 归属 |\n|---|---|---|\n";
    let reserved: 键位集合 = g
        .迁移计划
        .iter()
        .flat_map(|r| r.目标组合.iter().cloned())
        .collect();
    for (ident, accels) in &p.绑定列表 {
        for accel in accels {
            let attr = if let Some(r) = g.迁移计划.iter().find(|r| r.原组合列表.contains(accel))
            {
                format!("已迁移 → {}", r.新组合)
            } else if reserved.contains(accel) {
                "迁移落点（GNOME 功能新位置）".into()
            } else if g
                .条目
                .iter()
                .any(|e| !e.目标组合.is_empty() && 规范组合(&e.触发键) == *accel)
            {
                "被 Mackey 接管（macOS 行为）".into()
            } else {
                "GNOME 保留".into()
            };
            doc += &format!(
                "| `{ident}` | `{accel}` {} | {attr} |\n",
                显示组合(&json!(accel))
            );
        }
    }
    doc += &format!(
        "\n## 四、泛化兜底（自动生成 {} 条，遵循 macOS）\n\n终端通过 `application.not` 排除泛化兜底；显式条目仍按清单工作。\n\n| macOS | 触发 | 输出 |\n|---|---|---|\n",
        generated.len()
    );
    for e in generated {
        doc += &format!(
            "| {} | {} | {} |\n",
            e.苹果按键,
            显示组合(&json!(e.触发键)),
            显示组合(&e.目标组合["通用"])
        );
    }
    doc += "\n## 五、替换模式：被吞掉的 Linux Ctrl 组合\n\n终端及内嵌终端应用的 Ctrl+C 保留 SIGINT；未列出的 Ctrl 组合原样放行。\n\n";
    for (name, spec) in [("吞控制组合", &c.吞控制组合), ("吞终端组合", &c.吞终端组合)]
    {
        let triggers = 字符串列表(&spec["触发组合"]);
        doc += &format!(
            "`{name}`：{}，{} 个组合：{}。\n\n",
            if spec["启用"] == true {
                "启用"
            } else {
                "关闭"
            },
            triggers.len(),
            triggers
                .iter()
                .map(|t| 显示组合(&json!(t)))
                .collect::<Vec<_>>()
                .join("、")
        );
    }
    for matcher in 字符串列表(&c.吞控制组合["例外应用"]) {
        doc += &format!("- `{matcher}`\n");
    }
    doc += "\n## 六、需要迁移的 GNOME 键位\n\n| GNOME 功能 | 原按键 | 迁移到 | 为谁让路 |\n|---|---|---|---|\n";
    for r in &g.迁移计划 {
        doc += &format!(
            "| {} | {} | {} | {} |\n",
            r.标签,
            r.原组合,
            r.新组合,
            r.触发条目.as_deref().unwrap_or("")
        );
    }
    if let Ok(old) = 读取结构数据(备份)
        && let Some(old) = old.as_object()
    {
        doc += "\n此前已经执行过的迁移（由备份与当前键位推算；还原 / 卸载 可还原）：\n\n";
        for (ident, values) in old {
            if let Some(now) = p.绑定表.get(ident)
                && 字符串列表(values) != 解析加速键列表(now)
            {
                doc += &format!(
                    "- `{ident}`：{} → {}\n",
                    字符串列表(values).join("、"),
                    解析加速键列表(now).join("、")
                );
            }
        }
    }
    doc += "\n## 七、明确不做的事\n\n- 不做 Ctrl 与 ⌘ 全局交换。\n- 不删除 GNOME 功能；冲突先备份再迁移，卸载时还原。\n";
    doc
}

#[cfg(test)]
mod 测试 {
    use super::*;
    fn 清单() -> 行为清单 {
        serde_json::from_str(crate::嵌入清单).unwrap()
    }
    #[test]
    fn 旧版配置完全一致() {
        let g = 编译配置(&清单(), &json!({}), &探测结果::default()).unwrap();
        let expected: Value =
            serde_json::from_str(include_str!("../../测试/基准/xremap.json")).unwrap();
        assert_eq!(g.配置, expected);
        assert_eq!(g.条目.len(), 115);
    }
    #[test]
    fn 拒绝错误归属引用与终端例外() {
        let mut c = 清单();
        c.条目[0].归属 = "bad".into();
        assert!(校验(&c).is_err());
        c = 清单();
        c.条目[0].迁移引用 = "missing".into();
        assert!(校验(&c).is_err());
        c = 清单();
        c.吞控制组合["例外应用档"] = json!([]);
        assert!(校验(&c).is_err());
    }
    #[test]
    fn 规范加速键() {
        assert_eq!(规范组合("Super-S-a"), "S-SUPER-A");
        assert_eq!(解析加速键("<Control><Alt>Left").unwrap(), "C-A-LEFT");
        assert_eq!(转为加速键("S-SUPER-A"), "<Shift><Super>a");
        assert_eq!(解析加速键列表("@as []"), Vec::<String>::new());
    }
    #[test]
    fn 迁移原生键位并检查冲突() {
        let c = 清单();
        let mut p = 探测结果::default();
        for (schema, key, accels) in [
            (
                "org.gnome.shell.keybindings",
                "toggle-application-view",
                vec!["<Super>a"],
            ),
            (
                "org.gnome.shell.keybindings",
                "toggle-message-tray",
                vec!["<Super>v", "<Super>m"],
            ),
            (
                "org.gnome.desktop.wm.keybindings",
                "maximize",
                vec!["<Super>Up"],
            ),
            (
                "org.gnome.desktop.wm.keybindings",
                "unmaximize",
                vec!["<Super>Down", "<Alt>F5"],
            ),
            (
                "org.gnome.desktop.wm.keybindings",
                "switch-input-source",
                vec!["<Super>space", "XF86Keyboard"],
            ),
            (
                "org.gnome.desktop.wm.keybindings",
                "move-to-monitor-left",
                vec!["<Super><Shift>Left"],
            ),
            (
                "org.gnome.shell.keybindings",
                "switch-to-application-9",
                vec!["<Super>9"],
            ),
        ] {
            p.绑定表.insert(
                format!("{schema} {key}"),
                转为加速键列表(&accels.iter().map(|a| a.to_string()).collect::<Vec<_>>()),
            );
            p.已占用.extend(accels.iter().filter_map(|a| 解析加速键(a)));
        }
        let g = 编译配置(&c, &json!({}), &p).unwrap();
        assert_eq!(g.迁移计划.len(), 7);
        assert!(
            g.迁移计划
                .iter()
                .find(|r| r.键名 == "unmaximize")
                .unwrap()
                .新值
                .contains(&"<Alt>F5".to_owned())
        );
        p.已占用.insert("SUPER-C".into());
        assert!(编译配置(&c, &json!({}), &p).is_err());
    }
    #[test]
    fn 截图重新绑定与设备过滤() {
        let p = 探测结果 {
            绑定表: BTreeMap::from([(
                "org.gnome.shell.keybindings screenshot".into(),
                "['<Control>Print']".into(),
            )]),
            ..探测结果::default()
        };
        let g = 编译配置(
            &清单(),
            &json!({"修饰键布局":"电脑换位","device":{"only":["MX Keys"]}}),
            &p,
        )
        .unwrap();
        assert_eq!(g.配置["keymap"][3]["remap"]["S-SUPER-4"], "C-PRINT");
        assert!(
            g.配置["keymap"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["device"]["only"][0] == "MX Keys")
        );
        assert_eq!(g.配置["modmap"][0]["remap"]["ALT_L"], "SUPER_L");
        assert!(
            截图目标(
                &p.绑定表,
                &键位集合::from(["C-PRINT".into()]),
                &键位集合::new()
            )
            .is_empty()
        );
    }
}
