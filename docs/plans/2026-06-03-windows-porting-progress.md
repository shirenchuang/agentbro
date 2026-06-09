# Windows 适配进度记录 - 2026-06-03

这份文档记录当前本地 AgentBro fork 的 Windows 适配工作。
它是开发记录，不是正式发布说明。

## 背景

AgentBro 原本是一个 macOS 优先的 Tauri 应用。当前本地目标是做出一个可用于日常开发和验证的 Windows MVP：

- 应用可以在 Windows 上运行；
- 可以在 Windows 上安装、诊断 Agent hooks；
- 快捷键和键盘提示符合 Windows 使用习惯；
- 能正确识别 Codex Desktop、Codex CLI、OpenCode 的 Windows 路径；
- 本地改动后续可以拆分成更小的 PR 提交到上游。

## 已完成的事情

### Windows 构建和 bridge 打包

- 将 `package.json` 中原本依赖 Unix 命令的 bridge 构建脚本改为跨平台 Node 脚本。
- 新增 `scripts/build-bridge.mjs`，用于构建 `agentbro-bridge` 并复制正确的可执行文件到 Tauri resource 目录。
- 新增 Windows 构建命令：
  - `pnpm tauri:build:windows`
  - 使用 Tauri Windows 打包格式 `nsis,msi`。
- 修改 `src-tauri/build.rs`，在 bridge 二进制还没有构建前创建 resource 占位文件，避免 Tauri resource 解析失败。
- 增加 Windows schema/config 支持：
  - `src-tauri/gen/schemas/windows-schema.json`
  - `src-tauri/gen/schemas/desktop-schema.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`

### Windows Hook 传输

- 增加 TCP hook 传输支持。Windows 上不能依赖 Unix domain socket，也不能使用 macOS/tmux 那套 fallback。
- 更新 hook endpoint，使 bridge/plugin 可以收到 `AGENTBRO_HOOK_PORT`。
- 更新 `agentbro-bridge` 的 Windows hook response 处理。
- 修改 Windows 上的审批/自动审批 fallback：如果 TCP hook response 失败，返回明确错误，不再尝试 tmux。
- Hook Doctor 增加 TCP hook server 检查。
- Hook Doctor 文案从 macOS automation only 改为平台相关说明。

主要文件：

- `src-tauri/src/hook_endpoint.rs`
- `src-tauri/src/bridge/main.rs`
- `src-tauri/src/agents/hook_manager.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/hooks/server.rs`

### Hook Doctor 和诊断体验

- 增加 Windows 相关诊断内容：
  - bridge binary；
  - bridge version；
  - hook server TCP；
  - 已安装 hook 数量和具体 agent 名称；
  - 每个 hook profile 的健康状态；
  - Codex CLI 路径；
  - Codex app-server command 可用性；
  - Codex Desktop Windows App ID；
  - Codex auth 路径；
  - 本地 transcript 路径。
- 改进 Windows 路径脱敏。
- Hook Doctor 现在会显示类似 `OpenCode hook profile` 的具体条目，不再只显示 `2 adapter configs` 这种不清楚的信息。

主要文件：

- `src-tauri/src/commands/mod.rs`
- `src/components/settings/sections/IslandSection.tsx`

### Windows 命令和路径兼容

- 增加 Windows 友好的可执行文件查找逻辑。
- 增加 `.cmd` / `.exe` 处理。
- 避免把 Microsoft Store `WindowsApps` alias 当成真正的 Codex CLI。
- 增加 Codex Desktop 的 Windows App ID 和常见安装路径检测。
- 为 Codex Desktop webview assets 增加 Windows allowlist。
- 为账号授权流程增加 Windows terminal 启动逻辑。

主要文件：

- `src-tauri/src/agents/executable.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/tauri.conf.json`

### Codex Desktop 和 Codex CLI

- 区分 Codex CLI 和 Codex Desktop 行为。
- 修复 Windows 上 Codex Desktop 的 `Open app` / session jump：
  - 使用 `explorer.exe shell:AppsFolder\...` 打开 Windows App ID；
  - 不再使用坏掉的 `codex://` 或 `WindowsApps` exe 路径。
- 在 session state 中增加 `codex_app_server_thread_id`，让 app-server-backed session 可以保留真实 thread id。
- 调查 Codex Desktop Windows app-server 行为，确认限制：
  - Codex Desktop 会启动私有 app-server bridge；
  - AgentBro 自己启动的 app-server 不共享这个 session pool；
  - 把 Desktop thread id 发给单独 app-server 会得到 `thread not found`。
