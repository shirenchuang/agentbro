# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} adds the standalone Qoder desktop app and improves Island shortcuts, Agent Management, and Windows discovery.

### Highlights

- **Qoder Desktop in Agent Management and the Island.** AgentBro discovers Qoder Desktop, Qoder IDE, and Qoder CLI separately on macOS and Windows. In Qoder Desktop, the Hook reports live task activity and lets you allow once or deny eligible requests in the Island. [#562](https://github.com/shirenchuang/agentbro/pull/562)
- **Qoder CLI sessions and one-time approvals.** The CLI Hook follows session switches and exits, returns one-time decisions, and can be repaired without removing the other Qoder apps' Hooks. Hooks and Skills follow the CLI's custom user configuration directory. [#568](https://github.com/shirenchuang/agentbro/pull/568) · [#570](https://github.com/shirenchuang/agentbro/pull/570)
- **Island shortcuts follow the request you see.** The shortcuts act once per key press, use the visible prompt when the Island is open, and pick the latest pending request when the Island is collapsed. Skip opens structured questions so you can review and answer them. [#574](https://github.com/shirenchuang/agentbro/pull/574) · [#576](https://github.com/shirenchuang/agentbro/pull/576) · [#578](https://github.com/shirenchuang/agentbro/pull/578) · [#580](https://github.com/shirenchuang/agentbro/pull/580)
- **Agent cleanup protects shared resources.** AgentBro checks that an uninstall succeeded before removing Hooks and Skills, preserves Skills shared by other Agents, and refreshes Hook actions after cleanup. [#558](https://github.com/shirenchuang/agentbro/pull/558) · [#564](https://github.com/shirenchuang/agentbro/pull/564) · [#566](https://github.com/shirenchuang/agentbro/pull/566)
- **Windows detects CLI installs after a refresh.** Agent Management uses current saved Windows PATH entries when scanning for agents, including installs made since AgentBro opened. [#572](https://github.com/shirenchuang/agentbro/pull/572)
- **Claude Code Hooks follow the active profile over SSH.** AgentBro checks, installs, and removes Hooks in the directory selected by the remote login shell's `CLAUDE_CONFIG_DIR`. [#561](https://github.com/shirenchuang/agentbro/pull/561)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.5.1](https://github.com/shirenchuang/agentbro/compare/v3.5.1...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 新增独立 Qoder 桌面版接入，并改进了灵动岛快捷键、Agent 管理和 Windows 上的安装检测。

### 重点更新

- **Agent 管理和灵动岛支持 Qoder 桌面版。** 在 macOS 和 Windows 上分别识别 Qoder 桌面版、Qoder IDE 和 Qoder CLI。桌面版安装 Hook 后，灵动岛会显示实时任务状态，并能对支持的审批请求选择「允许一次」或「拒绝」。[#562](https://github.com/shirenchuang/agentbro/pull/562)
- **Qoder CLI 会话和单次审批。** Hook 会跟踪会话切换和退出，回传单次审批结果。修复 CLI Hook 时会保留其他 Qoder 应用的 Hook。Hook 和技能都遵循 CLI 的自定义用户配置目录。[#568](https://github.com/shirenchuang/agentbro/pull/568) · [#570](https://github.com/shirenchuang/agentbro/pull/570)
- **灵动岛快捷键跟随当前请求。** 每次按键只处理一次。灵动岛展开时操作当前显示的请求，收起时选取最近到达的待处理请求。「跳过」遇到结构化问题会打开问题卡片，让你查看并作答。[#574](https://github.com/shirenchuang/agentbro/pull/574) · [#576](https://github.com/shirenchuang/agentbro/pull/576) · [#578](https://github.com/shirenchuang/agentbro/pull/578) · [#580](https://github.com/shirenchuang/agentbro/pull/580)
- **清理 Agent 时保留共享资源。** AgentBro 确认卸载成功后才会移除 Hook 和技能，保留其他 Agent 共用的技能，并在清理后刷新 Hook 操作状态。[#558](https://github.com/shirenchuang/agentbro/pull/558) · [#564](https://github.com/shirenchuang/agentbro/pull/564) · [#566](https://github.com/shirenchuang/agentbro/pull/566)
- **Windows 刷新后可识别新安装的 CLI。** Agent 管理扫描时会读取 Windows 当前保存的 PATH，也能发现 AgentBro 启动后安装的程序。[#572](https://github.com/shirenchuang/agentbro/pull/572)
- **SSH 上的 Claude Code Hook 跟随当前配置。** AgentBro 在远程登录 Shell 的 `CLAUDE_CONFIG_DIR` 指定的目录中检查、安装和移除 Hook。[#561](https://github.com/shirenchuang/agentbro/pull/561)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.5.1 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.5.1...v{{VERSION}})
