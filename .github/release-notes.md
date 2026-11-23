# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} improves Island approval feedback and Windows Agent management.

### Highlights

- **Island approvals show delivery errors and allow retries.** The prompt stays open if the decision fails. While a decision is being sent, repeated clicks are ignored. If you decline with feedback, AgentBro sends the feedback after the denial succeeds. [#525](https://github.com/shirenchuang/agentbro/pull/525)
- **Windows finds runnable Codex launchers.** AgentBro prefers the Codex bundled with the desktop app. For a standalone installation, it can find a Windows launcher such as `codex.cmd` even when a Git Bash shim appears earlier on your PATH. [#518](https://github.com/shirenchuang/agentbro/pull/518)
- **Agent Management keeps package operation errors in the transcript.** If an install, update, or uninstall command fails before it starts or while AgentBro waits for it, the operation log retains the error for diagnosis. [#520](https://github.com/shirenchuang/agentbro/pull/520)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- Windows installer tests handle hosted system prompts and exercise the Island when first-run Settings has been closed. [#522](https://github.com/shirenchuang/agentbro/pull/522)
- [Full changes since v3.3.2](https://github.com/shirenchuang/agentbro/compare/v3.3.2...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 改进了灵动岛审批反馈和 Windows 上的 Agent 管理。

### 重点更新

- **灵动岛会显示审批发送失败的原因，并允许重试。** 发送失败时保留审批请求，发送过程中会忽略重复点击。带说明的拒绝会在拒绝成功后发送说明。[#525](https://github.com/shirenchuang/agentbro/pull/525)
- **Windows 可以找到可运行的 Codex 启动器。** AgentBro 优先使用桌面客户端自带的 Codex。使用独立安装时，即使 PATH 前面有 Git Bash 的启动脚本，也可以找到 `codex.cmd` 等 Windows 启动器。[#518](https://github.com/shirenchuang/agentbro/pull/518)
- **Agent 管理会在操作日志里保留包操作错误。** 安装、更新或卸载命令无法启动，或 AgentBro 等待命令结束时出错，都可以在操作日志里查看原因。[#520](https://github.com/shirenchuang/agentbro/pull/520)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- Windows 安装包测试会处理托管环境的系统提示，并覆盖关闭首次启动设置窗口后的灵动岛操作。[#522](https://github.com/shirenchuang/agentbro/pull/522)
- [自 v3.3.2 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.3.2...v{{VERSION}})
