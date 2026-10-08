# SPDX-License-Identifier: AGPL-3.0-or-later
.DEFAULT_GOAL := 帮助
.PHONY: 帮助 构建 发行构建 检查 格式化 类型检查 校验 测试 扩展测试 端到端测试 全部测试 打包 变更日志 发布说明
帮助:
	@cargo run --quiet --locked -- --help
构建:
	cargo build --locked
发行构建:
	cargo build --locked --release
检查 格式化 类型检查 校验 测试 扩展测试 端到端测试 全部测试 打包 变更日志 发布说明:
	cargo run --quiet --locked -- $@
