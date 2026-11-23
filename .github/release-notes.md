# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} improves Skill Pack migration, local Claude Code profile support, and SSH distribution safety.

### Highlights

- **Carry Agent recommendations when moving a Skill Pack.** Export for migration records which built-in Agents use the pack. During import into a local or SSH library, you can edit the suggested targets, review the changes, and confirm distribution. On Windows, detected Agent launchers can be selected even before their Skills folders exist. [#540](https://github.com/shirenchuang/agentbro/pull/540)
- **Use the configured Claude Code profile on macOS and Windows.** Local Skill discovery, distribution, and plugin settings follow `CLAUDE_CONFIG_DIR`. Agent Management reports the active paths, and the Skill Pack preview shows the destination for each change. [#542](https://github.com/shirenchuang/agentbro/pull/542)
- **Recheck SSH Skill distribution before writing.** If pack members, source files, destinations, or existing copies change after preview, AgentBro asks for a fresh review. Quick apply with a conflict opens a review before writing, and skipped targets stay out of the recorded pack assignments. [#544](https://github.com/shirenchuang/agentbro/pull/544)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.4.0](https://github.com/shirenchuang/agentbro/compare/v3.4.0...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 改进了技能包迁移、本机 Claude Code 配置目录支持和 SSH 分发保护。

### 重点更新

- **迁移技能包时带上 Agent 推荐。** 迁移导出会记录正在使用技能包的内置 Agent。导入本地或 SSH 技能库后，可以修改推荐目标、预览变更，再确认分发。在 Windows 上，即使还没有创建技能目录，只要检测到 Agent 的启动程序也可以选中它。[#540](https://github.com/shirenchuang/agentbro/pull/540)
- **macOS 和 Windows 使用 Claude Code 当前配置目录。** 本机技能发现、分发和插件设置遵循 `CLAUDE_CONFIG_DIR`。Agent 管理显示当前路径，技能包预览逐项显示分发路径。[#542](https://github.com/shirenchuang/agentbro/pull/542)
- **SSH 技能分发在写入前重新核对。** 如果预览后技能包成员、源文件、目标路径或已有副本有变动，AgentBro 会要求重新预览。一键应用遇到冲突时会先打开确认界面，跳过的目标不会记为已应用。[#544](https://github.com/shirenchuang/agentbro/pull/544)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.4.0 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.4.0...v{{VERSION}})
