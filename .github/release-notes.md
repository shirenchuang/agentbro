# AgentBro v2.7.0

## English

AgentBro v2.7.0 adds first-class ZCode and Doubao support while making Skills v2 imports and skill pack synchronization more predictable and resilient.

### Highlights

- **ZCode integration** - Detect ZCode sessions, display their activity and branding in the island, and manage ZCode Skills through the same AgentBro workflows as other supported coding agents.
- **Doubao integration** - Capture Doubao coding-session events through a dedicated watcher so active work appears in AgentBro alongside other agents.
- **Idempotent local Skill imports** - Re-importing an unchanged source is now skipped, generated folders such as virtual environments and render output are ignored, and the UI clearly reports when nothing needs importing.
- **Safer skill pack editing** - The editor shows each Skill's other pack memberships and allows removing members from applied packs while automatically synchronizing affected Agents.
- **More resilient pack synchronization** - Interrupted syncs recover to a retryable state, unchanged targets avoid redundant work, and stale or missing distributions are repaired more reliably.

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
- Versioned archive: `AgentBro_2.7.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.7.0 新增 ZCode 与豆包的原生支持，并让 Skills v2 的本地导入和技能包同步更可预期、更可靠。

### 重点更新

- **ZCode 集成** - 支持检测 ZCode 会话，在灵动岛中展示活动与品牌图标，并通过统一的 AgentBro 流程管理 ZCode Skills。
- **豆包集成** - 通过专用 watcher 捕获豆包编程会话事件，让进行中的工作与其他 Agent 一起显示在 AgentBro 中。
- **幂等的本地 Skill 导入** - 重复导入未变化的来源时会自动跳过；虚拟环境、渲染输出等生成目录不再参与内容判断，界面也会明确提示无需重复导入。
- **更安全的技能包编辑** - 编辑器会展示每个 Skill 所属的其他技能包；从已应用技能包移除成员后，会自动同步受影响的 Agent。
- **更可靠的技能包同步** - 中断的同步会恢复为可重试状态；未变化的目标不再重复写入，过期或缺失的分发也能更可靠地修复。

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
- 版本归档: `AgentBro_2.7.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
