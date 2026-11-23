# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} improves reliability when managing Agents and Skills on macOS and Windows.

### Highlights

- **Keyboard focus stays inside confirmations.** Agent and Skill dialogs prevent clicks and keyboard input from reaching the page behind them. Closing a dialog restores focus to the control that opened it when it is still available. [#494](https://github.com/shirenchuang/agentbro/pull/494)
- **Retry custom Agent saves without retyping.** A failed save keeps your entries and displays the error in the form. The dialog stays open while saving, and the SSH folder picker returns to the same form. [#496](https://github.com/shirenchuang/agentbro/pull/496)
- **Use Chinese and other Unicode names for custom Agents.** Names work on local and SSH runtimes, including when two display names share the same Latin text. Previously registered Agents keep their identifiers. [#498](https://github.com/shirenchuang/agentbro/pull/498)
- **Protect local Agent and Skill settings while saving.** Overlapping updates preserve each other's changes, failed writes leave the previous metadata file intact, and concurrent backup imports use separate temporary folders. [#500](https://github.com/shirenchuang/agentbro/pull/500)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.3.0](https://github.com/shirenchuang/agentbro/compare/v3.3.0...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 改进了 macOS 和 Windows 上 Agent 管理与 Skills 的稳定性。

### 重点更新

- **确认弹窗内的键盘焦点更稳定。** Agent 和 Skill 弹窗打开时，鼠标和键盘操作留在弹窗内。关闭时，如果打开弹窗的控件仍然可用，焦点会回到该控件。[#494](https://github.com/shirenchuang/agentbro/pull/494)
- **自定义 Agent 保存失败后可直接重试。** 表单会保留输入并就地显示错误。保存过程中弹窗保持打开，SSH 目录选择结束后也会回到同一个表单。[#496](https://github.com/shirenchuang/agentbro/pull/496)
- **自定义 Agent 可使用中文和其他 Unicode 名称。** 本机和 SSH 主机均支持，能区分包含相同拉丁字母的不同名称。已登记的 Agent 保留原标识。[#498](https://github.com/shirenchuang/agentbro/pull/498)
- **保存时保护本机 Agent 和 Skill 设置。** 同时发生的更新会保留各自的修改，写入失败时保留上一次的元数据文件，并发导入备份时分别使用独立的临时目录。[#500](https://github.com/shirenchuang/agentbro/pull/500)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.3.0 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.3.0...v{{VERSION}})
