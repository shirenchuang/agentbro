# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} polishes the Skill Center and Agent Management workspace introduced in v2.1.0. This release focuses on making skill packs easier to install from the market, keeping batch-adopted skills synchronized with packs, and improving Codex project inspection.

### Highlights

- **Market skill pack selection** - Browse marketplace sources by publisher, open source-level skill lists, select multiple skills, and install them into a chosen pack from the same flow.
- **Batch adoption into packs** - When adopting unmanaged skills in bulk, AgentBro can now attach the adopted skills to an existing or newly created pack and keep that pack membership synchronized.
- **Skill pack clarity** - Pack management now explains when adopted skills are synced to packs, refreshes applied agent state after pack updates, and keeps the active pack and agent views aligned.
- **Codex project inspection** - Project management now reads Codex MCP servers and plugin settings so project-level configuration is easier to audit from AgentBro.
- **Agent workspace polish** - Agent detail, install, library, and pack views received layout, icon, state, and copy refinements for smoother daily use.
- **Reliability fixes** - The release includes stronger tests for skill manager views and store behavior, plus Rust updates for current clippy checks.

### Documentation

- README and README.en now include clearer community onboarding and updated social preview assets.
- Release assets continue to include macOS universal DMGs, updater files, and early Windows installer artifacts.

### Contributors

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

AgentBro v{{VERSION}} 继续打磨 v2.1.0 引入的 Skill 中心和 Agent 管理工作台。这个版本重点优化市场技能包安装、批量接管后的技能包同步，以及 Codex 项目配置检查。

### 重点更新

- **市场技能包选择** - 按发布者浏览市场源，进入单个来源查看技能列表，批量选择技能，并在同一流程里安装到指定技能包。
- **批量接管写入技能包** - 批量接管未管理 Skills 时，现在可以把接管后的技能同步加入已有或新建技能包，并保持包成员关系一致。
- **技能包体验更清晰** - 技能包管理会说明接管后的同步行为，包更新后刷新已应用 Agent 状态，并保持当前包和 Agent 视图一致。
- **Codex 项目配置检查** - 项目管理现在可以读取 Codex MCP servers 和插件配置，更方便从 AgentBro 审计项目级配置。
- **Agent 工作台打磨** - Agent 详情、安装、库和技能包页面更新了布局、图标、状态和文案，日常使用更顺手。
- **可靠性修复** - 增加 Skill Manager 视图和 store 行为测试，并更新 Rust 代码以通过当前 clippy 检查。

### 文档

- README 和 README.en 更新了社区加入说明，并补充新的社交预览素材。
- 发布产物继续包含 macOS 通用 DMG、自动更新文件，以及早期 Windows 安装包。

### 贡献者

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
