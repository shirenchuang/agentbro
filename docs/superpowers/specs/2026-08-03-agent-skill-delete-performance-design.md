# Agent Skill 删除性能优化设计

日期：2026-08-03
Issue：#94
状态：已确认，待实现计划

## 背景

Agent 管理页删除已管理或未管理 Skill 时，确认弹窗会保持忙碌约 1–2 秒。卡顿来自真实的同步工作，而不是卡片组件复用本身：

- 本地删除已知目标后仍会全量扫描所属 Agent 或 `~/.agents/skills`。
- 扫描会递归发现 Skill、重新计算目录 SHA-256，并逐项更新 SQLite。
- 当前机器的 `~/.agents/skills` 约 194 MB；一次等价的只读哈希约耗时 1.4 秒。
- 删除返回后，前端串行请求 `listUnmanaged → getAgentDetail → loadOverview`，而 `loadOverview` 内部又请求一次 `listUnmanaged`。
- 远端每个请求都会建立独立 SSH 进程并上传完整 Python 管理脚本。批量删除 42 个未管理 Skill 时，当前实现可能重建约 47 次完整 inventory。
- 删除流程还会触发无关的项目列表刷新。

用户确认的核心体验目标是：点击确认后弹窗立即关闭，卡片显示“删除中”；成功后消失，失败时保留并显示错误。

## 目标

1. 删除确认弹窗在同一个交互帧内关闭，目标为 100 ms 内。
2. 已知 ID 的本地删除只更新对应文件和数据库行，不全量扫描其他 Skill。
3. 远端批量删除只为 mutation 构建一次 inventory。
4. 删除完成后用一次组合请求校准 Agent detail、overview 和 unmanaged inventory。
5. 单删、批删、部分失败和后台刷新失败都保持准确、可恢复的界面状态。
6. 保留共享目录现有的路径边界、软链接脱离、哈希验证、原子替换、回滚和 owner 校验。

## 非目标

- 不做列表虚拟化。
- 不做 Zustand 全局 selector 重构。
- 不引入 SSH ControlMaster 或长连接。
- 不把快照导出改造成通用异步任务系统。
- 不改变“重新扫描此 Agent”的完整文件系统校准语义。
- 不削弱 `~/.agents/skills` 的任何安全检查。

## 选定方案

采用端到端删除快路径：

- 前端立即退出确认态，以卡片级 busy 状态表达进行中操作。
- 本地后端执行精确删除和增量数据库更新。
- 远端批量删除复用单次 inventory 快照。
- 删除结果先增量写入当前视图，再以单次组合刷新后台校准。

未采用的方案：

- 仅隐藏弹窗和卡片：只能制造“假快”，不会减少扫描、哈希和 SSH 开销。
- 全面缓存、订阅和虚拟化重构：范围和风险明显超过本次删除性能问题。

## 交互设计

### 确认与进行中

用户点击确认后：

1. 同步保存本次删除目标的不可变快照。
2. 立即关闭确认弹窗。
3. 将目标 ID 加入 `deletingIds` 或 `deletingUnmanagedIds`。
4. 卡片继续显示，但按钮禁用并显示“删除中”。
5. 页面其他区域和其他卡片保持可操作。

确认弹窗不再等待删除命令或刷新命令完成。

### 成功

删除命令返回后，按成功 ID 增量更新当前状态：

- Agent 专属已管理：从 `selectedAgentDetail.skills` 移除。
- 共享继承已管理：从 `selectedAgentDetail.inheritedManagedSkills` 移除。
- Agent 专属未管理：从全局 `unmanaged` 移除。
- 共享继承未管理：同时从全局 `unmanaged` 和 `selectedAgentDetail.inheritedUnmanagedSkills` 移除。
- 更新当前可见计数，清除对应 busy 状态。

成功项不等待 overview 或 detail 重载即可消失。

### 部分失败

- 成功项消失。
- 失败项保留并恢复正常操作状态。
- 批量选择只保留失败项，便于重试。
- 错误提示包含 Skill 名称和后端返回的具体原因。
- 不重新打开确认弹窗。

### 后台刷新失败