- 因此 Windows 上暂时禁用 Codex Desktop 自由文本回复输入框，并返回明确不支持原因。

主要文件：

- `src-tauri/src/agents/codex.rs`
- `src-tauri/src/agents/executable.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/hooks/session_store.rs`
- `src/types/agent.ts`
- `src/utils/sessionCapabilities.ts`
- `src/components/notch/ApprovalBar.tsx`
- `src/components/overlay/OverlayFeedbackPanel.tsx`

### 快捷键和键盘逻辑 Windows 适配

- 新增 `src/utils/platform.ts`。
- 快捷键显示和匹配逻辑改为平台感知：
  - Windows 上将 `Command` / `Meta` 相关展示和逻辑调整为更符合 `Ctrl` 使用习惯；
  - 明确处理 `CommandOrControl`。
- 更新 Windows 快捷键测试。

主要文件：

- `src/utils/platform.ts`
- `src/utils/keyboardShortcuts.ts`
- `src/test/keyboardShortcuts.test.ts`
- `src/stores/configStore.ts`
- `src/test/configStore.test.ts`

### OpenCode 支持

- 确认本机 OpenCode 安装情况：
  - `opencode --version` 为 `1.15.13`；
  - 可执行文件位于 npm 全局路径；
  - 配置目录为 `C:\Users\12159\.config\opencode`；
  - 授权文件为 `C:\Users\12159\.local\share\opencode\auth.json`。
- 修复 OpenCode 配置目录检测：
  - 优先使用 `~/.config/opencode`；
  - 保留 `.opencode` 作为 fallback。
- 修复 OpenCode hook 行的配置目录按钮：
  - 即使只有目录、还没有 hook 文件，也可以打开目录。
- 实现 OpenCode provider 切换：
  - 写入 `~/.config/opencode/opencode.json`；
  - 保留已有 `plugin`；
  - 写入 `provider`、`model`、`small_model`；
  - 使用 OpenCode custom provider 格式；
  - 默认使用 `@ai-sdk/openai-compatible`；
  - 将 AgentBro provider env 字段映射到 OpenCode 的 `options.apiKey` 和 `options.baseURL`。
- 增加 OpenCode 配置写入的 Rust 测试。
- 修复 OpenCode account/usage 状态展示：
  - 不再显示过时的 `~/.opencode`；
  - 能识别 `~/.local/share/opencode/auth.json`；
  - 如果已经发现 auth 文件，就隐藏无意义的授权按钮；
  - 如果缺少 auth，则 Windows 上点击授权会打开 `opencode providers`。
- 调查 OpenCode 运行日志：
  - OpenCode 确实加载了 `agentbro.js`；
  - OpenCode 确实在发布事件；
  - 旧插件静默失败，没有把事件发到 AgentBro bridge。
- 给 OpenCode 生成插件增加 bridge 启动 fallback：
  - 优先尝试 `Bun.spawn`；
  - 失败后 fallback 到 `node:child_process.spawn`；
  - 如果 bridge 启动失败，写入 `[AgentBro]` 开头的 warning，方便排查。

主要文件：

- `src-tauri/src/agents/opencode.rs`
- `src-tauri/src/agents/detection.rs`
- `src-tauri/src/agents/profiles.rs`
- `src-tauri/src/switch/live_writer.rs`
- `src-tauri/src/commands/mod.rs`
- `src/components/settings/sections/IslandSection.tsx`

### Agent programs、配置和 UI 细节

- 更新部分 agent program 元数据，使其路径更适合 Windows。
- 给多个 UI 入口增加 Windows 安全处理。
- 更新 Settings 菜单和灵动岛相关行为。
- 更新新增设置文案和 Windows 提示所需的 i18n 文件。

主要文件：

- `src-tauri/src/agents/programs.rs`
- `src/components/settings/SettingsSidebar.tsx`
- `src/components/settings/sections/IslandSection.tsx`
- `src/App.tsx`
- `src/hooks/useTauri.ts`
- `src/i18n/locales/en.json`
- `src/i18n/locales/zh.json`
- `src/i18n/locales/ja.json`
- `src/i18n/locales/ko.json`
- `src/i18n/locales/tr.json`

### 宠物市场和 Windows 进程问题

