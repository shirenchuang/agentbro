# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} expands the app from a runtime Dynamic Island into a local-first Agent Management workspace. This release focuses on managing AI coding agents, skills, hooks, MCP servers, plugins, and project-level configuration from one desktop control panel.

### Highlights

- **Agent Management workspace** - Scan local CLIs and desktop agents, then review install state, versions, executable paths, config directories, official install pages, hooks, skills, MCP servers, plugins, and health state in one place.
- **Skill Center library** - Discover skills scattered across agent folders, adopt unmanaged skills into the center library, and distribute them to selected agents by symlink or copy.
- **Skill packs** - Group reusable skills into packs, apply or revoke them from agent detail pages, and handle conflicts with overwrite, skip, or keep-agent-copy choices.
- **Project management** - Import repositories, inspect project-level Claude Code and Codex instructions/config, scan project skills, MCP servers, and plugins, and install center skills into projects.
- **Per-agent hook management** - Install or remove hooks for each supported agent, inspect bridge commands and config paths, and toggle event groups such as approvals, notifications, lifecycle, and activity.
- **Diagnostics and recovery** - Surface broken links, changed copies, missing paths, unmanaged skills, and other local configuration issues so they can be fixed from the workspace.

### Documentation

- README and README.en now explain the Agent Management workflow, Skill Center, supported agent scopes, screenshots, and updated onboarding steps.
- Release assets continue to include macOS universal DMGs, updater files, and early Windows installer artifacts.

### Contributors

- @guijilvren
- @nicobeyond
- @shirenchuang

### Install And Update

- Homebrew supports one-line installation: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Releases and in-app auto update use the same release notes.
- Auto update artifacts include signed `AgentBro.app.tar.gz` and `latest.json`.

### Downloads

- Recommended download: `AgentBro_latest_universal.dmg`
- Versioned archive: `AgentBro_{{VERSION}}_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v{{VERSION}} 从运行时灵动岛，进一步扩展成一个本地优先的 Agent 管理工作台。这个版本重点更新 AI 编程 Agent 的管理能力，把 Agent 安装、版本、Hook、Skills、MCP、插件和项目级配置收进同一个桌面控制面板。

### 重点更新

- **Agent 管理工作台** - 自动扫描本机 CLI 和桌面 Agent，统一查看安装状态、版本、可执行文件路径、配置目录、官方下载入口、Hook、Skills、MCP、插件和健康状态。
- **Skill 中心库** - 扫描散落在不同 Agent 目录里的 Skills，把未管理的 Skill 接管到中心库，再按软链接或拷贝分发到指定 Agent。
- **技能包** - 把常用 Skills 组合成可复用包，在 Agent 详情页一键应用或撤销；遇到冲突时可选择覆盖、跳过或保留 Agent 现有副本。
- **项目管理** - 导入本地仓库，检查项目级 Claude Code / Codex 指令和配置，扫描项目 Skills、MCP、插件，并把中心库 Skills 安装到项目里。
- **按 Agent 管理 Hook** - 为每个支持的 Agent 安装或移除 Hook，查看 Bridge 命令和配置路径，并按审批、通知、生命周期、活动等事件组开关。
- **诊断与修复** - 暴露坏链接、副本变更、路径缺失、未接管 Skills 等本地配置问题，方便从工作台统一处理。

### 文档

- README 和 README.en 已更新 Agent 管理流程、Skill 中心库、支持范围、截图和接入步骤。
- 发布产物继续包含 macOS 通用 DMG、自动更新文件，以及早期 Windows 安装包。

### 贡献者

- @guijilvren
- @nicobeyond
- @shirenchuang

### 安装与更新

- Homebrew 支持一行安装: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Release 和应用内自动更新会使用同一份版本说明。
- 自动更新文件包含签名的 `AgentBro.app.tar.gz` 与 `latest.json`。

### 下载

- 推荐下载: `AgentBro_latest_universal.dmg`
- 版本归档: `AgentBro_{{VERSION}}_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
