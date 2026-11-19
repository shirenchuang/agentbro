# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} improves Codex desktop detection and hook installation on macOS.

### Highlights

- **Detect Codex in ChatGPT.app** (#118). When AgentBro opens from Finder, it can use the Codex executable bundled with ChatGPT.app for session monitoring. Hook Doctor identifies the desktop installation. A standalone CLI is optional.
- **Repair Codex hook trust configuration** (#112). Installing or repairing AgentBro hooks now removes duplicate trust entries even when Codex has rewritten the table headers with quotes. This prevents an invalid configuration from blocking Codex startup.

### Install and update

- Install with Homebrew using `brew tap shirenchuang/tap && brew install --cask agentbro`, or use the macOS installer below.
- In-app updates use the signed `AgentBro.app.tar.gz` and the same release notes.

### Downloads

- macOS universal installer: `AgentBro_latest_universal.dmg` or `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers: `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files: `AgentBro.app.tar.gz` and `latest.json`
- Mainland China mirror: https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg

### Notes

- macOS is the primary signed release channel.
- The early Windows installers may show SmartScreen warnings until Windows code signing is configured.

## 中文

AgentBro v{{VERSION}} 改善了 macOS 上的 Codex 桌面端检测和 Hook 安装。

### 重点更新

- **识别 ChatGPT.app 自带的 Codex**（#118）。从 Finder 打开 AgentBro 时，可以用 ChatGPT.app 内置的 Codex 监测会话。Hook Doctor 能识别桌面安装，无需单独安装 CLI。
- **修复 Codex Hook 信任配置**（#112）。即使 Codex 已把配置表头改写为带引号的格式，安装或修复 AgentBro Hook 时也会去除重复的信任条目，避免配置损坏导致 Codex 无法启动。

### 安装与更新

- 可用 Homebrew 安装，命令为 `brew tap shirenchuang/tap && brew install --cask agentbro`，也可下载下方的 macOS 安装包。
- 应用内更新使用已签名的 `AgentBro.app.tar.gz`，并显示同一份版本说明。

### 下载

- macOS 通用安装包：`AgentBro_latest_universal.dmg` 或 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包：`AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件：`AgentBro.app.tar.gz` 和 `latest.json`
- 国内镜像：https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg

### 说明

- macOS 是主要的签名发布渠道。
- Windows 安装包仍处于早期阶段，在配置 Windows 代码签名前可能显示 SmartScreen 提示。
