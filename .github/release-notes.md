# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} adds offline Skill Pack transfers and restores more project-locked Skills. It also improves Qoder approvals, keyboard use in the Island, and Windows startup.

### Highlights

- **Move a Skill Pack with a single ZIP.** Export its Skill files, member order, fingerprints, and recommended Agents together. Preview the import, restore missing Skills, or keep both copies when an existing Skill uses the same ID, including names that differ only in letter case. [#601](https://github.com/shirenchuang/agentbro/pull/601) · [#605](https://github.com/shirenchuang/agentbro/pull/605)
- **Restore and verify more project Skills.** Restore pinned Skills from GitLab and other HTTPS Git hosts. Renamed Skills can be restored under their locked alias, and linked npm Skills can be verified against the locked package and content. [#597](https://github.com/shirenchuang/agentbro/pull/597) · [#591](https://github.com/shirenchuang/agentbro/pull/591) · [#593](https://github.com/shirenchuang/agentbro/pull/593)
- **Qoder keeps the Island in sync.** AgentBro restores a missing or empty Hook bridge at startup. A disconnected approval is retired, and for Qoder Desktop the Island offers to open the app when the response can no longer be returned. [#600](https://github.com/shirenchuang/agentbro/pull/600) · [#585](https://github.com/shirenchuang/agentbro/pull/585) · [#609](https://github.com/shirenchuang/agentbro/pull/609)
- **Answer Island questions by keyboard.** Use Tab, Space, and Enter to select options and submit answers. When a shortcut opens a question, it stays visible while the Island has keyboard focus. [#587](https://github.com/shirenchuang/agentbro/pull/587) · [#589](https://github.com/shirenchuang/agentbro/pull/589)
- **Windows startup recovery and a more reliable welcome screen.** If startup stalls, Windows offers to keep waiting or close AgentBro so you can launch it again. The welcome screen closes after the display choice is saved, and a failed save can be retried. [#595](https://github.com/shirenchuang/agentbro/pull/595) · [#607](https://github.com/shirenchuang/agentbro/pull/607)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- [Full changes since v3.6.0](https://github.com/shirenchuang/agentbro/compare/v3.6.0...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 新增技能包离线迁移，支持恢复更多项目锁定的技能，并改进了 Qoder 审批、灵动岛键盘操作和 Windows 启动。

### 重点更新

- **用一个 ZIP 迁移技能包。** 将技能文件、成员顺序、内容指纹和推荐 Agent 一起导出。导入前可预览、恢复缺失技能，遇到同名或仅大小写不同的技能时可保留两份。[#601](https://github.com/shirenchuang/agentbro/pull/601) · [#605](https://github.com/shirenchuang/agentbro/pull/605)
- **恢复和校验更多项目技能。** 支持从 GitLab 等 HTTPS Git 源恢复锁定版本，按锁文件中的别名恢复重命名的技能，并按锁定的 npm 包版本和内容校验链接安装的技能。[#597](https://github.com/shirenchuang/agentbro/pull/597) · [#591](https://github.com/shirenchuang/agentbro/pull/591) · [#593](https://github.com/shirenchuang/agentbro/pull/593)
- **Qoder 与灵动岛保持同步。** 启动时恢复缺失或损坏为空文件的 Hook 桥接程序。断开的审批会自动撤下，Qoder Desktop 的审批无法回传时，灵动岛会提供打开 Qoder 的入口。[#600](https://github.com/shirenchuang/agentbro/pull/600) · [#585](https://github.com/shirenchuang/agentbro/pull/585) · [#609](https://github.com/shirenchuang/agentbro/pull/609)
- **用键盘回答灵动岛问题。** 支持用 Tab、空格和回车选择选项并提交。快捷键打开的问题会在灵动岛拥有键盘焦点时保持可见。[#587](https://github.com/shirenchuang/agentbro/pull/587) · [#589](https://github.com/shirenchuang/agentbro/pull/589)
- **Windows 启动可恢复，欢迎页保存更稳定。** 启动卡住时，Windows 会提供继续等待或关闭 AgentBro 的选项，方便再次启动。欢迎页会在展示模式保存后关闭，保存失败时可原地重试。[#595](https://github.com/shirenchuang/agentbro/pull/595) · [#607](https://github.com/shirenchuang/agentbro/pull/607)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- [自 v3.6.0 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.6.0...v{{VERSION}})