增量 UI 更新完成后启动组合刷新。第一次失败时自动重试一次；第二次仍失败时：

- 不撤销已经成功的删除。
- 显示“删除成功，但列表刷新失败”的错误。
- 保留手动“刷新总览”与“重新扫描此 Agent”作为恢复入口。

### 切换 Agent 或运行环境

每次 mutation 和 reconciliation 捕获：

- `runtimeEnvironmentId`
- `agentId`
- 单调递增的请求 generation

响应只在运行环境、Agent 和 generation 仍匹配时写入当前选中详情。旧响应不得覆盖用户后来切换到的页面。

## 前端架构

### 删除动作

`SkillsTab` 继续复用现有 managed/unmanaged collection。删除处理统一为三个阶段：

1. `beginDelete(targets)`：关闭弹窗并标记 busy。
2. `executeDelete(targets)`：调用批量删除 API；单删也走批量 API，以获得一致的成功/失败结果。
3. `applyDeleteResult(result)`：增量更新 Store，随后启动后台 reconciliation。

不新增共享专用卡片或共享专用列表。

### 组合刷新 DTO

新增 Agent Skill 视图快照：

```ts
interface AgentSkillViewSnapshot {
  agentDetail: AgentDetail
  overview: SkillOverview
  unmanaged: UnmanagedItemDto[]
}
```

新增 API：

```ts
refreshAgentSkillView(agentId: string): Promise<AgentSkillViewSnapshot>
```

Store 通过一次 `set` 原子更新：

- `selectedAgentDetail`
- `overview`
- `skills`
- `agents`
- `packs`
- `issues`
- `settings`
- `unmanaged`
- `lastOverviewLoadedAt`

这个动作不调用 `loadProjects()`，因为 Skill 删除不会改变项目列表。

### 后台校准

删除结果应用后，异步调用 `refreshAgentSkillView(agentId)`：

- 不阻塞删除完成提示。
- 不重新设置整页 loading。
- 一次失败后重试一次。
- 使用运行环境、Agent 和 generation guard。

现有 `refreshAgentSkills()` 中重复、串行的读取将不再用于删除路径。

## 本地 Rust 后端

### 已管理删除

`delete_skill_target_distribution(s)` 已经知道 target ID、owner 和 target path。成功路径调整为：

1. 校验 target 和共享路径边界。
2. 精确删除文件、目录或叶子软链接。
3. 删除 `skill_targets` 行；claims 继续由外键级联删除。
4. 精确清理同 owner、同 path 的陈旧 `unmanaged_items` 行。
5. 不调用 `scan_one_agent_into_db`。
6. 只有 `deleted > 0` 时导出一次 recovery snapshot。

显式重新扫描仍负责发现与本次 mutation 无关的外部文件变化。

### 未管理删除

`delete_unmanaged_agent_skill(s)` 成功路径调整为：

1. 校验 unmanaged ID、owner、路径边界和共享安全约束。
2. 精确删除目标文件、目录或叶子软链接。
3. 删除对应 `unmanaged_items` 行。
4. 不调用 `scan_one_agent_into_db`。
5. 不导出 recovery snapshot。

`unmanaged_items` 当前不包含在 recovery snapshot 中，因此删除未管理项时重写完整快照没有恢复价值。

### 组合读取

新增 Tauri command：

```text
refresh_agent_skill_view_v2(agent_id)
```

本地实现从 SQLite 读取 overview、Agent detail 和 unmanaged inventory，不触发文件系统扫描。命令返回统一的 `AgentSkillViewSnapshot`。

## 远端后端

### 批量未管理删除

当前实现会为每个 unmanaged ID 调用一次 `find_unmanaged()`，从而重复构建 inventory。新流程：

1. 命令开始时构建一次 inventory。
2. 建立 `unmanaged_id → item` 映射。
3. 逐项使用快照查找目标。
4. 每项仍独立执行 owner、路径和共享安全校验。
5. 返回 `deleted` 和逐项 `failures`。

快照只减少发现开销，不跳过 mutation 时的实时路径安全验证。

### 组合读取

新增远端 command，与本地使用相同 DTO。实现只调用一次 `inventory()`，然后从该快照派生：

