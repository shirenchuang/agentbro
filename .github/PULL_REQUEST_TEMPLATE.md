## 变更内容

<!-- 用 1-3 句话说明这个 PR 做了什么。 -->

## 背景与动机

Closes #<!-- 必填：填写对应 Issue 编号，例如 Closes #61 -->

## 实现方案

<!-- 说明关键实现、取舍、兼容性影响，或 reviewer 需要重点看的地方。 -->

## 验证方式

<!-- 列出已运行的命令、手动验证步骤，或说明暂未验证的原因。 -->

## 截图 / 录屏

<!-- UI 改动请附截图或录屏；非 UI 改动可写 N/A。 -->

## Checklist

- [ ] PR 目标分支是 `dev`，不是 `main`
- [ ] PR 已通过 `Closes #<编号>` 关联对应 Issue
- [ ] `pnpm lint` 已通过
- [ ] `pnpm test:run` 已通过
- [ ] `pnpm build` 已通过
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 已通过
- [ ] 如涉及文案，已同步更新 5 份语言文件（`en`、`zh`、`ja`、`ko`、`tr`）
- [ ] 未修改版本号（release 由 maintainer 统一处理）
