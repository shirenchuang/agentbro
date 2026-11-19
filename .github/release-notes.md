# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} fixes DeepSeek Harness approval controls and improves QwenWork CN monitoring and macOS foreground detection.

### Highlights

- **DeepSeek Harness approval controls** ([#129](https://github.com/shirenchuang/agentbro/pull/129)). The Island and Pet Surface show the two actions supported by the Harness desktop plugin, Allow once and Deny.
- **QwenWork CN account updates** ([#132](https://github.com/shirenchuang/agentbro/pull/132)). Other accounts keep updating when an account database becomes unreadable. AgentBro retains the last readable state until that account recovers or its displayed task expires.
- **Native macOS foreground and pointer queries** ([#131](https://github.com/shirenchuang/agentbro/pull/131)). AgentBro uses AppKit to find the foreground application and read the pointer location. These queries run in the AgentBro process.

### Install and update

- On macOS, use the signed universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [Mainland China macOS mirror](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The desktop session integrations run on macOS.
- Windows installers are early-release builds and may show SmartScreen warnings.

## 中文

AgentBro v{{VERSION}} 修正了 DeepSeek Harness 审批按钮，并改进了千问办公任务监测和 macOS 前台检测。

### 重点更新

- **DeepSeek Harness 审批按钮**（[#129](https://github.com/shirenchuang/agentbro/pull/129)）。灵动岛和桌宠统一显示 Harness 桌面插件支持的「允许一次」与「拒绝」。
- **千问办公多账号更新**（[#132](https://github.com/shirenchuang/agentbro/pull/132)）。个别账号库读不到时，其他账号的任务状态继续刷新。AgentBro 保留受影响账号上一次读取到的状态，直到账号库恢复读取或任务显示过期。
- **macOS 原生前台检测和鼠标定位**（[#131](https://github.com/shirenchuang/agentbro/pull/131)）。AgentBro 在自身进程中，通过 AppKit 获取前台应用和鼠标位置。

### 安装与更新

- macOS 可使用已签名的通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- 桌面实时会话功能适用于 macOS。
- Windows 安装包处于早期阶段，可能显示 SmartScreen 提示。
