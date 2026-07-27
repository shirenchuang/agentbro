# AgentBro v3.0.0

## English

AgentBro v3.0.0 turns Agent Management into a unified local-and-remote control center. Choose a runtime environment once, then use the same Skill Library, installation, skill packs, Agent inspection, and diagnostics against that machine.

### Highlights

- **One runtime switcher for the whole workspace** - Switch between the local Mac and configured SSH hosts from the lower-left environment control. Remote badges, connection state, and explicit connect actions remain visible throughout Agent Management, so every page makes its current target clear.
- **Full Agent and Skill management over SSH** - The remote environment now supports the Skill Library, Skill installation, skill packs, Agent scanning, MCP and plugin inspection, diagnostics, configuration management, and a remote-only folder picker without replacing the familiar local interface.
- **Dedicated remote server management** - SSH configuration now has its own settings page beside Agent Management. Configure a host once, connect when needed, diagnose the tunnel, and open a terminal directly at a detected remote directory.
- **Safer and more reliable remote operations** - Remote writes never silently fall back to the local machine. The release also fixes oversized command payloads, atomic state-file replacement, Python 3.7 compatibility, Unicode filenames, disconnected-state feedback, and legacy center-library migration to the fixed `~/.agentbro/skills` location.
- **Broader remote Agent discovery** - Remote scans now recognize OpenClaw workspaces and common user-level Node.js installations managed by NVM or fnm. Local executable discovery also recognizes current, legacy, and custom vfox Node.js SDK locations.

### Documentation

- The Chinese and English READMEs now present AgentBro as a desktop control center, add direct macOS and Windows downloads, and document the island, Agent Monitor, Agent Management, Agent Switch, SSH Remote, supported agents, and platform status.

### Contributors

- @shirenchuang

### Install And Update

- Homebrew supports one-line installation: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Releases and in-app auto update use the same release notes.
- Auto update artifacts include signed `AgentBro.app.tar.gz` and `latest.json`.

### Downloads

- Recommended download: `AgentBro_latest_universal.dmg`
- Versioned archive: `AgentBro_3.0.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v3.0.0 将 Agent 管理升级为统一的本机与远程控制中心。只需选择一次运行环境，Skill 库、安装、技能包、Agent 检查和诊断就会统一作用于对应机器。

### 重点更新

- **一个切换器控制整个工作区** - 可从左下角的运行环境入口在本机 Mac 与已配置的 SSH 主机之间切换。远程标识、连接状态和明确的连接按钮会贯穿 Agent 管理，让每个页面都清楚显示当前操作目标。
- **通过 SSH 完整管理 Agent 与 Skill** - 远程环境现在支持 Skill 库、Skill 安装、技能包、Agent 扫描、MCP 与插件检查、诊断、配置管理，以及只浏览远程服务器的目录选择器，同时保留与本机一致的操作界面。
- **独立的远程服务器管理** - SSH 配置从灵动岛设置中拆分为 Agent 管理旁的独立设置页。每台服务器只需配置一次，即可按需连接、诊断隧道，并让终端直接进入检测到的远程目录。
- **更安全可靠的远程操作** - 远程写操作不会静默回退到本机。本次还修复了超长命令参数、状态文件原子替换、Python 3.7 兼容、Unicode 文件名、断线状态反馈，以及旧中心库迁移到固定 `~/.agentbro/skills` 目录等问题。
- **更完整的远程 Agent 识别** - 远程扫描现在可识别 OpenClaw workspace，以及 NVM、fnm 管理的常见用户级 Node.js 安装。本机可执行文件扫描也新增了对当前、旧版和自定义 vfox Node.js SDK 路径的支持。

### 文档

- 中英文 README 现已统一将 AgentBro 介绍为桌面 Agent 控制中心，补充 macOS 与 Windows 直链下载，并完整说明灵动岛、Agent Monitor、Agent 管理、Agent Switch、SSH Remote、支持的 Agent 与平台状态。

### 贡献者

- @shirenchuang

### 安装与更新

- Homebrew 支持一行安装: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Release 和应用内自动更新会使用同一份版本说明。
- 自动更新文件包含签名的 `AgentBro.app.tar.gz` 与 `latest.json`。

### 下载

- 推荐下载: `AgentBro_latest_universal.dmg`
- 版本归档: `AgentBro_3.0.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
