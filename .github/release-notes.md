# AgentBro v{{VERSION}}

## English

AgentBro v{{VERSION}} makes it easier to move Skill Packs between local and SSH libraries and protects existing Hook settings.

### Highlights

- **Restore missing Skills together.** From a Skill Pack import preview, restore all missing members with recorded GitHub sources in one action. The preview shows which Skills failed and lets you retry them before creating the pack. [#531](https://github.com/shirenchuang/agentbro/pull/531)
- **Keep both versions of a conflicting Skill.** If the library already has different content under the same ID, restore the pinned version under a new ID. The library keeps the original, and the pack uses the restored copy in the same position. [#535](https://github.com/shirenchuang/agentbro/pull/535)
- **Verify GitHub sources when exporting Skill Packs.** Export with GitHub sources keeps a source only when the recorded commit and path reproduce the current Skill. Locally edited Skills still export with their current fingerprints. [#533](https://github.com/shirenchuang/agentbro/pull/533)
- **Preserve unreadable or invalid Hook settings.** Installing, repairing, or removing Hooks in local JSON settings stops with an error that identifies the affected file when its settings cannot be read or validated. [#529](https://github.com/shirenchuang/agentbro/pull/529)

### Install and update

- On macOS, use the universal installer, the in-app updater, or Homebrew with `brew tap shirenchuang/tap && brew install --cask agentbro`.

### Downloads

- macOS universal installer `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 installers `AgentBro_latest_x64-setup.exe` and `AgentBro_latest_x64.msi`
- In-app update files `AgentBro.app.tar.gz` and `latest.json`
- [macOS mirror in China](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### Notes

- The Windows installers are x64 and may show a SmartScreen prompt. Intermittent startup and Island accessibility reports are tracked in [#194](https://github.com/shirenchuang/agentbro/issues/194) and [#351](https://github.com/shirenchuang/agentbro/issues/351).
- The installed x64 app was tested on Windows Server 2025 and Windows 11 ARM, including batch restore, conflict copies, and re-export. [#535](https://github.com/shirenchuang/agentbro/pull/535)
- [Full changes since v3.3.3](https://github.com/shirenchuang/agentbro/compare/v3.3.3...v{{VERSION}})

## 中文

AgentBro v{{VERSION}} 改进了本地和 SSH 技能库之间的技能包迁移，并保护已有的 Hook 配置。

### 重点更新

- **批量恢复缺失技能。** 在技能包导入预览中，可以一次恢复所有记录了 GitHub 来源的缺失项。预览会列出失败项，创建技能包前可以重试。[#531](https://github.com/shirenchuang/agentbro/pull/531)
- **保留同一技能的两个版本。** 如果技能库里已有同 ID 的技能，但内容不同，可以为指定版本分配新 ID 并恢复。技能库会保留原有技能，技能包会在原来的位置使用恢复出的副本。[#535](https://github.com/shirenchuang/agentbro/pull/535)
- **导出技能包时验证 GitHub 来源。** 只有记录的提交和路径能够复现当前技能时，带 GitHub 来源的导出才会保存该来源。本地编辑过的技能仍会导出当前指纹。[#533](https://github.com/shirenchuang/agentbro/pull/533)
- **保留无法读取或无效的 Hook 配置。** 安装、修复或移除 Hook 时，如果本地 JSON 配置无法读取或校验，操作会停止并指出有问题的文件。[#529](https://github.com/shirenchuang/agentbro/pull/529)

### 安装与更新

- macOS 可使用通用安装包、应用内更新，或执行 Homebrew 命令 `brew tap shirenchuang/tap && brew install --cask agentbro`。

### 下载

- macOS 通用安装包 `AgentBro_{{VERSION}}_universal.dmg`
- Windows x64 安装包 `AgentBro_latest_x64-setup.exe` 和 `AgentBro_latest_x64.msi`
- 应用内更新文件 `AgentBro.app.tar.gz` 和 `latest.json`
- [macOS 国内镜像](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

### 说明

- Windows 安装包为 x64，可能出现 SmartScreen 提示。偶发启动和灵动岛无障碍问题在 [#194](https://github.com/shirenchuang/agentbro/issues/194) 和 [#351](https://github.com/shirenchuang/agentbro/issues/351) 中跟踪。
- x64 安装包已在 Windows Server 2025 和 Windows 11 ARM 上实测，覆盖批量恢复、冲突副本和重新导出。[#535](https://github.com/shirenchuang/agentbro/pull/535)
- [自 v3.3.3 以来的完整改动](https://github.com/shirenchuang/agentbro/compare/v3.3.3...v{{VERSION}})
