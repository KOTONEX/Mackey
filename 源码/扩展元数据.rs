// SPDX-License-Identifier: AGPL-3.0-or-later
/// GitHub 自分发包补齐管理器需要的整数版本，显示值仍采用 Cargo 版本。
pub fn 生成(原始: &[u8], 版本: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if 版本.is_empty()
        || 版本.len() > 16
        || !版本
            .bytes()
            .all(|字节| 字节.is_ascii_alphanumeric() || b" .".contains(&字节))
        || !版本.bytes().any(|字节| 字节.is_ascii_alphanumeric())
    {
        return Err(
            "项目版本不符合 GNOME version-name 格式：1 至 16 个字母、数字、空格或点".into(),
        );
    }
    let 分段: Vec<u64> = 版本.split('.').map(str::parse).collect::<Result<_, _>>()?;
    let [主版, 次版, 补丁] = 分段.as_slice() else {
        return Err("扩展版本必须包含主版本、次版本和补丁版本三个整数".into());
    };
    if *次版 >= 1000 || *补丁 >= 1000 {
        return Err("扩展整数版本编码要求次版本和补丁版本均小于 1000".into());
    }
    // 每段预留三位，避免跨主版本时排序颠倒；管理器读取为有符号 32 位整数。
    let 整数版本 = 主版
        .checked_mul(1_000_000)
        .and_then(|数值| 数值.checked_add(次版 * 1000))
        .and_then(|数值| 数值.checked_add(*补丁))
        .and_then(|数值| i32::try_from(数值).ok())
        .filter(|数值| *数值 > 0)
        .ok_or("扩展整数版本必须处于 1 至 2147483647 范围")?;
    let mut 元数据: serde_json::Value = serde_json::from_slice(原始)?;
    let 对象 = 元数据.as_object_mut().ok_or("扩展元数据必须是 JSON 对象")?;
    对象.insert("version".into(), 整数版本.into());
    对象.insert("version-name".into(), 版本.into());
    Ok(serde_json::to_vec_pretty(&元数据)?)
}

#[cfg(test)]
mod 测试 {
    use super::生成;

    #[test]
    fn 显示版本与兼容整数均由项目版本生成() {
        let 元数据: serde_json::Value = serde_json::from_slice(
            &生成(
                br#"{"uuid":"test","version":9,"version-name":"old"}"#,
                "2.4.2",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(元数据["version-name"], "2.4.2");
        assert_eq!(元数据["version"], 2_004_002);
        assert_eq!(元数据["uuid"], "test");
    }

    #[test]
    fn 编码跨版本保持递增且允许零次版本() {
        let 编码 = |版本| {
            let 元数据: serde_json::Value =
                serde_json::from_slice(&生成(b"{}", 版本).unwrap()).unwrap();
            元数据["version"].as_i64().unwrap()
        };
        assert_eq!(编码("0.1.0"), 1000);
        assert!(编码("1.999.999") < 编码("2.0.0"));
        assert!(编码("2.4.1") < 编码("2.4.2"));
    }

    #[test]
    fn 拒绝歧义编码整数溢出及不合法元数据() {
        for 版本 in [
            "",
            "0.0.0",
            "1.1000.0",
            "1.0.1000",
            "2148.0.0",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc.1",
            "18446744073709551615.0.0",
        ] {
            assert!(生成(b"{}", 版本).is_err(), "不应接受 {版本}");
        }
        assert!(生成(b"[]", "2.4.2").is_err());
        assert!(生成(b"not-json", "2.4.2").is_err());
    }
}
