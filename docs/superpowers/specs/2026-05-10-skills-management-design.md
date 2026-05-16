# AgentBro — 技能管理功能设计

## 概述

为 AgentBro 添加统一的 Skills / MCP / Plugins 管理能力，解决跨设备、跨 AI Agent 的技能配置碎片化问题。

**核心痛点：**
- 多台电脑间配置不同步
- 多个 AI Agent（Claude Code / Codex / Gemini CLI / Cursor / Hermes 等）的 skill/MCP 存放位置各异
- 技能包太多管理混乱，缺少批量启停和远程同步能力

## 设计决定

| 维度 | 决定 |
|------|------|
| 原子单位 | 单个 skill/MCP server，技能包（Pack）是集合 |
| 同步方式 | 三种：GitHub 私有仓库、导出/导入文件、云端（自建） |
| 安装方式 | 直接复制到 Agent 路径 / Symlink 到统一目录 `~/.agentbro/skills/`，用户可选 |
| 跨 Agent 同步 | 一键把 Agent A 的 skills 同步到 Agent B，不做格式转换 |
| 启停方式 | 配置级别，修改各 Agent 的 settings 文件；支持按 Agent 独立控制启停 |
| UI 入口 | Settings 窗口新增「技能管理」section |
| Skill 来源 | 第一版：手动输入地址（URL/GitHub/本地/skills.sh）+ 本地扫描；后续：marketplace |
| 技能包 manifest | 元数据清单，可含内联内容或第三方引用地址 |

## 数据架构

### 双层数据模型：Scanner + Registry

```
┌─────────────────────────────────────────────┐
│              UI 展示层（Skills 视图）           │
│         合并两个数据源，展示完整画面              │
└──────────┬──────────────────┬────────────────┘
           │                  │
    ┌──────▼──────┐    ┌──────▼──────┐
    │   Scanner    │    │  Registry   │
    │  扫描各 Agent │    │ metadata.json│
    │  目录，实时发现 │    │ 来源、Pack、 │
    │  已安装的 skill│    │ 同步配置     │
    └─────────────┘    └─────────────┘
         │                    │
    本地管理的 truth       跨设备同步的 truth
```

**Scanner**：打开 Skills 视图时扫描所有已启用 Agent 的 skill/MCP 目录，返回文件路径、名称、描述（从 frontmatter 解析）、文件大小、修改时间。

**Registry**（`~/.agentbro/metadata.json`）：只存储扫描无法推断的信息：

```json
{
  "sources": {
    "wechat-article": {
      "origin": "https://github.com/user/sz-workflows/skills/wechat-article",
      "installedVia": "island"
    }
  },
  "packs": [
    {
      "id": "content-production",
      "name": "内容生产",
      "description": "微信文章、图片、视频、资讯",
      "skills": ["wechat-article", "video-notes", "text-to-image"],
      "targetAgents": ["claude-code", "codex"]
    }
  ],
  "sync": {
    "method": "github",
    "githubRepo": "user/my-agent-config",
    "lastSyncAt": "2026-05-10T14:32:00Z",
    "autoSync": false
  }
}
```

**合并逻辑：**
- 扫描到 + registry 有 → 完整信息
- 扫描到 + registry 没有 → 显示为"本地发现"
- 扫描没到 + registry 有 → 标记为"未安装"，可一键重装

### 启停模型

每个 skill 在每个 Agent 上的启停状态独立控制。实现方式是修改该 Agent 的配置文件（如 Claude Code 的 `settings.json`）中的相关开关字段。

## 系统架构

### 后端模块（Rust / Tauri）

```
src-tauri/src/skills/
├── mod.rs              # 模块入口
├── scanner.rs          # 扫描各 Agent 的 skill/MCP 目录
├── registry.rs         # 读写 ~/.agentbro/metadata.json
├── installer.rs        # 安装/卸载（直接复制 or symlink）
├── sync.rs             # 同步引擎（Git / 导出导入 / Agent→Agent）
└── agent_paths.rs      # 各 Agent 的 skill/MCP/config 路径映射
```

