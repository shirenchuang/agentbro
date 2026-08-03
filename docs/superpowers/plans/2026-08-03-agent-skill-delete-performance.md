# Agent Skill 删除性能优化实施计划

日期：2026-08-03  
Issue：#94  
设计：`docs/superpowers/specs/2026-08-03-agent-skill-delete-performance-design.md`

## 交付目标

在不改变共享 Skill 安全语义的前提下，让删除确认弹窗立即关闭；本地删除不再全量扫描，远端批量删除只构建一次 inventory，并用一次组合请求完成后台校准。

## 任务 1：建立组合刷新数据契约

涉及文件：

- `src-tauri/src/skills/v2/models.rs`
- `src-tauri/src/skills/v2/commands.rs`
- `src-tauri/src/skills/v2/service.rs`
- `src-tauri/src/lib.rs`
- `src/services/skillApiV2.ts`

步骤：

1. 新增 `AgentSkillViewSnapshot`，包含 Agent detail、overview 和 unmanaged inventory。
2. 新增 `refresh_agent_skill_view_v2(agent_id)` Tauri command 并注册。
3. 本地 service 仅从 SQLite 投影快照，不触发扫描或项目加载。
4. 前端 API 暴露 `refreshAgentSkillView(agentId)`，类型与 Rust DTO 对齐。

验证：新增 Rust/TypeScript 测试，确认组合读取内容一致且不改变数据库。

## 任务 2：实现本地精确删除快路径

涉及文件：

- `src-tauri/src/skills/v2/service.rs`
- `src-tauri/src/skills/v2/db.rs`
- `src-tauri/src/skills/v2/tests.rs`

步骤：

1. managed 单删/批删成功后只移除精确 target 和同 owner、同 path 的陈旧 unmanaged 行。
2. 删除 managed 路径中的 `scan_one_agent_into_db`。
3. managed 只有实际删除成功时写 recovery snapshot；批量最多写一次。
4. unmanaged 单删/批删只删除精确文件和 DB 行，不扫描、不写 snapshot。
5. 保留文件系统失败不删 DB、owner/path/shared 边界校验和逐项失败语义。

验证：覆盖无关陈旧记录保留、同路径记录清理、snapshot 修改时间、零成功批次和 shared 安全回归。

## 任务 3：实现远端单次 inventory 快路径

涉及文件：

- `src-tauri/src/remote/skill_manager.py`
- `src-tauri/src/remote/skill_manager.rs`

步骤：

1. 将 overview、agent detail、unmanaged 列表拆成接收既有 inventory 的投影 helper。
2. `agent_detail` 的 pack 投影复用同一 inventory，不再嵌套调用 overview。
3. 新增远端组合刷新 command，一次 inventory 返回完整快照。
4. 批量 unmanaged 删除在入口只建立一次 ID map；逐项继续执行实时 owner/path/shared 安全检查。
5. 保留共享软链接脱离、哈希、原子替换和回滚实现。

验证：脚本测试统计 inventory 调用次数；42 项批删为 1 次，组合刷新为 1 次，并覆盖部分失败。

## 任务 4：实现前端即时关闭与增量状态更新

涉及文件：

- `src/stores/skillStoreV2.ts`
- `src/components/skills-v2/AgentManagementPage.tsx`
- `src/services/skillApiV2.ts`
- `src/i18n/locales/{en,zh,ja,ko,tr}.json`

步骤：

1. Store 新增原子应用组合快照与删除结果的动作。
2. 删除确认先同步保存目标、关闭弹窗并标记 busy，再发起异步 mutation。
3. 单删统一走批量 API，成功 ID 从正确的 managed/shared/unmanaged 集合移除。
4. 部分失败只保留失败项与选择，并显示逐项原因。
5. 成功后非阻塞调用组合刷新；首次失败自动重试一次，第二次失败提示但不撤销删除。
6. 使用环境 ID、Agent ID 和 generation 防止旧响应覆盖新页面。
7. 删除路径不再调用旧的 `refreshAgentSkills` 串行链路或项目刷新。

验证：组件测试在 mutation Promise 未完成时确认弹窗已关闭、卡片 disabled；覆盖成功、部分失败、重试和 stale guard。

## 任务 5：回归与交付

1. 运行相关 Rust 和 Vitest 用例。
2. 使用 `source-command-check` 依次执行：
   - `pnpm lint`
   - `pnpm test:run`
   - `pnpm build`
   - `cargo check --manifest-path src-tauri/Cargo.toml`
3. 复核 diff、生成物和敏感信息，创建 Conventional Commit。
4. 推送 `codex/issue-94-speed-up-skill-delete`，创建指向 `dev` 的 PR，正文包含 `Closes #94`。
5. 开启 squash auto-merge 和删除分支，监控 CI，确认 PR 合并且 Issue 关闭。

## 完成标准

- 弹窗关闭不等待 mutation Promise。
- 本地已知 ID 删除不触发全量扫描。
- unmanaged 删除不写 recovery snapshot。
- 远端 42 项批删 mutation 只构建一次 inventory。
- 后台校准只发一次组合请求，不刷新项目。
- 所有共享安全回归和必需校验通过。
