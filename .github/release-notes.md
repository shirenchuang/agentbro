# AgentBro v{{VERSION}}

## English

This hotfix improves external-display pet behavior and tightens native host resizing for the Dynamic Island.

### Fixes

- **Fix pet dragging across displays** - Pet windows now use native Tauri dragging when available, preserving the final position after dragging across monitors.
- **Keep pet clicks reliable on external displays** - External-display pet windows remain interactive, avoiding click-through loss after moving the pet off the primary screen.
- **Shrink the native host on hover** - The Dynamic Island native host now resizes promptly when leaving the stable collapsed canvas, reducing accidental occlusion.

### Contributors

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

这是一次热修复，改善外接显示器上的宠物窗口拖动与点击，并收紧灵动岛 native host 的尺寸切换。

### 修复

- **修复跨屏拖动宠物窗口** - 可用时改用 Tauri 原生拖动，跨显示器拖动后能保留最终窗口位置。
- **保持外接屏宠物可点击** - 宠物移动到非主屏后仍保持交互，避免点击穿透失控。
- **hover 时及时收缩 native host** - 灵动岛离开 collapsed 稳定画布后会及时调整 native host 尺寸，减少遮挡。

### 贡献者

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