- 修复 Windows 上安装宠物时 `npx` spawn 失败的问题。
- 为宠物包安装流程增加 Windows-aware 处理。
- 改进市场下载、安装错误处理和命令执行路径。

主要文件：

- `src-tauri/src/market/mod.rs`
- `src-tauri/src/pets/mod.rs`
- `src-tauri/src/commands/mod.rs`

### Display、Idle、Terminal、Monitor 调整

- 增加 Windows 安全 fallback，用于 display 和 idle 行为。
- 为 terminal wave/jump 相关逻辑增加 Windows 处理。
- 调整 monitor/remote attach 中和 Windows 进程、路径差异相关的代码。

主要文件：

- `src-tauri/src/platform/display.rs`
- `src-tauri/src/platform/idle.rs`
- `src-tauri/src/terminal/wave.rs`
- `src-tauri/src/commands/monitor.rs`
- `src-tauri/src/remote/attach.rs`

## 新增文件

当前工作区新增了这些文件：

- `scripts/build-bridge.mjs`
  - 跨平台 bridge 构建和复制脚本。
- `src-tauri/gen/schemas/windows-schema.json`
  - Windows Tauri schema/config 支持。
- `src/utils/platform.ts`
  - 前端平台检测工具。
- `docs/plans/2026-06-03-windows-porting-progress.md`
  - 本进度记录文档。

## 修改文件

当前 `git diff --name-only` 中的已修改文件：

- `package.json`
- `src-tauri/build.rs`
- `src-tauri/src/agents/claude_code.rs`
- `src-tauri/src/agents/codex.rs`
- `src-tauri/src/agents/detection.rs`
- `src-tauri/src/agents/executable.rs`
- `src-tauri/src/agents/hook_manager.rs`
- `src-tauri/src/agents/profiles.rs`
- `src-tauri/src/agents/programs.rs`
- `src-tauri/src/bridge/main.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/commands/monitor.rs`
- `src-tauri/src/hook_endpoint.rs`
- `src-tauri/src/hooks/server.rs`
- `src-tauri/src/hooks/session_store.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/market/mod.rs`
- `src-tauri/src/pets/mod.rs`
- `src-tauri/src/platform/display.rs`
- `src-tauri/src/platform/idle.rs`
- `src-tauri/src/remote/attach.rs`
- `src-tauri/src/switch/live_writer.rs`
- `src-tauri/src/terminal/wave.rs`
- `src-tauri/tauri.conf.json`
- `src/App.tsx`
- `src/components/notch/ApprovalBar.tsx`
- `src/components/notch/HoverList.tsx`
- `src/components/notch/NotchPanel.tsx`
- `src/components/notch/tips.ts`
- `src/components/overlay/OverlayFeedbackPanel.tsx`
- `src/components/settings/SettingsSidebar.tsx`
- `src/components/settings/sections/IslandSection.tsx`
- `src/components/skills/SkillDetailSlider.tsx`
- `src/hooks/useTauri.ts`
- `src/i18n/locales/en.json`
- `src/i18n/locales/ja.json`
- `src/i18n/locales/ko.json`
- `src/i18n/locales/tr.json`
- `src/i18n/locales/zh.json`
- `src/services/tauriApi.ts`
- `src/stores/configStore.ts`
- `src/test/approvalBarComposer.test.tsx`
- `src/test/configStore.test.ts`
- `src/test/keyboardShortcuts.test.ts`
- `src/test/notchPanel.test.tsx`
- `src/test/overlayFeedbackComposer.test.tsx`
- `src/test/sessionCapabilities.test.ts`
- `src/test/settingsIslandMenu.test.tsx`
- `src/types/agent.ts`
- `src/utils/keyboardShortcuts.ts`
- `src/utils/sessionCapabilities.ts`

## 删除文件

没有删除源代码文件。

没有主动删除 secrets、签名文件、法律文件或品牌资源。

## 移除或禁用的行为

以下行为没有删除文件，但在 Windows 上被禁用或改成更明确的错误路径：

- Windows 上不再使用 tmux fallback 处理审批/自动审批；
- Windows 上禁用 Codex Desktop 自由文本回复注入；
- Windows Hook Doctor 不再要求 macOS automation 权限；
- 不再把 `WindowsApps` alias 当作 Codex CLI 可执行文件。

## 观察过的本机用户配置

下面这些文件用于排查 OpenCode，但它们不是仓库文件：

