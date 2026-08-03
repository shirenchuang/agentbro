# AgentBro v3.1.0

## English

AgentBro v3.1.0 adds first-class Antigravity 2.0 support and turns shared Agent Skills into a clearer, safer, and faster part of Agent Management on both local and remote environments.

### Highlights

- **Antigravity 2.0 desktop and `agy` CLI support** - AgentBro now detects Antigravity desktop and CLI installations, registers their official lifecycle and permission hooks, and manages their Skills, MCP servers, plugins, and hook paths while preserving user-owned configuration. Legacy AgentBro-managed hooks are migrated without rewriting unrelated settings.
- **Layered Agent and shared Skill management** - Agent-owned Skills and the shared `~/.agents/skills` inventory now appear as distinct managed, unmanaged, and read-only inherited scopes. Codex, Kimi Code, OpenClaw, and ZCode receive accurate shared inheritance without duplicate cards, counts, or scan results, including Skills installed through nested package-manager wrappers.
- **Complete shared Skill controls locally and over SSH** - Shared inherited Skills now use the same cards, lists, details, adoption, sync, diagnosis, and removal controls as Agent-owned Skills wherever those capabilities are supported. Destructive actions explain their cross-Agent impact, validate paths fail-safe, and use atomic remote updates with recovery artifacts.
- **Much faster Skill deletion** - Deleted Skills disappear from the interface immediately, batch operations avoid repeated local scans and remote inventory rebuilds, and one guarded refresh reconciles the final state while keeping failed items visible.
- **Correct runtime targeting from the tray** - The Skill Pack picker now restores the current local or remote environment before loading and refreshes available hosts whenever it opens, preventing actions from using stale runtime state.
- **No background console flashes on Windows** - Startup detection, integration diagnostics, hook checks, Codex usage polling, and other automatic probes now run without opening command windows. Explicit terminal launches remain visible.

### Contributors

- @shirenchuang

### Install And Update

- Homebrew supports one-line installation: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Releases and in-app auto update use the same release notes.
- Auto update artifacts include signed `AgentBro.app.tar.gz` and `latest.json`.

### Downloads

- Recommended download: `AgentBro_latest_universal.dmg`
- Versioned archive: `AgentBro_3.1.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v3.1.0 新增对 Antigravity 2.0 的完整支持，并让共享 Agent Skill 成为本机与远程 Agent 管理中更清晰、更安全、更快速的一部分。

### 重点更新

- **支持 Antigravity 2.0 桌面端与 `agy` CLI** - AgentBro 现在可以检测 Antigravity 桌面端与 CLI，注册官方生命周期和权限 Hook，并管理其 Skill、MCP Server、插件与 Hook 路径，同时保留用户自己的配置。旧版由 AgentBro 管理的 Hook 会被迁移，其他无关设置不会被改写。
- **分层管理 Agent Skill 与共享 Skill** - Agent 自有 Skill 与共享 `~/.agents/skills` 清单现在会分别展示为已管理、未管理和只读继承范围。Codex、Kimi Code、OpenClaw 与 ZCode 可以准确继承共享 Skill，不再出现重复卡片、数量或扫描结果，也可识别通过嵌套包管理器目录安装的 Skill。
- **本机与 SSH 环境均可完整操作共享 Skill** - 在能力支持的环境中，共享继承 Skill 现在与 Agent 自有 Skill 复用相同的卡片、列表、详情、收编、同步、诊断和移除操作。破坏性操作会说明对多个 Agent 的影响，采用失败即停止的路径校验，并通过带恢复产物的原子更新保障远程操作安全。
- **显著加快 Skill 删除** - 删除后的 Skill 会立即从界面消失；批量操作不再重复执行本机扫描或远程清单重建，并通过一次受保护的刷新校准最终状态，同时保留删除失败的项目。
- **托盘操作始终使用正确运行环境** - 技能包选择器现在会在加载前恢复当前本机或远程环境，并在每次打开时刷新可用主机，避免使用过期的运行环境状态执行操作。
- **Windows 后台不再闪现控制台窗口** - 启动检测、集成诊断、Hook 检查、Codex 用量轮询及其他自动探测现在不会弹出命令窗口；用户主动打开的终端仍会正常显示。

### 贡献者

- @shirenchuang

### 安装与更新

- Homebrew 支持一行安装: `brew tap shirenchuang/tap && brew install --cask agentbro`
- GitHub Release 和应用内自动更新会使用同一份版本说明。
- 自动更新文件包含签名的 `AgentBro.app.tar.gz` 与 `latest.json`。

### 下载

- 推荐下载: `AgentBro_latest_universal.dmg`
- 版本归档: `AgentBro_3.1.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
