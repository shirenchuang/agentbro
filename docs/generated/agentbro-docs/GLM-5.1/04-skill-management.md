# 04 — Skill 管理现状

> 评测模型：GLM-5.1 · 生成日期：2026-06-13
> 基于 `src-tauri/src/skills/*`、`src/services/skillApi.ts`、`src/stores/skillStore.ts`、`src/components/skills/*`、`src/components/settings/sections/{SkillsSection,AgentsSection,SettingsApp,SettingsSidebar}.tsx` 实读。

---

## 4.1 结论先行

**Skill 管理在当前 `dev` 分支是完整且可达的子系统**，覆盖扫描/安装/市场/集合/包/同步/插件/MCP/Obsidian/GitHub 导入/AI 解释。这与 README.md:34「Skills 管理暂时不会出现在公开版菜单」**不符**——实际它挂在「agents」设置项下。

> ⚠️ 入口发现（确认）：`SettingsApp.tsx:139-165` 不渲染独立 `SkillsSection`（`SkillsSection.tsx` 存在但**是死代码**，仅被自身 CSS 引用）。真正的入口是 `activeSection === 'agents'` → `AgentsSection`，其按 `activeView` 切换技能子视图（`AgentsSection.tsx:769-782`）：`central`/`skills`/`plugins`/`profiles`/`discover`/`market`/`sync`。`CollectionsView` 与 `ObsidianView` 只在死代码 `SkillsSection.tsx` 里出现（Obsidian 的扫描逻辑仍能经 `DiscoverView` 间接触达）。

---

## 4.2 能力清单（按文件:行确认）

| 能力 | 后端 | 前端 |
| --- | --- | --- |
| 扫描（按 Agent / 全部） | `skills/scanner.rs:10-50`；命令 `scan_all_skills`/`scan_agent_skills`（`lib.rs:2481-2490`） | `skillApi.scanAll/scanAgent` |
| 中央技能库（`~/.agents/skills` + legacy `.agentbro/skills`） | `skills/agent_paths.rs:160-170`；`get_central_skill_bundles*`（`lib.rs:2492-2582`） | `CentralSkillListView.tsx` |
| 项目发现（深度 8、上限 400） | `skills/scanner.rs:52-74`；`discover_*_cmd`（`lib.rs:2584-2637`） | `DiscoverView.tsx` |
| 扫描根（默认 `~/code`、`~/projects`、`~/workspace`、`~/Documents`） | `skills/registry.rs:254-286,453-462` | `DiscoverView`（`getScanRoots/setScanRoots`） |
| Obsidian vault 发现（深度 ≤6、上限 100） | `skills/scanner.rs:76-152`；`get_obsidian_*_cmd` | `ObsidianView.tsx`（仅死代码路径渲染） |
| 安装（本地路径/github URL/raw .md/.zip；symlink/direct） | `skills/installer.rs:11-149`；`install_skill_cmd`（`lib.rs:2641-2675`） | `InstallDialog.tsx`、`skillApi.install` |
| GitHub repo 导入（逐 skill 冲突预览 + overwrite/skip/rename） | `skills/installer.rs:102-119`；`preview_github_repo_import`/`import_github_repo_skills`（`lib.rs:2684-2801`） | `InstallDialog` |
| 批量导入已发现 skill | `batch_import_discovered_skills_cmd`（`lib.rs:2653-2675`） | `DiscoverView` |
| 插件（`.claude-plugin`/`.codex-plugin`，仅 claude-code/codex） | `skills/installer.rs:290-329`；`scanner.rs:546-704` | `PluginListView.tsx`、`PackDialog` |
| MCP server（upsert/remove/validate/toggle） | `skills/installer.rs:742-843,774-813` | `PluginListView`、`skillApi.upsertMcpServer` |
| 启用/禁用 toggle（写 `disabledSkills[]`） | `skills/installer.rs:681-740` | `skillApi.toggle` |
| 文件树/内容查看 | `skills/scanner.rs:798-842`；`read_skill_files/read_skill_file_content`（`lib.rs:3018-3080`） | `FileTreeViewer.tsx`、`FileContentViewer.tsx`、`SkillDetailSlider.tsx` |
| AI 解释（4 段：用途/何时用/要求/风险，OpenAI 兼容 + 本地回退） | `skills/explanation.rs:19-185` | `SkillDetailSlider`（`get/generateSkillExplanation`） |
| 包 SkillPack（skill 集 + 目标 Agent，apply） | `skills/registry.rs:134-159`；`installer.rs:920-948`；`list/create/update/delete_pack_cmd`、`apply_pack_cmd` | `PackListView.tsx`、`PackDialog.tsx`、`PackCard.tsx` |
| 集合 Collection（CRUD + export/import JSON + 批量安装） | `skills/registry.rs:161-252`；`lib.rs:3082-3147` | `CollectionsView.tsx`（**UI 仅死代码可达**） |
| 同步（GitHub push/pull + 冲突解决 keep_local/use_remote/keep_both；Agent↔Agent；zip 备份） | `skills/sync.rs:8-462`；`lib.rs:3149-3253` | `SyncView.tsx` |
| 市场（内置 6 项目录 + 自定义 JSON registry + search/sync） | `skills/marketplace.rs:48-387`；`lib.rs:3189-3262` | `MarketplaceView.tsx`（合并 `src/data/officialSources.ts`） |
| 注册表元数据 | `get_registry_metadata`（`lib.rs:3189`）返回完整 `Metadata` | `skillApi.getMetadata` |

