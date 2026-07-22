# AgentBro v2.6.0

## English

AgentBro v2.6.0 makes skill setup and Agent maintenance faster, with batch marketplace installs, a menu-bar skill pack picker, and clearer lifecycle controls for local coding agents.

### Highlights

- **Batch marketplace installs** - Select multiple Skills from one repository, install them in a single background task, follow progress from anywhere in Settings, and cancel an active download when needed.
- **Menu-bar skill pack picker** - Open a lightweight picker from the macOS menu bar to enable or disable skill packs without opening the full settings window.
- **Practical Agent lifecycle controls** - Agent details now expose richer program, version, configuration, hook, and health information, with supported install, update, uninstall, and cleanup actions.
- **Stronger skill organization** - Move directly distributed Skills into packs, batch-manage Agent Skills, resolve conflicts more clearly, and keep pack application progress visible while navigating.
- **Smoother island and settings interactions** - Settings controls respond more fluidly, the island collapses before Settings opens, and native hover/cursor handling avoids transparent-window interference.

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
- Versioned archive: `AgentBro_2.6.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.6.0 让 Skill 配置和 Agent 维护更高效，新增技能市场批量安装、菜单栏技能包快切，并完善本地编程 Agent 的生命周期管理。

### 重点更新

- **技能市场批量安装** - 可从同一仓库选择多个 Skill，通过单个后台任务完成安装；离开市场页面后仍能查看进度，并可随时取消正在进行的下载。
- **菜单栏技能包快切** - 可从 macOS 菜单栏打开轻量选择器，直接启用或停用技能包，无需进入完整设置窗口。
- **更实用的 Agent 生命周期管理** - Agent 详情页提供更完整的程序、版本、配置、Hook 和健康信息，并为支持的 Agent 提供安装、更新、卸载与清理操作。
- **更强的 Skill 整理能力** - 可将直接分发的 Skill 移入技能包、批量管理 Agent Skills、更清晰地解决冲突，并在页面切换时持续显示技能包应用进度。
- **更流畅的灵动岛与设置交互** - 设置控件响应更及时；打开设置前会先折叠灵动岛，原生悬停和鼠标穿透处理也不再让透明窗口阻挡操作。

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
- 版本归档: `AgentBro_2.6.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
