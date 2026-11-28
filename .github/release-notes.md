# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} adds Gemini Desktop coding integration on macOS, restores more conversation history in the Island, and improves Skill Pack transfers and Agent management.

### Highlights

- **Gemini Desktop and Antigravity on macOS.** Follow coding tasks from Gemini Desktop's built-in Antigravity mode in the Island and apply Skill Packs directly to Gemini Desktop. Gemini handles its own approvals, and removing the shared Hooks keeps them removed. [#651](https://github.com/shirenchuang/agentbro/pull/651) · [#667](https://github.com/shirenchuang/agentbro/pull/667) · [#669](https://github.com/shirenchuang/agentbro/pull/669) · [#675](https://github.com/shirenchuang/agentbro/pull/675)
- **See recent messages when opening a session.** The Island includes the latest ten user or assistant messages when the transcript contains them, even when tool calls fill the usual history window. This applies to Gemini Desktop, Codex, Claude, and other supported transcripts. Gemini Desktop shows earlier turns and the current reply when a task completes. Claude Desktop can fall back to the Claude Code transcript when its history index is unavailable. [#707](https://github.com/shirenchuang/agentbro/pull/707) · [#709](https://github.com/shirenchuang/agentbro/pull/709) · [#711](https://github.com/shirenchuang/agentbro/pull/711)
- **Keep Skill sources when moving or restoring Skills.** Offline Skill Pack ZIPs preserve GitHub source details, and pinned sources reopen at the recorded commit. Project locks can restore verified HTTPS archives and direct downloads. ZIP reviews compare executable permissions, snapshots preserve Skill relationships and order, and diagnosis repairs preserve restored Skills. [#671](https://github.com/shirenchuang/agentbro/pull/671) · [#673](https://github.com/shirenchuang/agentbro/pull/673) · [#687](https://github.com/shirenchuang/agentbro/pull/687) · [#691](https://github.com/shirenchuang/agentbro/pull/691) · [#695](https://github.com/shirenchuang/agentbro/pull/695) · [#701](https://github.com/shirenchuang/agentbro/pull/701)
- **Keep Island questions and drafts available.** Tool activity preserves pending Hook interactions, disconnected questions and plan approvals clear, and new questions can appear after reconnection. Keyboard selection stays with its session, text composition preserves drafts, and the Windows question list stays open after a footer click. [#645](https://github.com/shirenchuang/agentbro/pull/645) · [#655](https://github.com/shirenchuang/agentbro/pull/655) · [#659](https://github.com/shirenchuang/agentbro/pull/659) · [#663](https://github.com/shirenchuang/agentbro/pull/663) · [#665](https://github.com/shirenchuang/agentbro/pull/665) · [#703](https://github.com/shirenchuang/agentbro/pull/703)
- **Handle Qoder approvals and failures in the Island.** Deny a Qoder Desktop request directly from the Island, see permission denials and task failures, and respond to simultaneous approvals separately. Agent Management distinguishes Windows CLI and desktop installations and reports native Qoder CLI versions. [#618](https://github.com/shirenchuang/agentbro/pull/618) · [#626](https://github.com/shirenchuang/agentbro/pull/626) · [#638](https://github.com/shirenchuang/agentbro/pull/638) · [#647](https://github.com/shirenchuang/agentbro/pull/647) · [#649](https://github.com/shirenchuang/agentbro/pull/649)
- **Manage the Agent installation you reviewed.** npm commands target the selected installation on the current computer or over SSH, avoid accidental downgrades, and complete after the command exits. Agent discovery finds registered desktop apps outside standard paths on macOS and Windows, and macOS uninstalls target the reviewed app. Login shell discovery also has a timeout. [#614](https://github.com/shirenchuang/agentbro/pull/614) · [#616](https://github.com/shirenchuang/agentbro/pull/616) · [#620](https://github.com/shirenchuang/agentbro/pull/620) · [#622](https://github.com/shirenchuang/agentbro/pull/622) · [#630](https://github.com/shirenchuang/agentbro/pull/630) · [#641](https://github.com/shirenchuang/agentbro/pull/641) · [#661](https://github.com/shirenchuang/agentbro/pull/661) · [#697](https://github.com/shirenchuang/agentbro/pull/697) · [#706](https://github.com/shirenchuang/agentbro/pull/706)
- **Get clearer Hook, startup, and MCP diagnostics.** Hook Doctor shows delivery and active configuration. Diagnostic exports include startup and Hook records, Windows reports startup failures and occupied Hook ports, and local Codex MCP inspection resolves configured HTTP credentials. Saving Island preferences preserves Hook state and applies configuration notifications in save order. [#624](https://github.com/shirenchuang/agentbro/pull/624) · [#632](https://github.com/shirenchuang/agentbro/pull/632) · [#636](https://github.com/shirenchuang/agentbro/pull/636) · [#643](https://github.com/shirenchuang/agentbro/pull/643) · [#677](https://github.com/shirenchuang/agentbro/pull/677) · [#679](https://github.com/shirenchuang/agentbro/pull/679) · [#699](https://github.com/shirenchuang/agentbro/pull/699)
- **Keep the Island in place across display changes.** macOS preserves the configured offset and Windows recenters the Island. On Windows, Hooks start before the desktop windows are constructed, and launching AgentBro again opens Settings. [#653](https://github.com/shirenchuang/agentbro/pull/653) · [#657](https://github.com/shirenchuang/agentbro/pull/657) · [#681](https://github.com/shirenchuang/agentbro/pull/681) · [#683](https://github.com/shirenchuang/agentbro/pull/683)
- **Get an update when your platform installer is ready.** The update check verifies installer availability before offering an update through the download path. [#685](https://github.com/shirenchuang/agentbro/pull/685)

### Install and update

- On macOS, use the in-app updater, the universal installer, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.
- On Windows, AgentBro can offer the published x64 installer from its update screen.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- macOS in-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.7.0](https://github.com/shirenchuang/agentbro/compare/v3.7.0...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 新增 macOS 上的 Gemini Desktop 编码集成，补全灵动岛会话历史，并改进技能包迁移和 Agent 管理。

### 重点更新

- **macOS 支持 Gemini Desktop 和 Antigravity。** 灵动岛可跟踪 Gemini Desktop 内嵌 Antigravity 编码模式的任务，技能包也可直接应用到 Gemini Desktop。审批由 Gemini 自己处理。移除共用 Hook 后，AgentBro 会保留移除状态。[#651](https://github.com/shirenchuang/agentbro/pull/651) · [#667](https://github.com/shirenchuang/agentbro/pull/667) · [#669](https://github.com/shirenchuang/agentbro/pull/669) · [#675](https://github.com/shirenchuang/agentbro/pull/675)
- **打开会话就能看到近期聊天。** 只要会话记录里有足够的消息，即使普通历史窗口被工具调用占满，灵动岛也会包含最近 10 条用户或助手消息。适用于 Gemini Desktop、Codex、Claude 及其他已支持的会话记录。Gemini Desktop 会显示前几轮对话和任务完成时的最新回复。Claude Desktop 历史索引不可用时，可回退读取 Claude Code 会话记录。[#707](https://github.com/shirenchuang/agentbro/pull/707) · [#709](https://github.com/shirenchuang/agentbro/pull/709) · [#711](https://github.com/shirenchuang/agentbro/pull/711)
- **迁移或恢复技能时保留来源。** 离线技能包 ZIP 会保留 GitHub 来源信息，已固定版本的来源会按记录中的提交打开。项目锁文件可恢复经过校验的 HTTPS 归档和直链下载。ZIP 预览会比较执行权限，快照保留技能关系和顺序，诊断修复保留已恢复的技能。[#671](https://github.com/shirenchuang/agentbro/pull/671) · [#673](https://github.com/shirenchuang/agentbro/pull/673) · [#687](https://github.com/shirenchuang/agentbro/pull/687) · [#691](https://github.com/shirenchuang/agentbro/pull/691) · [#695](https://github.com/shirenchuang/agentbro/pull/695) · [#701](https://github.com/shirenchuang/agentbro/pull/701)
- **灵动岛保留待处理问题和草稿。** 工具运行期间，待处理的 Hook 交互会继续显示。连接断开后失效的问题和计划审批会清除，重连后的新问题可正常出现。键盘选中状态跟随原会话，输入法组字时保留草稿，Windows 点击底部按钮后会保持问题列表打开。[#645](https://github.com/shirenchuang/agentbro/pull/645) · [#655](https://github.com/shirenchuang/agentbro/pull/655) · [#659](https://github.com/shirenchuang/agentbro/pull/659) · [#663](https://github.com/shirenchuang/agentbro/pull/663) · [#665](https://github.com/shirenchuang/agentbro/pull/665) · [#703](https://github.com/shirenchuang/agentbro/pull/703)
- **在灵动岛处理 Qoder 审批和失败状态。** 可直接拒绝 Qoder Desktop 请求，查看权限拒绝和任务失败状态，并分别处理同时出现的审批。Agent 管理会区分 Windows 上的 Qoder CLI 和桌面安装，并显示原生 Qoder CLI 版本。[#618](https://github.com/shirenchuang/agentbro/pull/618) · [#626](https://github.com/shirenchuang/agentbro/pull/626) · [#638](https://github.com/shirenchuang/agentbro/pull/638) · [#647](https://github.com/shirenchuang/agentbro/pull/647) · [#649](https://github.com/shirenchuang/agentbro/pull/649)
- **管理已确认的 Agent 安装。** npm 命令对选定的本机或 SSH 安装执行，避免意外降级，并在命令退出后结束操作。macOS 和 Windows 可发现注册在标准目录之外的桌面应用，macOS 的卸载操作针对已确认的应用。登录 Shell 探测也增加了超时。[#614](https://github.com/shirenchuang/agentbro/pull/614) · [#616](https://github.com/shirenchuang/agentbro/pull/616) · [#620](https://github.com/shirenchuang/agentbro/pull/620) · [#622](https://github.com/shirenchuang/agentbro/pull/622) · [#630](https://github.com/shirenchuang/agentbro/pull/630) · [#641](https://github.com/shirenchuang/agentbro/pull/641) · [#661](https://github.com/shirenchuang/agentbro/pull/661) · [#697](https://github.com/shirenchuang/agentbro/pull/697) · [#706](https://github.com/shirenchuang/agentbro/pull/706)
- **查看更完整的 Hook、启动和 MCP 诊断。** Hook 诊断显示事件投递和生效配置，诊断导出包含启动和 Hook 记录。Windows 会报告启动失败和 Hook 端口占用，本地检查 Codex MCP 时可解析已配置的 HTTP 凭据。保存灵动岛偏好时会保留 Hook 状态，配置通知也按保存顺序应用。[#624](https://github.com/shirenchuang/agentbro/pull/624) · [#632](https://github.com/shirenchuang/agentbro/pull/632) · [#636](https://github.com/shirenchuang/agentbro/pull/636) · [#643](https://github.com/shirenchuang/agentbro/pull/643) · [#677](https://github.com/shirenchuang/agentbro/pull/677) · [#679](https://github.com/shirenchuang/agentbro/pull/679) · [#699](https://github.com/shirenchuang/agentbro/pull/699)
- **显示器变化后灵动岛会重新定位。** macOS 会保留已设置的偏移，Windows 会重新居中。Windows 也会在构建桌面窗口前启动 Hook，再次启动 AgentBro 会打开设置。[#653](https://github.com/shirenchuang/agentbro/pull/653) · [#657](https://github.com/shirenchuang/agentbro/pull/657) · [#681](https://github.com/shirenchuang/agentbro/pull/681) · [#683](https://github.com/shirenchuang/agentbro/pull/683)
- **对应平台安装包可用后再提示更新。** 检查更新会先确认安装包已发布，再提供下载安装入口。[#685](https://github.com/shirenchuang/agentbro/pull/685)

### 安装与更新

- macOS 可使用应用内更新、通用安装包，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。
- Windows 可从 AgentBro 的更新界面获取已发布的 x64 安装包。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- macOS 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.7.0 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.7.0...v{{VERSION}})
