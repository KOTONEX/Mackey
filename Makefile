# SPDX-License-Identifier: AGPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Mackey contributors
SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

.PHONY: help lint typecheck check generate fetch-engine install uninstall enable disable status doctor \
        test test-all test-contract test-e2e logs clean

help:
	@printf '%s\n' \
	  'Mackey 开发命令' \
	  '' \
	  '  make lint            静态检查（bash -n / py_compile / shellcheck / checklist.json + typecheck）' \
	  '  make typecheck       类型注解检查：mypy（Python tools）+ tsc（extension.js / 契约测试）' \
	  '  make check           离线跑生成器，校验键名与配置组装' \
	  '  make generate        依据本机 dconf 重新生成配置与 docs/03-行为清单.md' \
	  '  make fetch-engine    下载与指令集架构/桌面匹配的 xremap 最新发布版本（存到 XDG 数据目录）' \
	  '  make test-contract   扩展契约测试（需 gjs + D-Bus 会话）' \
	  '  make test-e2e        端到端「焦点桥 → 按应用分流」（需 input 组 + /dev/uinput）' \
	  '  make test            离线单元测试：focusd + 生成器 + 引擎下载（不连 D-Bus，CI 可跑）' \
	  '  make test-all        三套自测全跑（./bin/mackey test）' \
	  '  make install         安装扩展与 systemd 用户服务（全部在 $$HOME 内）' \
	  '  make uninstall       卸载并还原' \
	  '  make enable          启动服务并启用扩展' \
	  '  make disable         停用（键盘回到 Linux 原生行为）' \
	  '  make status          当前状态一览' \
	  '  make doctor          环境体检' \
	  '  make logs            查看服务日志' \
	  '  make clean           清理本地产物（__pycache__ 等）'

lint: typecheck
	bash -n bin/mackey
	@for f in install.sh uninstall.sh tests/*.sh; do bash -n "$$f" || exit 1; done
	python3 -m py_compile tools/generate.py tools/focusd.py tools/detect-keyboard.py tools/fetch-engine.py tests/fake_keyboard.py tests/test_focusd.py tests/test_generate.py tests/test_fetch_engine.py
	@for f in config/checklist.json extension/*/metadata.json .vscode/extensions.json .vscode/settings.json; do \
	    jq -e . "$$f" > /dev/null || exit 1; \
	done
	@if command -v shellcheck > /dev/null 2>&1; then \
	    shellcheck -S warning bin/mackey install.sh uninstall.sh tests/*.sh; \
	else \
	    printf '%s\n' '（未安装 shellcheck，跳过；安装后可用 apt/dnf/pacman install shellcheck）'; \
	fi
	@printf '%s\n' '✓ lint 通过'

# 默认宽松：缺 mypy/tsc 时跳过并提示（与 shellcheck 的处理一致）。
# CI 用 MACKEY_TYPECHECK_STRICT=1 把「工具缺失」变成失败，避免静默退化。
typecheck:
	@missing=""; \
	if python3 -c 'import mypy' > /dev/null 2>&1; then \
	    python3 -m mypy || exit 1; \
	else \
	    missing="$${missing}mypy "; \
	    printf '%s\n' '（未安装 mypy，跳过 Python 类型检查；安装：pip install mypy）'; \
	fi; \
	if command -v tsc > /dev/null 2>&1; then \
	    tsc -p tsconfig.json || exit 1; \
	else \
	    missing="$${missing}tsc "; \
	    printf '%s\n' '（未安装 tsc，跳过 JS 类型检查；安装：npm i -g typescript）'; \
	fi; \
	if [ -n "$${missing}" ] && [ "$${MACKEY_TYPECHECK_STRICT:-0}" = "1" ]; then \
	    printf '%s\n' "✗ strict 模式：类型检查工具缺失（$${missing}）" >&2; \
	    exit 1; \
	fi; \
	printf '%s\n' '✓ 类型检查通过'

check:
	@tmp=$$(mktemp -d); trap 'rm -rf "$$tmp"' EXIT; \
	python3 tools/generate.py --no-probe --out-dir "$$tmp" --docs "$$tmp/03-行为清单.md" > /dev/null && \
	test -s "$$tmp/xremap.json" && test -s "$$tmp/relocations.json" && \
	printf '%s\n' '✓ 生成器自检通过'

generate:
	python3 tools/generate.py

fetch-engine:
	./bin/mackey fetch-engine

install:
	./bin/mackey install

uninstall:
	./bin/mackey uninstall

enable:
	./bin/mackey enable

disable:
	./bin/mackey disable

status:
	./bin/mackey status

doctor:
	./bin/mackey doctor

test:
	python3 tests/test_focusd.py
	python3 tests/test_generate.py
	python3 tests/test_fetch_engine.py
	bash tests/test-uninstall-cleanup.sh
	bash tests/test-install-idempotency.sh

test-all:
	./bin/mackey test

test-contract:
	bash tests/verify-extension.sh

test-e2e:
	@cfg="$${XDG_CONFIG_HOME:-$$HOME/.config}/mackey/config.json"; \
	engine=""; \
	for cand in \
	    "$${MACKEY_ENGINE:-}" \
	    "$$(if [ -f "$$cfg" ]; then python3 -c 'import json,sys; v=json.load(open(sys.argv[1],encoding="utf-8")).get("engine",""); print(v if isinstance(v,str) else "")' "$$cfg" 2>/dev/null; fi || true)" \
	    "$${XDG_DATA_HOME:-$$HOME/.local/share}/mackey/bin/xremap" \
	    "./.vendor/xremap" \
	    "$$(command -v xremap 2>/dev/null || true)"; do \
	    if [ -n "$$cand" ] && [ -x "$$cand" ]; then engine="$$cand"; break; fi; \
	done; \
	if [ -z "$$engine" ]; then \
	    printf '%s\n' '找不到可执行的 xremap 引擎；先 make fetch-engine 或设置 MACKEY_ENGINE=...' >&2; \
	    exit 1; \
	fi; \
	printf '%s\n' "==> 使用引擎：$$engine"; \
	MACKEY_ENGINE="$$engine" bash tests/e2e-app-match.sh && \
	MACKEY_ENGINE="$$engine" bash tests/e2e-swallow-exempt.sh

logs:
	./bin/mackey logs

clean:
	rm -rf tools/__pycache__ tests/__pycache__ __pycache__ .mypy_cache
	rm -f tools/*.pyc tests/*.pyc *.pyc
	@printf '%s\n' '✓ 已清理本地产物'