---

## 4.3 前端结构

- **Store**：`src/stores/skillStore.ts`（`useSkillStore`，`:50`）。状态含 `skills/packs/collections/scanRoots/obsidianVaults/syncConfig/loading/scanning/activeTab/selectedSkillId/fileTree/searchQuery/typeFilter/agentFilter/batchMode`。`loadAll()` 并行 `scanAll`+`getMetadata`，按 id 合并去重 agents（`:69-136`）。**无自身持久化**——真相源在后端 `~/.agentbro/metadata.json`（`agent_paths.rs:172-174`）。
- **Service**：`src/services/skillApi.ts` 导出 `skillApi` 对象（`:262`，~55 方法），每方法包 `invoke()`；`__TAURI_INTERNALS__` 不在时返回浏览器 demo stub。
- **UI 组件**：`src/components/skills/` 下 18 个组件——视图（Central/Skill/Plugin/Collections/Pack/Discover/Obsidian/Marketplace/Sync）、详情（SkillDetailSlider）、对话框（InstallDialog/PackDialog）、展示（SkillCard/PackCard/FrontmatterCard/FileTreeViewer/FileContentViewer/InlineConfirmAction）。

---

## 4.4 后端模块（`src-tauri/src/skills/`，8 文件）

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 全部 serde 类型（SkillType/InstallMode/ScannedSkill/CentralSkillBundle/SkillPack/SkillCollection/SyncConfig/Marketplace*/FileTreeNode…）+ 集成测试（`:274-756`） |
| `agent_paths.rs` | **每个 Agent 的安装目录映射**（`paths_for_agent` `:13-104`，37 个已知 Agent id）；中央库 `~/.agents/skills`、legacy `~/.agentbro/skills`、metadata 路径 |
| `scanner.rs` | 文件系统扫描（技能/插件/MCP）、项目发现、Obsidian、文件树 |
| `installer.rs` | 复制/symlink/GitHub clone（带镜像回退 ghfast.top 等）/zip/raw 下载、toggle、MCP upsert、apply_pack |
| `registry.rs` | 持久化元数据（`~/.agentbro/metadata.json`）：sources/packs/collections/scan_roots/discovered/sync/custom_agents/marketplace_sources/explanations |
| `marketplace.rs` | 内置目录（Anthropic/OpenAI Skills、GitHub/Playwright/Context7/Notion MCP）+ 自定义 manifest + search/install |
| `sync.rs` | GitHub push/pull（冲突落 `pending-pull`）、resolve_conflicts、Agent↔Agent、zip backup |
| `explanation.rs` | AI 解释（`AGENTBRO_AI_API_KEY`/`OPENAI_API_KEY`，默认 `gpt-4.1-mini`，curl 调用，本地回退） |

**安装目标示例**（`agent_paths.rs`）：claude-code→`~/.claude/skills`（MCP/settings `~/.claude/settings.json`）；codex→`~/.codex/skills`+`~/.agents/skills`；gemini→`~/.gemini/skills`；cursor→`~/.cursor/skills`+`~/.cursor/rules`。Symlink 模式先复制进中央 `~/.agents/skills` 再 symlink 到各 Agent。

---

## 4.5 当前架构下的产品缺口（基于代码，非大重构建议）

1. **`SkillsSection.tsx` 死代码 / `CollectionsView` UI 不可达**（确认）：独立 `SkillsSection` 未被 `SettingsApp` 引用；`CollectionsView` 与专用 `ObsidianView` 仅存在于该死代码中。后端命令齐全但 UI 路由断开。→ 影响用户感知"功能缺失"。
2. **`stop_project_scan` 是 no-op stub**（`lib.rs:2609-2611` 返回 `Ok(())`）：前端"停止扫描"按钮无实际效果（发现扫描是同步阻塞的 `discover_project_skills`，无任务句柄可取消）。见 05-code-review 风险 5。
3. **市场只支持 `category:"skill"` 安装**（`marketplace.rs:154-156`）：plugin/mcp 类目返回错误——市场目录里有 MCP 项但点击安装会失败。
4. **AI 解释依赖外部 API key**：无 key 时退化为 `local_explanation`（`explanation.rs:144-185`），质量受限；curl 调用错误处理较薄。
5. **GitHub clone 走镜像回退**（`installer.rs:509-545`）：依赖第三方镜像（ghfast.top/ghproxy.net），稳定性与隐私（token 经第三方域名）需注意。
6. **同步冲突落 `pending-pull` 而非合并**（`sync.rs`）：需用户手动 `resolve_conflicts`，无自动三方合并。
7. **scan/registry 全量重扫**：`scan_all` 每次 walk 全部 Agent 目录，规模大时可能慢（推断，未见增量/缓存）。
8. **Obsidian 专用视图不可达**：发现逻辑可经 `DiscoverView` 间接调用，但 vault 列表 UI 未在活跃路由渲染。

> 这些都是**现有代码可直接看到的能力边界**，不需要大重构；多数可通过补 UI 路由或补 stub 实现收敛。
