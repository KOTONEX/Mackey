// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::{写入结构数据, 原子写入, 读取结构数据, 路径集合};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{fs, path::Path};

/// 仅作为旧配置升级输入识别英文键；写回时只有中文自有键。
pub fn 规范化(mut 配置: Value) -> Result<Value> {
    ensure!(配置.is_object(), "用户配置必须是 JSON 对象");
    let 对象 = 配置.as_object_mut().unwrap();
    for (旧键, 新键) in [
        ("modifier_layout", "修饰键布局"),
        ("engine", "引擎"),
        ("_modifier_layout_说明", "_修饰键布局_说明"),
        ("_engine_说明", "_引擎_说明"),
    ] {
        if let Some(值) = 对象.remove(旧键) {
            对象.entry(新键).or_insert(值);
        }
    }
    if let Some(旧值) = 对象.get("修饰键布局").and_then(Value::as_str) {
        let 新值 = match 旧值 {
            "apple" => Some("苹果"),
            "pc-swap" => Some("电脑换位"),
            "auto" => Some("自动"),
            _ => None,
        };
        if let Some(新值) = 新值 {
            对象.insert("修饰键布局".into(), Value::String(新值.into()));
        }
    }
    Ok(配置)
}
pub fn 读取(文件: &Path) -> Result<Value> {
    规范化(读取结构数据(文件)?)
}
/// 原文备份不会在再次升级时覆盖，用户仍可手动找回原字段。
pub fn 升级(路径: &路径集合) -> Result<()> {
    let 文件 = 路径.配置.join("config.json");
    if !文件.exists() {
        return Ok(());
    }
    let 原配置 = 读取结构数据(&文件)?;
    let 配置 = 规范化(原配置.clone())?;
    if 配置 != 原配置 {
        let 备份 = 文件.with_file_name("config.json.汉化前备份");
        if !备份.exists() {
            原子写入(路径, &备份, &fs::read(&文件)?, 0o600)?;
        }
        写入结构数据(路径, &文件, &配置)?;
    }
    Ok(())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    use serde_json::json;
    #[test]
    fn 升级保留值及外部字段且中文键优先() {
        let 配置 = 规范化(json!({"modifier_layout":"pc-swap","engine":"旧路径","引擎":"新路径","device":{"only":["键盘"]},"个人字段":true})).unwrap();
        assert_eq!(配置["修饰键布局"], "电脑换位");
        assert_eq!(配置["引擎"], "新路径");
        assert_eq!(配置["device"]["only"][0], "键盘");
        assert_eq!(配置["个人字段"], true);
        assert!(配置.get("engine").is_none());
        assert!(配置.get("modifier_layout").is_none());
        assert_eq!(规范化(配置.clone()).unwrap(), 配置);
        assert!(规范化(json!([])).is_err());
    }
}
