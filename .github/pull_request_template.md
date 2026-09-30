## 变更内容

<!-- 简述这个 PR 做了什么、为什么。若关联 issue，请写 "Closes #123"。 -->

## 类型

- [ ] `新增` 新功能
- [ ] `修复` 缺陷修复
- [ ] `文档` 文档
- [ ] `测试` 测试
- [ ] `重构` 重构
- [ ] `杂务` 杂项
- [ ] `初始化` 初始提交

## 自测

- [ ] `make lint` 通过
- [ ] `make check` 通过
- [ ] `make test` 通过（离线单元测试）
- [ ] 条件允许时 `make test-contract` / `make test-e2e` 通过（未运行时请在下方说明原因：离线 / 无 GNOME 会话 / 缺 `input` 组等）

## 约束检查

- [ ] 没有写入 `$HOME` 之外的路径，未使用 `sudo`，未以 root 运行
- [ ] 行为变更只改了 `config/checklist.json`，并已重新运行 `make generate`
- [ ] 未手工编辑自动生成文件（`docs/03-行为清单.md`）
