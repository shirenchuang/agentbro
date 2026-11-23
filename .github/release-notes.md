# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} improves Windows startup and Island approvals and makes MCP management more reliable.

### Highlights

- **Windows skips duplicate launches early.** If AgentBro is already starting or running for the same user and Hook port, a second launch exits before loading WebView2. The first process keeps handling Hooks. [#514](https://github.com/shirenchuang/agentbro/pull/514)
- **Windows Island approvals can recover keyboard focus more than once.** A pending approval button can regain focus after a separate loss, so you can still make your choice with the keyboard. [#511](https://github.com/shirenchuang/agentbro/pull/511)
- **MCP changes stay on the Agent and host you reviewed.** Switching between Agents, the local machine, or SSH hosts clears outdated inventories and confirmations. Responses from the previously selected host no longer replace the current list. [#505](https://github.com/shirenchuang/agentbro/pull/505)
- **The MCP editor preserves focus and draft values.** Renaming environment variables or HTTP headers keeps keyboard focus. Switching transports preserves each draft, and saving preserves argument order, intentional empty arguments, and existing credentials when you clear a replacement value. [#507](https://github.com/shirenchuang/agentbro/pull/507)
- **Agent Management displays OpenCode's native MCP configuration.** View entries from user-level JSON and JSONC files on local and SSH hosts, with their configuration source and enabled state. This inventory is read-only. [#509](https://github.com/shirenchuang/agentbro/pull/509)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.3.1](https://github.com/shirenchuang/agentbro/compare/v3.3.1...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 改进了 Windows 启动与灵动岛审批，以及 MCP 管理的稳定性。

### 重点更新

- **Windows 提前结束重复启动。** 同一用户下的 AgentBro 正在用同一个 Hook 端口启动或运行时，第二个进程会在加载 WebView2 前退出，第一个进程继续处理 Hook。[#514](https://github.com/shirenchuang/agentbro/pull/514)
- **Windows 灵动岛审批可以多次恢复键盘焦点。** 同一个待审批按钮再次失去焦点后仍可恢复，方便继续用键盘作出选择。[#511](https://github.com/shirenchuang/agentbro/pull/511)
- **MCP 修改固定在已确认的 Agent 和主机上。** 切换 Agent、本机或 SSH 主机时会清除旧列表和确认弹窗。上一个主机迟到的响应也不会覆盖当前列表。[#505](https://github.com/shirenchuang/agentbro/pull/505)
- **MCP 编辑器保留焦点和草稿。** 重命名环境变量或 HTTP 请求头时会保留键盘焦点，切换传输方式时会保留各自的草稿。保存时会保留参数顺序和有意留空的参数，清空替换值也会保留原凭据。[#507](https://github.com/shirenchuang/agentbro/pull/507)
- **Agent 管理可以查看 OpenCode 原生 MCP 配置。** 支持读取本机和 SSH 主机上的用户级 JSON 与 JSONC 文件，显示配置来源和启用状态。列表为只读。[#509](https://github.com/shirenchuang/agentbro/pull/509)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.3.1 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.3.1...v{{VERSION}})