- overview
- selected Agent detail
- unmanaged inventory

为避免隐藏的重复扫描，相关构造函数拆为接收既有 inventory 的纯投影 helper：

- `overview_from_inventory(...)`
- `agent_detail_from_inventory(...)`
- `unmanaged_inventory_from_inventory(...)`

`agent_detail` 构造 available packs 时不得再次调用 `overview()`。

### 安全逻辑

以下逻辑保持原样：

- `shared_skill_mutation_path`
- center symlink dependent 发现与安全脱离
- 哈希与 `SKILL.md` 验证
- 同文件系统 staging、backup、原子替换和回滚
- 回滚失败时保留 recovery artifacts
- 普通与共享 owner 严格匹配

安全路径可能因真实目录复制而耗时，但不能为性能目标省略。

## 性能预算

### 前台体验

- 确认弹窗关闭：目标小于 100 ms。
- 页面在 mutation 期间保持响应。
- 普通软链接或小目录删除不再包含全量扫描时间。
- 大型真实目录的物理删除允许继续耗时，但只影响对应卡片的 busy 状态。

### 本地后端

- 已知 ID 删除不调用 `scan_one_agent_into_db`。
- 未管理删除不写 recovery snapshot。
- 已管理批量删除最多写一次 snapshot，且 `deleted == 0` 时不写。

### 远端后端

- N 项批量未管理删除：一次 mutation inventory，不随 N 线性增加 inventory 次数。
- 删除后的 reconciliation：一次 inventory。
- 42 项批量删除的完整前台/校准流程总计最多两次 inventory。
- 删除路径不触发项目列表请求。

## 错误语义

- 文件系统删除失败时不得删除数据库行。
- 数据库删除失败时返回明确错误并保留失败项；DB-only reconciliation 不得假定磁盘已经回滚，错误提示应引导用户执行“重新扫描此 Agent”以校准真实文件状态。
- shared 安全校验失败时目标和恢复副本必须保留。
- mutation 成功、reconciliation 失败时以 mutation 结果为准，不伪装成删除失败。
- 批量 API 必须保留逐项失败，不因一个失败回滚已经成功的独立删除。

## 测试设计

### 前端

1. 点击确认后，在删除 Promise 未完成时弹窗已经关闭。
2. 未完成项显示 busy 且不可重复操作。
3. managed 单删成功后立即从正确的专属或共享集合移除。
4. unmanaged 单删成功后同时更新全局和共享继承集合。
5. 批量部分失败时成功项消失、失败项保留并保持选择。
6. reconciliation 只调用一次组合 API，不调用旧的串行刷新链路或项目刷新。
7. 第一次 reconciliation 失败会重试一次；第二次失败显示准确提示。
8. 切换 Agent 或运行环境后，旧响应不覆盖新详情。

### 本地 Rust

1. managed 删除保留无关的陈旧 inventory 记录，证明没有隐式全量扫描。
2. managed 删除精确清理同 owner、同 path 的陈旧 unmanaged 行。
3. unmanaged 删除保留其他 inventory 行且不触发全量扫描。
4. unmanaged 删除前后 snapshot 内容和修改时间不变。
5. managed `deleted == 0` 不更新 snapshot；成功 batch 只更新一次。
6. shared root、parent symlink、leaf symlink、outside path 和 traversal 回归测试继续通过。

### 远端

1. 42 项批量未管理删除只调用一次 inventory。
2. mixed owner、stale ID 和部分失败仍返回逐项结果。
3. 组合刷新只调用一次 inventory。
4. Agent detail 的 pack 投影不触发第二次 inventory。
5. 共享同名、别名、链式软链接、symlink root、原子替换与 rollback fault tests 全部继续通过。

### 完整门禁

- `pnpm lint`
- `pnpm test:run`
- `pnpm build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- PR CI 的 `cargo fmt --check`、严格 clippy 和全量 `cargo test`

## 交付边界

本 Issue 完成时应同时交付本地与远端快路径、组合刷新、交互状态和回归测试。若性能数据表明剩余卡顿来自卡片重渲染或超大目录物理删除，另开独立 Issue；本任务不顺带进行 Store selector 或虚拟化重构。
