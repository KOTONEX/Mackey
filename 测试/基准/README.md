这些 JSON 来自 Rust 重构前的 Python 生成器：

```bash
python3 tools/generate.py --no-probe --out-dir tests/fixtures --docs 测试/基准/behavior.md
```

固定 Apple 布局、不探测 dconf；115 条展开后条目（89 条显式 + 26 条泛化）。
Rust 单元测试和独立二进制集成测试比较 JSON 值，避免格式与键顺序影响差分结果。
迁移计划的动态情况、截图绑定和终端/IDE 例外另由 Rust 测试覆盖。
