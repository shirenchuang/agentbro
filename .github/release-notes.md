# AgentBro v2.4.0

## English

AgentBro v2.4.0 improves custom agent setup and tightens skill source handling, making the skill manager more predictable when importing local skills or configuring agent-specific fields.

### Highlights

- **Custom agent config fields** - Added support for agent-defined configuration fields in the add-agent dialog, so agents can request the settings they actually need during setup.
- **Skill source selection** - Fixed skill source field selection to keep local, marketplace, and pack-backed skill flows aligned.
- **Local skill import conflict handling** - Avoided false conflict warnings when importing local skills, reducing noisy blockers in normal skill management workflows.

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
- Versioned archive: `AgentBro_2.4.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.4.0 改进了自定义 Agent 设置和技能来源处理，让技能管理器在导入本地技能、配置 Agent 专属字段时更稳定、更可预期。

### 重点更新

- **自定义 Agent 配置字段** - 新增 Add Agent 对话框对 Agent 自定义配置字段的支持，Agent 可以在设置时要求填写真正需要的参数。
- **技能来源选择** - 修复技能来源字段选择逻辑，让本地、市场和技能包来源的流程保持一致。
- **本地技能导入冲突处理** - 避免导入本地技能时出现误报冲突，减少正常技能管理流程里的干扰。

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
- 版本归档: `AgentBro_2.4.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
