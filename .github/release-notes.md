# AgentBro v2.10.0

## English

AgentBro v2.10.0 turns Agent settings into a hands-on control center for plugins, MCP servers, configuration files, and local Skills.

### Highlights

- **Complete plugin control center** - Discover installed plugins for Codex, Claude Code, WorkBuddy, ZCode, and Kimi; search and filter them, safely enable or disable supported plugins, inspect manifests and packaged capabilities, and preview Markdown, text, or image files without leaving AgentBro.
- **Interactive MCP management and inspection** - Manage stdio, HTTP, and SSE servers while preserving configured secrets, inspect tools, resources, prompts, and connection logs, call tools with explicit arguments and risk confirmation, preview generated prompt messages without sending them to a model, and cancel operations that are no longer needed. This also documents the MCP management foundation introduced in v2.9.0 but omitted from its published notes.
- **A more capable Agent configuration workspace** - See live installed app versions, open programs and configuration locations, edit supported JSON or text configuration files with validation and safe writes, and use clearer install, update, uninstall, and cleanup actions from the selected Agent.
- **Safer, faster Skill organization** - Batch-delete managed or unmanaged Agent Skills, take over matching center-library Skills in one action, continue successful batch adoptions into an existing or new skill pack, and keep built-in Doubao Skills visible as a separate read-only group.
- **Stronger release reliability** - Stable releases now carry complete Git history in build jobs, reject stale development-branch version metadata, and automatically open a pull request to synchronize each published release back to `dev`.

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
- Versioned archive: `AgentBro_2.10.0_universal.dmg`
- Stable Windows installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`; prereleases use versioned installer names.
- Auto update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### Notes

- macOS remains the primary, signed release channel.
- Windows installers are early MVP artifacts and may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v2.10.0 将 Agent 设置升级为可直接管理插件、MCP 服务、配置文件和本地 Skill 的完整控制中心。

### 重点更新

- **完整的插件控制中心** - 可发现 Codex、Claude Code、WorkBuddy、ZCode 与 Kimi 已安装的插件，支持搜索筛选、安全启停受支持的插件、查看清单与内置能力，并直接预览 Markdown、文本或图片文件。
- **可交互的 MCP 管理与检查** - 可在保留已配置密钥的前提下管理 stdio、HTTP 和 SSE 服务，检查工具、资源、Prompt 与连接日志；还能在明确参数和风险确认后调用工具、预览但不发送给模型的 Prompt 消息，并取消不再需要的操作。本次说明也补录了 v2.9.0 已上线但未写入已发布说明的 MCP 管理基础能力。
- **更强的 Agent 配置工作台** - 可查看本机应用的实时版本，打开程序与配置位置，通过校验和安全写入编辑受支持的 JSON 或文本配置文件，并在当前 Agent 中使用更清晰的安装、更新、卸载与清理操作。
- **更安全高效的 Skill 整理** - 支持批量删除已管理或未管理的 Agent Skills，一键接管中心库中的同名 Skill，将批量接管成功的 Skill 继续同步到已有或新建技能包，并把豆包内置 Skill 单独显示为只读分组。
- **更可靠的发布流程** - 构建任务会保留完整 Git 历史，开发分支版本元数据过期时会阻止发布，并在每个稳定版发布后自动创建回同步到 `dev` 的拉取请求。

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
- 版本归档: `AgentBro_2.10.0_universal.dmg`
- 稳定版 Windows 安装包: `AgentBro_latest_x64-setup.exe` 与 `AgentBro_latest_x64.msi`; 预览版使用带版本号的安装包文件名。
- 自动更新文件: `AgentBro.app.tar.gz` 与 `latest.json`
- 国内直链: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`

### 说明

- macOS 仍是主要的签名发布渠道。
- Windows 安装包属于早期 MVP 产物，在配置 Windows 代码签名前可能出现 SmartScreen 提示。
