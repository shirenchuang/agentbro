# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} brings DeepSeek Harness and QwenWork CN desktop task activity into the macOS Island.

### Highlights

- **DeepSeek Harness in the Island** ([#124](https://github.com/shirenchuang/agentbro/pull/124)). The AgentBro desktop plugin reports turn activity. Eligible approvals offer Allow once and Deny. Open Harness once, quit it, install its Hook in AgentBro, then reopen Harness.
- **QwenWork CN live task status** ([#124](https://github.com/shirenchuang/agentbro/pull/124)). AgentBro reads local task phases and shows queued, running, finished, and waiting states. Handle confirmations and questions in QwenWork.
- **Separate desktop entries and Skill management** ([#121](https://github.com/shirenchuang/agentbro/pull/121)). Agent Management detects both desktop apps and manages their local directory-format Skills. DeepSeek Harness also shows inherited Skills from the default shared directory.

### Install and update

- On macOS, use the signed universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [Mainland China macOS mirror](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The new desktop session integrations run on macOS.
- Windows installers are early-release builds and may show SmartScreen warnings.

## 中文

AgentBro v{{VERSION}} 将 DeepSeek Harness 和千问办公桌面端的任务状态接入 macOS 灵动岛。

### 重点更新

- **DeepSeek Harness 接入灵动岛**（[#124](https://github.com/shirenchuang/agentbro/pull/124)）。AgentBro 桌面插件会报告任务运行状态，可转发的审批支持允许一次和拒绝。先打开 Harness 一次并退出，再在 AgentBro 安装对应 Hook，然后重新打开 Harness。
- **千问办公实时任务状态**（[#124](https://github.com/shirenchuang/agentbro/pull/124)）。AgentBro 只读观察本地任务状态，显示排队、运行、完成和等待交互。确认和提问在千问办公里处理。
- **独立桌面入口和 Skill 管理**（[#121](https://github.com/shirenchuang/agentbro/pull/121)）。Agent 管理可识别这两款桌面应用，并管理各自的目录式 Skill。DeepSeek Harness 还会显示从默认共享目录继承的 Skill。

### 安装与更新

- macOS 可使用已签名的通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- 新接入的桌面实时会话功能适用于 macOS。
- Windows 安装包处于早期阶段，可能显示 SmartScreen 提示。
