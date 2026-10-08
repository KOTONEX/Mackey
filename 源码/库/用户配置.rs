// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::路径::读取结构数据;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::path::Path;

const 重装提示: &str = "旧配置不受支持。请先在旧版本仓库运行 ./uninstall.sh --yes（Python 版）或 ./卸载.sh --确认执行（旧 Rust 版），还原键位并清除旧配置，再运行新版 ./安装.sh";

pub fn 校验(配置: Value) -> Result<Value> {
    let 对象 = 配置
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("用户配置必须是 JSON 对象"))?;
    for 键 in [
        "modifier_layout",
        "engine",
        "_modifier_layout_说明",
        "_engine_说明",
    ] {
        ensure!(!对象.contains_key(键), "{重装提示}（旧字段：{键}）");
    }
    if let Some(值) = 对象.get("修饰键布局") {
        ensure!(
            matches!(值.as_str(), Some("苹果" | "微软" | "自动")),
            "{重装提示}（修饰键布局仅接受：苹果、微软、自动）"
        );
    }
    if let Some(值) = 对象.get("引擎") {
        ensure!(值.is_string(), "引擎路径必须是字符串");
    }
    Ok(配置)
}
pub fn 读取(文件: &Path) -> Result<Value> {
    校验(读取结构数据(文件)?)
}
#[cfg(test)]
mod 测试 {
    use super::*;
    use serde_json::json;
    #[test]
    fn 仅接受新接口且保留外部字段() {
        let 配置 =
            json!({"修饰键布局":"微软","引擎":"新路径","device":{"only":["键盘"]},"个人字段":true});
        assert_eq!(校验(配置.clone()).unwrap(), 配置);
        assert!(校验(json!({})).is_ok());
        assert!(校验(json!([])).is_err());
        for 布局 in ["apple", "pc-swap", "auto", "电脑换位"] {
            let 错误 = 校验(json!({"修饰键布局":布局})).unwrap_err().to_string();
            assert!(错误.contains("卸载"));
        }
        for 键 in [
            "modifier_layout",
            "engine",
            "_modifier_layout_说明",
            "_engine_说明",
        ] {
            assert!(校验(json!({键:"旧值"})).is_err());
        }
    }
}
