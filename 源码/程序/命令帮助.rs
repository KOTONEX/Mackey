// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Context, Result};
use clap::Command;

pub fn 显示(命令: &mut Command, 子命令: Option<&str>) -> Result<()> {
    let 命令 = match 子命令 {
        Some(名称) => 命令
            .find_subcommand_mut(名称)
            .filter(|命令| !命令.is_hide_set())
            .with_context(|| format!("未知子命令「{名称}」。可用命令见 `mackey 帮助`。"))?,
        None => 命令,
    };
    命令.print_long_help()?;
    println!();
    Ok(())
}

pub fn 汉化(命令: Command) -> Command {
    fn 设置(命令: Command) -> Command {
        命令
            .disable_help_flag(true)
            .disable_version_flag(true)
            .disable_help_subcommand(true)
            .subcommand_value_name("命令")
            .subcommand_help_heading("命令")
            .help_template(
                "{before-help}{about-with-newline}\n用法：{usage}\n\n{all-args}{after-help}",
            )
            .mut_subcommands(设置)
    }
    fn 翻译(mut 命令: Command) -> Command {
        let 用法 = 命令.render_usage().to_string();
        let 用法 = 用法
            .strip_prefix("Usage: ")
            .unwrap_or(&用法)
            .replace("[OPTIONS]", "[选项]");
        命令
            .override_usage(用法)
            .mut_args(|参数| {
                let 标题 = if 参数.is_positional() {
                    "参数"
                } else {
                    "选项"
                };
                let mut 帮助 = 参数.get_help().map(ToString::to_string).unwrap_or_default();
                let 默认: Vec<_> = 参数
                    .get_default_values()
                    .iter()
                    .map(|值| 值.to_string_lossy())
                    .collect();
                if 参数.get_action().takes_values() && !默认.is_empty() {
                    帮助.push_str(&format!("（默认：{}）", 默认.join("、")));
                }
                let 可选: Vec<_> = 参数
                    .get_possible_values()
                    .iter()
                    .filter(|值| !值.is_hide_set())
                    .map(|值| 值.get_name().to_owned())
                    .collect();
                if !可选.is_empty() {
                    帮助.push_str(&format!("（可选：{}）", 可选.join("、")));
                }
                参数
                    .help(帮助)
                    .help_heading(标题)
                    .hide_default_value(true)
                    .hide_possible_values(true)
            })
            .mut_subcommands(翻译)
    }
    let mut 命令 = 设置(命令);
    命令.build();
    翻译(命令)
}