**scanner.rs**
- 输入：AgentType 或 all
- 遍历该 Agent 的 skill 目录（路径由 agent_paths 提供）
- 解析 frontmatter 提取 name/description
- 同时解析 MCP 配置（各 Agent 的 mcp.json / settings.json 中的 mcpServers）
- 返回 `Vec<ScannedSkill>`

**agent_paths.rs**
- 每个 Agent 的 skill 目录、MCP 配置文件、settings 文件路径
- 扩展现有 `AgentAdapter` trait，增加 `fn skill_paths(&self) -> SkillPaths`
- 示例：
  - Claude Code: `~/.claude/skills/`, `~/.claude/settings.json`
  - Codex: `~/.codex/agents/`, `~/.codex/config.json`
  - Gemini: `~/.gemini/skills/`, `~/.gemini/settings.json`

**installer.rs**
- `install(source, targets, mode)` — 从来源获取 skill，安装到目标 Agent 目录
- `uninstall(skill_id, agent)` — 删除文件或移除 symlink
- `toggle(skill_id, agent, enabled)` — 修改目标 Agent 配置启停 skill
- 来源支持：本地路径、GitHub URL（clone 或 raw download）、skills.sh URL

**registry.rs**
- 读写 `~/.agentbro/metadata.json`
- CRUD for 来源记录和 Pack 定义
- 安装时自动记录来源

**sync.rs**
- Git 模式：推送 metadata.json + skill 文件到用户的私有 GitHub 仓库；拉取后根据 metadata 重建
- 导出模式：打包为 `.agentbro-backup.zip`
- Agent→Agent：扫描源 Agent 的 skills，批量调用 installer 安装到目标 Agent
- 冲突检测：对比本地和远端的修改时间，冲突时弹出解决对话框（保留本地/使用远端/保留两者）

### 前端组件（React）

```
src/components/settings/sections/
├── SkillsSection.tsx           # 新增 — 技能管理主入口
├── SkillsSection.css

src/components/skills/
├── SkillListView.tsx           # Skills 扁平列表视图
├── PackListView.tsx            # Pack 管理视图
├── SyncView.tsx                # 同步面板
├── SkillCard.tsx               # 单个 skill 卡片
├── PackCard.tsx                # 单个 Pack 卡片
├── SkillDetailSlider.tsx       # 右侧滑入的详情面板（文件浏览器）
├── FileTreeViewer.tsx          # 树形文件浏览器 + 代码预览
├── InstallDialog.tsx           # 安装对话框（4 种来源各自的表单）
├── PackDialog.tsx              # 新建/编辑技能包对话框
├── SyncConflictDialog.tsx      # 同步冲突解决对话框
├── UninstallConfirmDialog.tsx  # 卸载确认对话框
├── TokenConfigDialog.tsx       # GitHub Token 配置
├── AgentSyncConfirmDialog.tsx  # Agent→Agent 同步确认
```

### Tauri IPC Commands

```rust
// 扫描
#[tauri::command]
fn scan_agent_skills(agent: AgentType) -> Vec<ScannedSkill>;
#[tauri::command]
fn scan_all_skills() -> HashMap<AgentType, Vec<ScannedSkill>>;

// 安装/卸载
#[tauri::command]
fn install_skill(source: String, targets: Vec<TargetConfig>, mode: InstallMode) -> Result<(), String>;
#[tauri::command]
fn uninstall_skill(skill_id: String, agent: Option<AgentType>) -> Result<(), String>;
#[tauri::command]
fn toggle_skill(skill_id: String, agent: AgentType, enabled: bool) -> Result<(), String>;

// 文件浏览
#[tauri::command]
fn read_skill_files(skill_path: String) -> FileTree;
#[tauri::command]
fn read_skill_file_content(file_path: String) -> String;

// Pack 管理
#[tauri::command]
fn list_packs() -> Vec<SkillPack>;
#[tauri::command]
fn create_pack(pack: SkillPack) -> Result<(), String>;
#[tauri::command]
fn update_pack(pack: SkillPack) -> Result<(), String>;
#[tauri::command]
fn delete_pack(id: String) -> Result<(), String>;
#[tauri::command]
fn apply_pack(pack_id: String, agents: Vec<AgentType>) -> Result<(), String>;

// 同步
#[tauri::command]
fn configure_sync(config: SyncConfig) -> Result<(), String>;
#[tauri::command]
fn push_sync() -> Result<SyncResult, String>;
#[tauri::command]
fn pull_sync() -> Result<SyncResult, String>;
#[tauri::command]
fn resolve_conflicts(resolutions: Vec<ConflictResolution>) -> Result<(), String>;
#[tauri::command]
fn sync_agent_to_agent(from: AgentType, to: AgentType) -> Result<SyncPreview, String>;
#[tauri::command]
fn export_backup(path: String) -> Result<(), String>;
#[tauri::command]
fn import_backup(path: String) -> Result<(), String>;
```

