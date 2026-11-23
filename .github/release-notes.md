# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} brings portable Skill Packs, project Skill recovery, and broader Windows Agent support. It also improves the Island, Agent Management, and MCP across local and SSH runtimes.

### Highlights

- **Share and restore Skill Packs.** Export a versioned JSON manifest or open one from a local file or a public GitHub file link. Review missing members and SHA-256 content differences before importing. An optional export records the repository path and exact commit for eligible GitHub Skills, so a missing member can be restored and verified on the selected local or SSH host. Earlier manifests still open. [Portable manifests](https://github.com/shirenchuang/agentbro/pull/232) · [Content review](https://github.com/shirenchuang/agentbro/pull/370) · [Pinned restore](https://github.com/shirenchuang/agentbro/pull/412) · [GitHub links](https://github.com/shirenchuang/agentbro/pull/420)
- **Restore project Skills from a lockfile.** Open Projects from Settings to inspect `skills-lock.json`, check installed Skills against it, and recover selected missing entries. Supported v2 entries can be restored from a pinned GitHub revision or an installed npm dependency. AgentBro verifies the recorded content before adding a Skill and keeps the lockfile and existing Skills in place. [GitHub restore](https://github.com/shirenchuang/agentbro/pull/214) · [Lock verification](https://github.com/shirenchuang/agentbro/pull/226) · [npm restore](https://github.com/shirenchuang/agentbro/pull/346)
- **Connect more Agents on Windows.** DeepSeek Harness desktop reports live sessions and eligible approvals through its AgentBro Hook. QwenWork CN appears in Agent Management and shows task activity in the Island. Use QwenWork itself to answer its confirmations. WorkBuddy and WorkBuddy AI use separate desktop profiles, and WorkBuddy AI confirmation notices lead back to the desktop app. [Harness](https://github.com/shirenchuang/agentbro/pull/198) · [QwenWork](https://github.com/shirenchuang/agentbro/pull/200) · [WorkBuddy](https://github.com/shirenchuang/agentbro/pull/202)
- **Bring WSL sessions into the Windows Island.** Install Claude Code or Codex CLI Hooks for a selected registered WSL distribution in Island settings. Sessions show the distribution and can reopen the WSL terminal at the Linux project directory. Codex uses its own Hook review. [WSL support](https://github.com/shirenchuang/agentbro/pull/204) · [Distribution detection](https://github.com/shirenchuang/agentbro/pull/272)
- **Review Git Skill updates.** Check for a newer revision and see which files will be added, changed, or removed before applying it. Imports retain the resolved commit, and public Markdown and ZIP download links can be imported directly. [Git updates](https://github.com/shirenchuang/agentbro/pull/189) · [File review](https://github.com/shirenchuang/agentbro/pull/330) · [Direct downloads](https://github.com/shirenchuang/agentbro/pull/188)

### Island and desktop reliability

- Sessions preserve newer live events when earlier data arrives late. Restart restores sessions with Unicode identifiers, and a temporary process-list failure keeps existing sessions visible. QwenWork gives active and waiting tasks priority over completed history and follows fresh Flash messages even when the conversation record is older. [Session updates](https://github.com/shirenchuang/agentbro/pull/171) · [Unicode sessions](https://github.com/shirenchuang/agentbro/pull/210) · [QwenWork activity](https://github.com/shirenchuang/agentbro/pull/481)
- Approval, question, and plan responses stay attached to the displayed request and its original Hook. Controls are individually keyboard accessible. On Windows, the visible approval pane keeps its size and can restore renderer focus while the Island is in the foreground. [Hook replies](https://github.com/shirenchuang/agentbro/pull/258) · [Windows approval controls](https://github.com/shirenchuang/agentbro/pull/414) · [Questions](https://github.com/shirenchuang/agentbro/pull/435) · [Plans](https://github.com/shirenchuang/agentbro/pull/437)
- On Windows, the Island uses the target display coordinates, stays where you release it after dragging, and avoids requesting focus during hidden startup. Session jumps reuse an identified Windows Terminal window when available. On macOS, foreground app and cursor detection run in-process. Visibility and layout preferences stay consistent when you preview settings or turn the Island back on. [Dragging](https://github.com/shirenchuang/agentbro/pull/358) · [Startup focus](https://github.com/shirenchuang/agentbro/pull/477) · [Terminal jumps](https://github.com/shirenchuang/agentbro/pull/286) · [macOS detection](https://github.com/shirenchuang/agentbro/pull/131)

### Agent Management and Skills

- Search the Agent sidebar by name and see installation status from current detection. Windows recognizes registered desktop apps, reads executable versions, respects PATH order across launchers, and opens the requested folder in Explorer. Narrow settings windows keep Agent, Projects, and Skill Pack controls usable. [Agent search](https://github.com/shirenchuang/agentbro/pull/148) · [Windows app detection](https://github.com/shirenchuang/agentbro/pull/431) · [Launcher order](https://github.com/shirenchuang/agentbro/pull/439) · [Explorer](https://github.com/shirenchuang/agentbro/pull/222)
- Agent install, update, and uninstall stay active when you navigate away. Reopening Settings recovers package output and refreshes detected versions when the command finishes. Operations use the selected toolchain, verify the resulting program state, and block a second operation while one is running. Native Claude Code can use its own updater, desktop-bundled Codex shows its detected version, and Windows also supports uninstalling pip-installed Aider. [Operation checks](https://github.com/shirenchuang/agentbro/pull/427) · [Package output](https://github.com/shirenchuang/agentbro/pull/472) · [Claude Code](https://github.com/shirenchuang/agentbro/pull/465) · [Codex](https://github.com/shirenchuang/agentbro/pull/467)
- Agent cleanup stays within the reviewed set of Skills and Hooks. Skill distribution, adoption, deletion, and Pack operations stay on the reviewed Agent and runtime, including during host changes. Import and copy operations preserve existing files when a replacement fails or the target has changed. Skill inventory updates are published together, and Pack descriptions and tags stay intact. [Cleanup review](https://github.com/shirenchuang/agentbro/pull/485) · [Distribution deletion](https://github.com/shirenchuang/agentbro/pull/487) · [Copy recovery](https://github.com/shirenchuang/agentbro/pull/240)
- The Skill Overview uses the root `SKILL.md`. File previews stay on the selected file when an earlier document read finishes, including for Windows paths. Switching hosts closes previews opened from Agent Management. Database migration preserves SQLite WAL data, and open errors preserve the existing database for recovery. [File previews](https://github.com/shirenchuang/agentbro/pull/489) · [Database recovery](https://github.com/shirenchuang/agentbro/pull/344) · [WAL migration](https://github.com/shirenchuang/agentbro/pull/349)

### SSH and MCP

- Manage custom Agents, their Hooks, native plugin settings, and Codex MCP servers on the selected SSH host. Configuration saves keep edits made during the save, preserve the original files when a write fails, and keep existing MCP credentials in place. Private Git credentials stay out of command arguments, saved remote URLs, and Skill backup metadata. [SSH Agent management](https://github.com/shirenchuang/agentbro/pull/404) · [Plugin settings](https://github.com/shirenchuang/agentbro/pull/374) · [Codex MCP](https://github.com/shirenchuang/agentbro/pull/294) · [Git credentials](https://github.com/shirenchuang/agentbro/pull/328)
- The MCP Inspector handles negotiated capabilities, pagination, complete stdio and HTTP streams, and cancellation of the original SSH operation. Local HTTP calls retain the configured origin and negotiated protocol. Verbose local servers keep running, and Windows package, Skill, and MCP background tasks avoid opening extra consoles. [SSH MCP](https://github.com/shirenchuang/agentbro/pull/296) · [Local streaming](https://github.com/shirenchuang/agentbro/pull/310) · [Windows background tasks](https://github.com/shirenchuang/agentbro/pull/186)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.
- If DeepSeek Harness is already connected, quit it, remove and reinstall its Hook in AgentBro, then reopen Harness.
- Windows installers are x64. Windows can show a SmartScreen prompt. Intermittent Windows startup and accessibility behavior are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [Mainland China macOS mirror](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)
- [Full changes since v3.2.2](https://github.com/shirenchuang/agentbro/compare/v3.2.2...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 加入可移植技能包、项目 Skill 恢复和更多 Windows Agent 接入，并改进了本机及 SSH 环境下的灵动岛、Agent 管理和 MCP。

### 重点更新

- **分享和恢复技能包。** 可导出带版本号的 JSON 清单，也可通过本地文件或公开的 GitHub 文件链接打开。导入前可核对缺失成员和 SHA-256 内容差异。可选的 GitHub 来源导出会为符合条件的 Skill 记录仓库路径和精确提交，在选定的本机或 SSH 主机恢复缺失内容并校验。旧版清单仍可打开。[可移植清单](https://github.com/shirenchuang/agentbro/pull/232) · [内容核对](https://github.com/shirenchuang/agentbro/pull/370) · [固定版本恢复](https://github.com/shirenchuang/agentbro/pull/412) · [GitHub 链接](https://github.com/shirenchuang/agentbro/pull/420)
- **按锁文件恢复项目 Skills。** 从设置打开「项目」，查看 `skills-lock.json`、校验已安装内容，并选择要恢复的缺失项。支持从 GitHub 固定提交或已安装的 npm 依赖恢复符合条件的 v2 条目。写入前校验记录的内容，保留原锁文件和已有 Skills。[GitHub 恢复](https://github.com/shirenchuang/agentbro/pull/214) · [锁文件校验](https://github.com/shirenchuang/agentbro/pull/226) · [npm 恢复](https://github.com/shirenchuang/agentbro/pull/346)
- **Windows 接入更多桌面 Agent。** DeepSeek Harness 桌面端可通过 AgentBro Hook 上报实时会话和支持的审批。千问办公出现在 Agent 管理中，灵动岛会显示任务状态，确认事项在千问办公内处理。WorkBuddy 和 WorkBuddy AI 使用各自的桌面配置，WorkBuddy AI 待确认通知可跳回客户端。[Harness](https://github.com/shirenchuang/agentbro/pull/198) · [千问办公](https://github.com/shirenchuang/agentbro/pull/200) · [WorkBuddy](https://github.com/shirenchuang/agentbro/pull/202)
- **WSL 会话接入 Windows 灵动岛。** 在灵动岛设置中为选定的已注册 WSL 发行版安装 Claude Code 或 Codex CLI Hook。会话显示所属发行版，点击可回到 Linux 项目目录下的 WSL 终端。Codex 继续使用自己的 Hook 审核流程。[WSL 支持](https://github.com/shirenchuang/agentbro/pull/204) · [发行版检测](https://github.com/shirenchuang/agentbro/pull/272)
- **应用 Git Skill 更新前核对文件。** 可检查新提交，预览将新增、修改和删除的文件再应用。导入会记录实际解析出的提交，也支持直接导入公开的 Markdown 和 ZIP 下载链接。[Git 更新](https://github.com/shirenchuang/agentbro/pull/189) · [文件预览](https://github.com/shirenchuang/agentbro/pull/330) · [直接下载](https://github.com/shirenchuang/agentbro/pull/188)

### 灵动岛与桌面稳定性

- 较早的会话数据延迟到达时会保留较新的实时事件。重启后可恢复带 Unicode 标识的会话，进程列表暂时读取失败时会保留已有会话。千问办公优先显示运行中和待确认任务，Flash 会话记录较旧时仍跟踪新消息。[会话更新](https://github.com/shirenchuang/agentbro/pull/171) · [Unicode 会话](https://github.com/shirenchuang/agentbro/pull/210) · [千问状态](https://github.com/shirenchuang/agentbro/pull/481)
- 审批、问题和计划的回复会发送给当前显示的请求及原始 Hook。灵动岛操作可独立使用键盘访问。Windows 上审批视图保持可用尺寸，并能在灵动岛处于前台时恢复界面焦点。[Hook 回传](https://github.com/shirenchuang/agentbro/pull/258) · [Windows 审批](https://github.com/shirenchuang/agentbro/pull/414) · [问题回复](https://github.com/shirenchuang/agentbro/pull/435) · [计划审批](https://github.com/shirenchuang/agentbro/pull/437)
- Windows 灵动岛使用目标显示器坐标，拖动松开后留在释放位置，隐藏启动时避免请求焦点。跳转会话时可复用已识别的 Windows Terminal 窗口。macOS 在应用进程内检测前台应用和指针。预览设置或重新启用灵动岛时，可见性和布局配置保持一致。[拖动](https://github.com/shirenchuang/agentbro/pull/358) · [启动焦点](https://github.com/shirenchuang/agentbro/pull/477) · [终端跳转](https://github.com/shirenchuang/agentbro/pull/286) · [macOS 检测](https://github.com/shirenchuang/agentbro/pull/131)

### Agent 管理与 Skills

- Agent 侧栏支持按名称搜索，按当前检测结果显示安装状态。Windows 可识别已注册的桌面应用、读取可执行文件版本、按 PATH 顺序选择启动器，并在资源管理器中打开指定目录。较窄的设置窗口也能使用 Agent、项目和技能包操作。[Agent 搜索](https://github.com/shirenchuang/agentbro/pull/148) · [Windows 应用识别](https://github.com/shirenchuang/agentbro/pull/431) · [启动器顺序](https://github.com/shirenchuang/agentbro/pull/439) · [资源管理器](https://github.com/shirenchuang/agentbro/pull/222)
- 离开 Agent 管理页面时，安装、更新和卸载继续运行。重开设置可恢复命令输出，并在命令结束后刷新检测到的版本。操作使用选定的工具链并核实程序结果，已有操作运行时会阻止第二项操作。原生 Claude Code 可使用自带更新器，Codex 桌面内置程序显示检测到的版本，Windows 还支持卸载通过 pip 安装的 Aider。[结果验证](https://github.com/shirenchuang/agentbro/pull/427) · [命令输出](https://github.com/shirenchuang/agentbro/pull/472) · [Claude Code](https://github.com/shirenchuang/agentbro/pull/465) · [Codex](https://github.com/shirenchuang/agentbro/pull/467)
- Agent 清理限定在核对过的 Skills 和 Hooks。分发、接管、删除和技能包操作会留在已核对的 Agent 和运行环境，切换主机也保持原范围。导入或复制替换失败、目标已有变化时会保护已有文件。Skill 清单整批刷新，技能包描述和标签得以保留。[清理范围](https://github.com/shirenchuang/agentbro/pull/485) · [分发删除](https://github.com/shirenchuang/agentbro/pull/487) · [复制恢复](https://github.com/shirenchuang/agentbro/pull/240)
- Skill 概览读取根目录的 `SKILL.md`。较早的文档读取完成时，文件预览仍保留用户选中的文件，并支持 Windows 路径。切换主机时会关闭从 Agent 管理打开的预览。数据库迁移会保留 SQLite WAL 数据，打开失败时会保留原库供恢复。[文件预览](https://github.com/shirenchuang/agentbro/pull/489) · [数据库恢复](https://github.com/shirenchuang/agentbro/pull/344) · [WAL 迁移](https://github.com/shirenchuang/agentbro/pull/349)

### SSH 与 MCP

- 可在选定的 SSH 主机管理自定义 Agent、Hook、原生插件设置及 Codex MCP 服务。保存期间新做的编辑会保留，写入失败时保留原文件，已有 MCP 凭据也会保留。私有 Git 凭据排除在命令参数、保存的远程地址和 Skill 备份元数据之外。[SSH Agent 管理](https://github.com/shirenchuang/agentbro/pull/404) · [插件设置](https://github.com/shirenchuang/agentbro/pull/374) · [Codex MCP](https://github.com/shirenchuang/agentbro/pull/294) · [Git 凭据](https://github.com/shirenchuang/agentbro/pull/328)
- MCP 检查器支持协商能力、分页、完整 stdio 与 HTTP 流，以及取消原始 SSH 操作。本机 HTTP 请求保持配置的来源和协商的协议，日志较多的本机服务可继续运行。Windows 上的程序包、Skill 和 MCP 后台操作减少额外控制台弹窗。[SSH MCP](https://github.com/shirenchuang/agentbro/pull/296) · [本机流式响应](https://github.com/shirenchuang/agentbro/pull/310) · [Windows 后台操作](https://github.com/shirenchuang/agentbro/pull/186)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。
- 若已接入 DeepSeek Harness，请先退出客户端，在 AgentBro 中卸载并重新安装它的 Hook，再打开 Harness。
- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和界面无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中持续跟踪。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)
- [自 v3.2.2 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.2.2...v{{VERSION}})
