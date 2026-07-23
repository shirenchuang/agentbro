# AgentBro v2.8.0

## English

AgentBro v2.8.0 deepens Kimi Code support and streamlines the path from discovering an unmanaged Skill to organizing and using it.

### Highlights

- **First-class Kimi Code CLI workflows** - Detect and manage the current Kimi Code CLI through its `.kimi-code` configuration, including global and project Skills, Agents, MCP servers, plugins, and install/update commands.
- **Richer Kimi session activity** - Approval requests and results, interruptions, stop failures, notifications, compaction, subagents, and final responses are normalized so the island reflects Kimi work more accurately.
- **Adopt directly into a skill pack** - After adopting an unmanaged Skill, add it to an existing pack or create a new pack in the same dialog. If only the pack update fails, the operation can be retried without adopting the Skill twice.
- **Safer adoption choices** - Stale or no-longer-valid adoption options are rejected with a clear localized error instead of applying an unexpected fallback.
- **Easier Agent navigation from the tray** - The Agent selector in the skill pack picker now supports horizontal wheel scrolling and pointer dragging without accidental selection.

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
- Versioned archive: `AgentBro_2.8.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.8.0 深化了对 Kimi Code 的支持，并简化了从发现未管理 Skill 到完成整理和使用的整个流程。

### 重点更新

- **完整的 Kimi Code CLI 工作流** - 基于当前 `.kimi-code` 配置检测和管理 Kimi Code CLI，覆盖全局与项目级 Skills、Agents、MCP 服务、插件以及安装/更新命令。
- **更完整的 Kimi 会话状态** - 统一处理审批请求与结果、中断、停止失败、通知、上下文压缩、子 Agent 和最终回复，让灵动岛更准确地呈现 Kimi 的工作过程。
- **接管后直接加入技能包** - 接管未管理 Skill 后，可在同一对话框中加入已有技能包或新建技能包。若只有技能包更新失败，可以重试且不会重复接管 Skill。
- **更安全的接管选项** - 对已过期或不再有效的接管方式给出清晰的本地化错误，不再应用意外的兜底操作。
- **更方便的托盘 Agent 导航** - 技能包选择器中的 Agent 列表支持横向滚轮与指针拖动，并避免拖动后误选。

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
- 版本归档: `AgentBro_2.8.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
