#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# 解压真实发行包，在隔离 HOME 下验证独立二进制与嵌入资源。
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE="${1:?请指定发行包}"
WORK=$(mktemp -d "$HOME/.mackey-package.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/home" "$WORK/extracted"
tar -xzf "$ARCHIVE" -C "$WORK/extracted"
PACKAGE=$(find "$WORK/extracted" -mindepth 1 -maxdepth 1 -type d)
test -x "$PACKAGE/mackey"
test -s "$PACKAGE/LICENSE"
test -s "$PACKAGE/LICENSES/依赖许可证.tsv"
test -s "$PACKAGE/CHANGELOG.md"
test -s "$PACKAGE/发布说明.md"
EXPECTED=$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --locked --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "mackey") | .version')
ACTUAL=$("$PACKAGE/mackey" --version)
test "$ACTUAL" = "mackey $EXPECTED"
HOME="$WORK/home" "$PACKAGE/mackey" 生成 --不探测 \
    --用户配置 "$ROOT/测试/基准/用户配置.json" \
    --输出目录 "$WORK/home/generated" --文档 "$WORK/home/行为清单.md"
jq -S . "$ROOT/测试/基准/xremap.json" > "$WORK/expected.json"
jq -S . "$WORK/home/generated/xremap.json" > "$WORK/actual.json"
cmp "$WORK/expected.json" "$WORK/actual.json"
test -s "$WORK/home/行为清单.md"
echo '✓ 发行包解压验证通过：版本、许可、嵌入清单与旧版输出一致'
