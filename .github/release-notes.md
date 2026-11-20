# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} restores DeepSeek Harness conversation previews and adds QwenWork CN Flash task activity to the Island.

### Highlights

- **DeepSeek Harness conversation previews** ([#137](https://github.com/shirenchuang/agentbro/pull/137)). The Island uses your prompt as the session title and shows previews of your prompt and the assistant’s reply in the session details.
- **QwenWork CN Flash task activity** ([#137](https://github.com/shirenchuang/agentbro/pull/137)). Tasks run with the Flash engine now appear in the Island with their titles and status, including running and completed.

### Install and update

- On macOS, use the signed universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.
- To update an existing Harness integration, quit DeepSeek Harness, remove and reinstall its Hook in AgentBro, then reopen Harness.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [Mainland China macOS mirror](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The desktop session integrations run on macOS.
- Windows installers are early-release builds and may show SmartScreen warnings.

## 中文

AgentBro v{{VERSION}} 修复了 DeepSeek Harness 的会话预览，并将千问办公 Flash 任务接入灵动岛。

### 重点更新

- **DeepSeek Harness 会话预览**（[#137](https://github.com/shirenchuang/agentbro/pull/137)）。灵动岛根据提问生成会话标题，并在详情中显示提问和助手回复的预览。
- **千问办公 Flash 任务状态**（[#137](https://github.com/shirenchuang/agentbro/pull/137)）。使用 Flash 引擎的任务会在灵动岛显示标题和状态，包括运行中和已完成。

### 安装与更新

- macOS 可使用已签名的通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。
- 若已接入 Harness，请先退出 DeepSeek Harness，在 AgentBro 中卸载并重新安装它的 Hook，再打开 Harness。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- 桌面实时会话功能适用于 macOS。
- Windows 安装包处于早期阶段，可能显示 SmartScreen 提示。
