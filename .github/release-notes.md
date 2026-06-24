# AgentBro v2.3.0

## English

AgentBro v2.3.0 improves Windows compatibility and cleans up hook installation flows. This release is the first to meaningfully widen Windows support alongside the established macOS experience.

### Highlights

- **Windows compatibility** - Improved floating window, tray, hook transport, and launch-at-login behavior on Windows 11. AgentBro is more usable as a daily driver on Windows while remaining an MVP artifact pending code signing.
- **Marketplace skill install flow** - Clarified the skill pack install flow so users can see which pack a marketplace skill will be installed into before confirming.
- **Codex hook and Windows skill links** - Fixed Codex hook configuration paths and corrected skill link resolution on Windows so hooks and skill shortcuts work out of the box.

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
- Versioned archive: `AgentBro_2.3.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.3.0 改进了 Windows 兼容性并优化了 Hook 安装流程。这是首个在成熟的 macOS 体验基础上显著拓宽 Windows 支持的版本。

### 重点更新

- **Windows 兼容性** - 改进了 Windows 11 上的悬浮窗、托盘、Hook 传输和开机自启行为，AgentBro 在 Windows 上的日常可用性显著提升。
- **市场技能安装流程** - 明确了技能包安装流程，用户在确认前可以看到市场技能将被安装到哪个技能包。
- **Codex Hook 与 Windows 技能链接** - 修复了 Codex Hook 配置路径和 Windows 上的技能链接解析，确保 Hook 和技能快捷方式开箱即用。

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
- 版本归档: `AgentBro_2.3.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。