## UI 交互设计

### 三 Tab 布局

| Tab | 内容 |
|-----|------|
| 技能列表 | 扁平列表，分"AgentBro 安装"和"本地发现"两组。搜索框 + 类型筛选（Skills/MCP）+ Agent 筛选 pills。支持批量选择模式。 |
| 技能包 | Pack 卡片列表，每个 Pack 展示包含的技能、目标 Agent、安装状态。支持创建/编辑/删除/全部启用/全部禁用/推送到 Agent。 |
| 同步 | GitHub 仓库同步（含 Token 配置）、导出/导入、Agent→Agent 一键同步、各 Agent 扫描状态。 |

### 技能详情面板

**从右侧滑入**，而非固定在右边。点击技能卡片时，详情面板从屏幕右侧滑入覆盖部分内容区。包含：

1. **安装位置** — 列出已安装的 Agent，每个 Agent 有独立的启停 toggle
2. **未安装的 Agent** — 显示"＋ 安装到 XXX"的操作行
3. **基本信息** — 类型、安装方式、大小、修改时间
4. **所属技能包** — 显示归属的 Pack，点击可跳转；支持"加入技能包"操作
5. **文件浏览器** — 树形目录（可展开/折叠/搜索）+ 代码预览窗口（带行号和语法高亮）
6. **底部操作栏** — 打开文件 / 加入技能包 / 卸载

### 启停控制

每个 skill 在每个 Agent 上的启停状态独立。在技能列表卡片上的 toggle 控制全局启停（所有 Agent），在详情面板的 Agent 行中的 toggle 控制单个 Agent 的启停。

### 安装对话框

4 种来源各自有独立的表单：

| 来源 | 表单内容 |
|------|---------|
| URL 地址 | URL 输入框 |
| GitHub 仓库 | owner/repo + 分支 + 子路径 |
| 本地路径 | 路径输入框 + 浏览按钮 |
| skills.sh | 搜索框 + 搜索结果列表（可多选） |

公共部分：选择目标 Agent（多选）+ 安装方式（直接复制 / Symlink）

### 关键交互状态

| 状态 | 处理 |
|------|------|
| 空状态（无技能） | 引导安装第一个技能 |
| 搜索无结果 | 提示"未找到匹配技能"+ 清除筛选按钮 |
| 扫描中 | Spinner + "正在扫描…" |
| 同步中 | 进度条动画 |
| 安装中 | 进度反馈 |
| 同步冲突 | 弹出冲突解决对话框，逐项选择"保留本地/使用远端/保留两者" |
| 卸载确认 | 二次确认弹窗，显示涉及的 Agent 列表 |
| Agent→Agent 同步确认 | 展示操作明细（复制 N 个、跳过 N 个、更新 N 个） |
| Token 配置 | 输入 PAT + 验证状态反馈 |

### 批量操作

支持多选模式，选中多个 skill 后可：批量启用、批量禁用、批量添加到技能包、批量卸载。

### 更新提示

从 GitHub 安装的技能显示版本标记，有新版本时显示黄色圆点，支持一键更新。

## 交互原型

可交互 HTML mockup 位于 `.superpowers/brainstorm/skills-ui-v3.html`，覆盖所有上述交互。

## 未来迭代

- Skill Marketplace — 内置可浏览的技能目录
- 云端同步服务 — 自建同步后端，替代 GitHub 仓库方案
- 自动同步 — 配置间隔自动推送/拉取
- 右键上下文菜单 — macOS 风格的右键操作
- 拖拽排序 — Pack 内技能顺序和列表自定义排序
