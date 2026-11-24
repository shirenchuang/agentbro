# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} fixes Windows Codex Desktop jumps, keeps answered Island prompts closed, and respects custom Claude Code profiles over SSH.

### Highlights

- **Open Codex Desktop sessions more reliably on Windows.** AgentBro checks the desktop package registered for the current user and recognizes supported standalone installs. A session jump can fall back to an available launcher when Windows cannot open the thread link. [#551](https://github.com/shirenchuang/agentbro/pull/551)
- **Keep answered questions and plan approvals closed in the Island.** Once a response is accepted, a delayed session update can no longer reopen that prompt or replace a newer one. This also applies after responding through a keyboard shortcut or another AgentBro window. [#553](https://github.com/shirenchuang/agentbro/pull/553)
- **Use the active Claude Code profile on SSH hosts.** Skill discovery, distribution, and plugin settings follow `CLAUDE_CONFIG_DIR` from the remote login shell. Each profile keeps its own Skill Pack assignments. Pack and library operations recheck the reviewed targets if the profile changes before confirmation. [#549](https://github.com/shirenchuang/agentbro/pull/549)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.5.0](https://github.com/shirenchuang/agentbro/compare/v3.5.0...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 修复了 Windows 上的 Codex 桌面版会话跳转和灵动岛提示重复弹出的问题，并支持 SSH 主机上的 Claude Code 自定义配置目录。

### 重点更新

- **Windows 上更可靠地打开 Codex 桌面版会话。** AgentBro 会检查当前用户注册的桌面应用包，也能识别受支持的独立安装。Windows 无法打开会话链接时，可以改用可用的启动程序打开 Codex。[#551](https://github.com/shirenchuang/agentbro/pull/551)
- **灵动岛已处理的问题和计划审批保持关闭。** 回复被接收后，延迟的会话更新不会让同一条提示重新弹出，也不会挤掉更新的提示。通过快捷键或 AgentBro 的其他窗口回复时同样生效。[#553](https://github.com/shirenchuang/agentbro/pull/553)
- **SSH 主机使用 Claude Code 当前配置目录。** 技能发现、分发和插件设置遵循远程登录 Shell 中的 `CLAUDE_CONFIG_DIR`。不同配置各自记录已应用的技能包。如果在确认前切换了配置，技能包和技能库操作会重新核对预览时的目标。[#549](https://github.com/shirenchuang/agentbro/pull/549)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.5.0 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.5.0...v{{VERSION}})
