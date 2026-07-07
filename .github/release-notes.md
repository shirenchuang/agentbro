# AgentBro v2.5.0

## English

AgentBro v2.5.0 expands WorkBuddy support and makes Skills v2 agent management more practical for day-to-day cleanup, distribution, and navigation.

### Highlights

- **Richer WorkBuddy integration** - WorkBuddy now installs hooks into `.workbuddy/settings.json` with nested hook entries and covers prompts, tool use, permissions, notifications, compaction, session lifecycle, and subagent events.
- **Better permission and session handling** - AgentBro recognizes more WorkBuddy/Codex event shapes, clears resolved permission prompts more reliably, and removes closed or stale Codex App threads from the island.
- **Skills v2 cleanup tools** - Agent management now supports deleting custom Agent registrations, removing unmanaged local skills, and deleting managed skill distributions from detail views.
- **Skill pack conflict recovery** - Pack sync can surface blocking conflicts and let you choose whether to overwrite, keep the Agent copy, or skip a target.
- **Smarter Agent navigation** - Installed Agents in the settings sidebar are ordered by usage, with manual up/down controls for keeping frequent Agents close at hand.

### Documentation

- Release assets continue to include macOS universal DMGs, updater files, and Windows installer artifacts.

### Contributors

- @shirenchuang

### Install And Update

- Homebrew supports one-line installation: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Releases and in-app auto update use the same release notes.
- Auto update artifacts include signed `AgentBro.app.tar.gz` and `latest.json`.

### Downloads

- Recommended download: `AgentBro_latest_universal.dmg`
- Versioned archive: `AgentBro_2.5.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.5.0 扩展了 WorkBuddy 支持，也让 Skills v2 的 Agent 管理更适合日常清理、分发和导航。

### 重点更新

- **更完整的 WorkBuddy 集成** - WorkBuddy 现在会把 hook 安装到 `.workbuddy/settings.json`，使用嵌套 hook 配置，并覆盖提示词、工具调用、权限、通知、上下文压缩、会话生命周期和子任务事件。
- **更稳的权限与会话处理** - AgentBro 能识别更多 WorkBuddy/Codex 事件格式，更可靠地清理已处理的权限请求，并从灵动岛里移除已关闭或过期的 Codex App 线程。
- **Skills v2 清理工具** - Agent 管理页现在支持删除自定义 Agent 注册、移除未接管的本地 Skill，以及从详情页删除已管理的 Skill 分发。
- **技能包冲突恢复** - 技能包同步遇到阻止项时，可以直接选择覆盖、保留 Agent 版本或跳过目标。
- **更聪明的 Agent 导航** - 设置侧边栏里的已安装 Agent 会按使用情况排序，并支持手动上移/下移常用 Agent。

### 文档

- 发布产物继续包含 macOS 通用 DMG、自动更新文件以及 Windows 安装包。

### 贡献者

- @shirenchuang

### 安装与更新

- Homebrew 支持一行安装: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Release 和应用内自动更新会使用同一份版本说明。
- 自动更新文件包含签名的 `AgentBro.app.tar.gz` 与 `latest.json`。

### 下载

- 推荐下载: `AgentBro_latest_universal.dmg`
- 版本归档: `AgentBro_2.5.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