- `C:\Users\12159\.config\opencode\opencode.json`
- `C:\Users\12159\.config\opencode\opencode.jsonc`
- `C:\Users\12159\.config\opencode\plugins\agentbro.js`
- `C:\Users\12159\.local\share\opencode\auth.json`
- `C:\Users\12159\.local\share\opencode\log\*.log`

当前重要结论：

- OpenCode 已经加载 AgentBro 插件；
- 旧生成插件没有成功把事件送到 AgentBro；
- 新生成插件已经加入 Node spawn fallback；
- 需要重新安装 OpenCode hook，并重启 OpenCode，才能让新的插件代码生效。

## 已执行的验证

通过的命令：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml opencode_ --lib --no-run
pnpm lint
pnpm test:run src/test/sessionCapabilities.test.ts src/test/approvalBarComposer.test.tsx src/test/overlayFeedbackComposer.test.tsx
pnpm build
```

说明：

- `pnpm test:run` 和 `pnpm build` 在沙箱内遇到 Vite/Rolldown 的 Windows `spawn EPERM`，所以在沙箱外重跑后通过。
- 完整 Rust 测试运行时，本地 Windows 环境启动 Tauri lib test binary 出现 `STATUS_ENTRYPOINT_NOT_FOUND`。过滤测试使用 `--no-run` 可以编译通过，`cargo check` 也通过。
- `pnpm build` 通过，但保留了原本存在的 bundle size / dynamic import warning。

## 还没有做到的事情

### OpenCode 运行时验证

还没有完全完成。

下一步需要：

1. 重启 AgentBro 或 `pnpm tauri:dev`；
2. 在 OpenCode hook 行点击重新安装；
3. 完全关闭并重新打开 OpenCode；
4. 新建一个 OpenCode 会话；
5. 确认灵动岛出现 OpenCode 会话。

如果仍然不出现，需要检查 OpenCode 日志中是否有 `[AgentBro]` 开头的错误：

```text
C:\Users\12159\.local\share\opencode\log
```

### OpenCode 用量读取

还没有实现。

现在 UI 能正确识别 OpenCode 配置和 auth 路径，但 AgentBro 还不会解析 OpenCode usage/quota 数据。当前状态仍然是“已知策略，但 usage reader 未接入”。

### Codex Desktop Windows 回复

还没有实现。

当前行为是有意禁用，并显示明确错误。原因是 Codex Desktop 在 Windows 上使用私有 app-server session pool，AgentBro 目前不能安全地往 Codex Desktop 注入新的用户消息。

### Windows 打包完整 QA

还没有完成。

仍需验证：

- `pnpm tauri:build:windows`；
- NSIS 安装包；
- MSI 安装包；
- 安装流程中的 WebView2 runtime 检测；
- 在干净 Windows 机器上的安装/卸载 smoke test；
- 安装后的开机启动 smoke test；
- DPI 缩放、多显示器、overlay 位置和置顶行为。

### 所有 Agent 的 Hook 覆盖

只完成了一部分。

重点测试或修改过：

- Codex CLI；
- Codex Desktop；
- OpenCode；
- Claude 相关路径；
- 一些通用 hook 路径。

仍需专门验证：

- Gemini CLI；
- Cursor / Cursor CLI；
- Cline；
- Copilot；
- Qwen / Kimi / DeepSeek；
- custom hook installs。

### 上游 PR 准备

还没有完成。

当前本地改动太大，不适合一次性提交一个 PR。建议后续拆分为：

1. `feat: add windows command/path compatibility`
2. `feat: add windows hook tcp transport`
3. `fix: improve windows hook doctor diagnostics`
4. `feat: support opencode config and hook forwarding on windows`
5. `build: add windows tauri packaging support`
6. `fix: adapt frontend shortcuts and composer capability by platform`

### 文档和 README 更新

还没有完全完成。

主 README 仍然是 macOS 优先说明。等 Windows MVP 更稳定后，需要更新平台支持文档。

### fork 重新命名

还没有做。

项目说明中提到，如果 fork 后重新分发，需要更换产品名称。目前这次 Windows 适配没有进行产品重命名。

## 建议下一步

1. 重新安装 OpenCode hook，并测试新的 OpenCode 会话。
2. 运行 `pnpm tauri:build:windows`。
3. 本地验证 NSIS/MSI 安装包。
4. 在真实 OpenCode 会话后导出诊断，确认 Hook Doctor 明确显示 OpenCode。
5. 提交上游前，将当前改动拆分成更小的 commit 和 PR。
