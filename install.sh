#!/usr/bin/env bash
set -euo pipefail

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"

arch=$(uname -m)
case "$arch" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) echo "不支持的架构：$arch" >&2; exit 1 ;;
esac

tag=$(curl -fsSIL -o /dev/null -w '%{url_effective}' \
  https://github.com/KOTONEX/Mackey/releases/latest | sed 's#.*/tag/##')
case "$tag" in
  v[0-9]*) version=${tag#v} ;;
  *) echo "无法解析最新版本：$tag" >&2; exit 1 ;;
esac

tarball=mackey-${version}-linux-${arch}.tar.gz
base=https://github.com/KOTONEX/Mackey/releases/download/${tag}
curl -fsSLO "$base/SHA256SUMS-${version}.txt"
curl -fsSLO "$base/$tarball"
expected=$(awk -v f="$tarball" '$2==f {print $1}' "SHA256SUMS-${version}.txt")
test -n "$expected" || { echo "校验清单缺少 $tarball" >&2; exit 1; }
echo "$expected  $tarball" | sha256sum -c -

tar -xzf "$tarball"
cd "mackey-${version}-linux-${arch}"
./mackey 安装 "$@"

