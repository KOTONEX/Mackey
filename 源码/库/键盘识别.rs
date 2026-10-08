// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn 识别(text: &str) -> Value {
    let noise=regex::Regex::new("(?i)button|video bus|speaker|hdmi|headphone|mic|touchpad|mouse|lid switch|hid events|avrcp|consumer control|wireless radio").unwrap();
    let builtin=regex::Regex::new("(?i)AT Translated Set 2 keyboard|ThinkPad|Intel HID|Sony|Dell WMI|Lenovo|Asus|Acer|MSI|Toshiba|HP WMI").unwrap();
    let apple_name = regex::Regex::new("(?i)apple|magic keyboard|macbook").unwrap();
    let vendor_re = regex::Regex::new("(?i)Vendor=([0-9a-f]{4})").unwrap();
    let mut devices = BTreeMap::<String, (i32, String)>::new();
    for block in text.split("\n\n") {
        let mut name = String::new();
        let mut vendor = String::new();
        let mut 键盘识别 = false;
        let mut score = 5;
        for line in block.lines() {
            if let Some(n) = line.strip_prefix("N: Name=") {
                name = n.trim().trim_matches('"').to_owned();
            }
            if let Some(c) = vendor_re.captures(line) {
                vendor = c[1].to_lowercase();
            }
            if line.starts_with("H:") {
                键盘识别 = line
                    .replace("Handlers=", "")
                    .split_whitespace()
                    .any(|s| s == "kbd");
            }
            if line.starts_with("B: EV=")
                && ["120013", "100013", "12001b"]
                    .iter()
                    .any(|m| line.ends_with(m))
            {
                score += 10;
            }
            if let Some(bits) = line.strip_prefix("B: KEY=") {
                score += bits.split_whitespace().count().min(20) as i32;
            }
        }
        if !键盘识别 || name.is_empty() || noise.is_match(&name) {
            continue;
        }
        let entry = devices.entry(name).or_insert((score, vendor.clone()));
        if score > entry.0 {
            *entry = (score, vendor);
        }
    }
    let mut devices: Vec<_> = devices.into_iter().collect();
    devices.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    let primary = devices
        .iter()
        .find(|(n, _)| !builtin.is_match(n))
        .or(devices.first());
    let Some((name, (_, vendor))) = primary else {
        return json!({"名称":"","厂商":"","建议布局":"微软"});
    };
    let apple = vendor == "05ac" || apple_name.is_match(name);
    json!({"名称":name,"厂商":vendor,"键盘列表":devices.iter().map(|d|&d.0).collect::<Vec<_>>(),"苹果键盘":apple,"建议布局":if apple{"苹果"}else{"微软"}})
}
pub fn 当前键盘() -> Value {
    识别(&std::fs::read_to_string("/proc/bus/input/devices").unwrap_or_default())
}
#[cfg(test)]
mod 测试 {
    use super::*;
    #[test]
    fn 外接苹果键盘优先且忽略噪声() {
        let text = "I: Vendor=0001\nN: Name=\"AT Translated Set 2 keyboard\"\nH: Handlers=kbd event1\n\nI: Vendor=05ac\nN: Name=\"Magic Keyboard\"\nH: Handlers=kbd event2\n\nN: Name=\"Power Button\"\nH: Handlers=kbd event0\n";
        assert_eq!(识别(text)["建议布局"], "苹果");
        assert_eq!(识别("")["建议布局"], "微软");
    }
}
