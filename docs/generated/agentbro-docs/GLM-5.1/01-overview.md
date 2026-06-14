# 01 — 项目总览

> 评测模型：GLM-5.1 · 生成日期：2026-06-13 · 仓库：`shirenchuang/agentbro`（分支 `dev`，版本 `1.1.3`）
> 所有结论均对应真实文件路径；推断处已显式标注「推断」。

---

## 1.1 AgentBro 是什么

AgentBro 是一个**原生 macOS 桌面悬浮窗（灵动岛 / Dynamic Island）**应用，用来集中收纳 AI 编程 Agent（Claude Code、Codex、Gemini CLI、Cursor、Copilot 等）的运行状态。它把"最容易打断心流的事情"——权限请求、提问、计划审批、工具调用、完成提醒——收进一个低打扰的浮窗里，让你不必切回终端就能处理。

第一性原则（README.md:32、AGENTS.md:8、`.claude/CLAUDE.md` 第 1 节）：

- **本地优先**：Hook Server 默认运行在本机，状态不离开本地。
- **低打扰**：通过智能抑制（终端聚焦时不弹窗）、静默规则、勿扰时段、能量策略等手段降低打扰。

技术栈（来自 `package.json` 与 `src-tauri/Cargo.toml`）：

| 层 | 技术 |
| --- | --- |
| 前端 | React 19 + TypeScript + Vite 8 + Zustand 5 + i18next + framer-motion |
| 后端 | Rust 2021 edition + Tauri 2（macOS 优先，Windows MVP） |
| IPC | Tauri `invoke` / `emit`（`Result<T, String>` 跨边界） |
| 本地 Hook 接入 | Unix socket `/tmp/agentbro-<uid>.sock` + TCP `127.0.0.1:17894` |
| 远程接入 | SSH 反向隧道 / attach 流（Python daemon + hook 脚本） |
| 持久化 | `~/.agentbro/config.json`（ConfigStore）、`~/.agentbro/metadata.json`（技能注册表）、`~/.agentbro/switch.db`（SQLite，Agent Switch）、`~/.agentbro/sounds/` |

> ⚠️ 文档漂移提示：`.claude/CLAUDE.md` 第 1 节写的是 `/tmp/agentbro.sock` 或 `127.0.0.1:17892`；实际代码 `src-tauri/src/hook_endpoint.rs:32-48` 为 `/tmp/agentbro-<uid>.sock` 与端口 **17894**。README.md:76 写法正确。这是 Code Review 风险点之一。

---

## 1.2 核心用户场景

依据 README.md:64-78「主要功能」与代码实际能力：

1. **灵动岛浮窗**：紧凑/悬停/展开/详情多视图查看 Agent 状态。
2. **即时处理**：在浮窗内批准权限、回答问题、审批计划、查看完成提醒与回复卡片。
3. **快速回复**：不切回终端，直接在弹窗输入消息继续对话（`send_message` → 终端 send-keys / Codex app-server `turn/steer`）。
4. **任务感知**：工具调用、Subagent 活动、任务摘要、Token/Rate Limit（支持场景下）。
5. **宠物模式 / 宠物市场**：浮窗切换为宠物状态面板，活力随上下文压力变化；`abpets` CLI 驱动的社区宠物市场。
6. **Hook 集成**：一键安装 Hook + Hook Doctor 诊断 + 自定义 Hook 模板。
7. **桌面体验**：全局快捷键、声音、通知、主题、显示器位置、终端焦点智能降噪。
8. **SSH Remote**：把远程机器上的 Agent 事件转发回本机灵动岛。
9. **Webhook 通知**：钉钉 / 飞书 Webhook（含延迟提醒）。

> ⚠️ 重要文档漂移：README.md:34 声称「第一个开源版本聚焦在灵动岛模块。Agent Monitor、Agent Switch、Skills 管理等更大的模块暂时不会出现在公开版菜单里」。**但代码并非如此**：`src/components/settings/SettingsApp.tsx:139-165` 实际渲染了 `monitor`（`AgentMonitorSection`）、`switch`（`SwitchSection`）和 `agents`（其中包含完整的 Skills 子视图族）。这三个模块在当前 `dev` 分支是**可达**的。详见 04-skill-management 与 05-code-review。

---

## 1.3 项目规模

统计方式：`find` 排除 `node_modules/dist/build/target/.git`。

| 语言 | 文件数 | 行数 |
| --- | --- | --- |
| Rust (`.rs`) | 104 | ~60,936 |
| TypeScript (`.tsx`) | 121 | ~33,029 |
| TypeScript (`.ts`) | 73 | ~12,997 |
| CSS | 50 | ~23,888 |

Tauri 后端（Rust）是体量最大、最复杂的部分；前端以 React 组件 + Zustand store 为主。

---

## 1.4 顶层目录速查

| 路径 | 职责 |
| --- | --- |
| `src/` | 前端 React 应用（灵动岛 UI、设置面板、技能/Switch UI、宠物） |
| `src/components/notch/` | 灵动岛主界面（折叠条、悬停列表、聊天、权限/计划/完成卡片…） |
| `src/components/overlay/` | Overlay 卡片（Permission/Question/Plan/Completion/Compacting/Response） |
| `src/components/settings/` | 设置面板各 Section（General/Island/Agents/Monitor/Switch/About） |
| `src/components/skills/` | 技能管理视图（中央库/全部/插件/集合/包/发现/市场/同步/Obsidian） |
| `src/stores/` | Zustand：`sessionStore`、`configStore`、`agentStore`、`themeStore`、`petStore`、`skillStore`、`switchStore`、`marketStore`、`updateStore`、`petVitalsDebugStore` |
| `src/services/` | Tauri IPC 薄封装：`tauriApi.ts`、`skillApi.ts`、`switchApi.ts`、`agentApi.ts`、`monitorApi.ts` |
| `src/hooks/` | `useTauri.ts`（IPC 事件 Hook 集合）、`useAutoHide`、`useTick`、`useUpdater` |
| `src/i18n/locales/` | 五语言资源 `en/zh/ja/ko/tr.json` |
| `src/themes/` | 主题定义（`default/ink-amber/minimal-dot`，外部主题运行时扫描） |
| `src-tauri/src/` | Rust 后端 |
| `src-tauri/src/agents/` | **每个支持的 AI Agent 一个 `.rs` 文件**（扩展点 #1，22 个适配器） |
| `src-tauri/src/hooks/` | Hook Server（`server.rs`）、`session_store.rs`、`conversation_parser.rs`、`file_watcher.rs`、`recovery.rs`、`diagnostics.rs`、`tool_processor.rs`、`claude_desktop_watcher.rs` |
| `src-tauri/src/commands/` | Tauri IPC 命令（`mod.rs`、`buddy.rs`、`monitor.rs`、`persistence.rs`） |
| `src-tauri/src/skills/` | 技能管理后端（scanner/installer/registry/marketplace/sync/agent_paths/explanation） |
| `src-tauri/src/switch/` | Agent Switch 后端（db/schema/providers/prompts/presets/pricing/usage/health/live_writer/migration/deeplink） |
| `src-tauri/src/remote/` | SSH Remote 转发（manager/attach/ssh_tunnel/installer/ssh_config/path） |
| `src-tauri/src/market/`、`pets/` | 宠物市场与宠物资源 |
| `src-tauri/src/terminal/` | 终端聚焦/jump/approval（tmux send-keys 回退）/registry/suppression/wave |
| `src-tauri/src/webhook/` | 钉钉/飞书 Webhook 转发 |
| `src-tauri/src/platform/` | display/display_controller/idle/monitor_tracker/notifications |
| `src-tauri/src/bridge/main.rs` | `agentbro-bridge` 二进制（编译版本地 Hook 转发器） |
| `src-tauri/src/lib.rs` | 应用入口 + `invoke_handler`（命令注册表，5306 行） |
| `docs/` | 发布、隐私、遥测、设计 demo 等 |

---

## 1.5 主要运行入口

- **前端入口**：`src/main.tsx` → `src/App.tsx`。`App.tsx` 通过 URL hash 或 `getCurrentWindow().label` 区分三个窗口（`notch` / `settings` / `pet`），分别渲染 `NotchPanel`、`SettingsApp`、`PetApp`（`App.tsx:60-90`）。三个窗口加载同一 `index.html`。
- **Rust 入口**：`src-tauri/src/main.rs` → `agentbro_lib::run()`（`src-tauri/src/lib.rs`），在 `run()` 中构建 `AppState`、启动 `HookServer`、`ConversationWatcher`、`NetworkMonitor`、`RemoteManager`、Codex app-server 后台同步，并注册 `tauri::generate_handler![…]`（`lib.rs:5058-5302`）。
- **Bridge 二进制入口**：`src-tauri/src/bridge/main.rs`（`src-tauri/Cargo.toml` 中 `[[bin]] name = "agentbro-bridge"`），是 Agent Hook 调用的本地转发器，把 JSON 经 Unix socket/TCP 发给 `HookServer`。
- **开发命令**：`pnpm dev`（仅浏览器 UI，`http://localhost:1423`）、`pnpm tauri:dev`（先 `build:bridge` 再起原生窗口）。

---

## 1.6 前端 / 后端 / Hook / Agent 适配器之间的关系

```
AI Agent CLI (Claude Code / Codex / …)
   │  触发 Hook 事件，调用 ~/.agentbro/bin/agentbro-bridge 或 Python 脚本
   ▼
[本地] agentbro-bridge (bridge/main.rs)            [远程] remote-hook.py → remote-agent.py
   │  JSON-line over Unix socket / TCP                  │  经 ssh attach 流 / ssh -R 反向隧道
   ▼                                                     ▼
HookServer (hooks/server.rs)  ←─ parse_with_adapters →  AgentAdapter.parse_event
   │  更新 SessionStore (hooks/session_store.rs)
   │  emit "session-update" 事件
   ▼
Tauri 命令 / 事件 (commands/mod.rs, lib.rs)
   │  invoke() / listen()
   ▼
src/services/tauriApi.ts  →  Zustand stores  →  NotchPanel / Overlay / Settings UI
   ▲
   └── 用户操作（批准权限/回复）走 respond_permission 等 command → 回到 HookServer oneshot channel → 写回 Hook 脚本
```

要点：

- **Agent 适配器**（`src-tauri/src/agents/*.rs`，22 个）负责"把某个 Agent 的原始 Hook payload 解析成统一的 `AgentEvent`"，以及"安装/卸载该 Agent 的 Hook 配置"。是扩展点 #1。
- **Hook Server**（`hooks/server.rs`）是事件汇聚中枢：接收 JSON-line，路由到适配器 `parse_event`，更新 `SessionStore`，并在权限/问题/计划场景下用 `oneshot` channel 同步等待 UI 回应再写回 Hook 脚本。
- **Rust command/event** 与前端通过 Tauri IPC 交互：`get_sessions` 拉取、`session-update` 事件推送、`respond_permission` 等回写。
- **前端**通过 `tauriApi.ts` 薄封装调用命令、`useTauri.ts` 监听事件并同步到 Zustand store，UI 组件从 store 取数渲染。

这条链路的精确文件级走查见 `02-architecture.md` 与 `03-data-flow.md`。

---

## 1.7 关键目录与职责表（含核心文件）

| 模块 | 关键文件 | 一句话职责 |
| --- | --- | --- |
| IPC 入口 | `src-tauri/src/lib.rs` | 应用 `run()`、命令注册表 `invoke_handler` |
| 后端状态 | `src-tauri/src/commands/mod.rs` | `AppState` 聚合所有服务句柄 |
| Hook 接入端点 | `src-tauri/src/hook_endpoint.rs` | 计算 socket 路径与 TCP 端口 |
| Hook Server | `src-tauri/src/hooks/server.rs` | 收事件、路由适配器、等待 UI 回应 |
| 会话状态（Rust） | `src-tauri/src/hooks/session_store.rs` | `SessionStore`，emit `session-update` |
| 适配器 trait | `src-tauri/src/agents/traits.rs` | `AgentAdapter` trait（8 必需方法） |
| 适配器注册 | `src-tauri/src/agents/mod.rs` | `all_adapters()`（22 个）+ `AgentEvent` 枚举 |
| Hook 配置注入 | `src-tauri/src/agents/profiles.rs`、`hook_manager.rs`、`toml_hooks.rs` | JSON/YAML/TOML Hook 安装/卸载、事件描述符 |
| CLI 探测 | `src-tauri/src/agents/detection.rs`、`executable.rs` | 探测已安装 Agent（PATH/配置目录） |
| 技能后端 | `src-tauri/src/skills/*` | 扫描/安装/市场/集合/同步/MCP/插件 |
| Switch 后端 | `src-tauri/src/switch/*` | 多 Provider 配置切换（SQLite） |
| 远程后端 | `src-tauri/src/remote/*` | SSH 远程事件转发 |
| Bridge | `src-tauri/src/bridge/main.rs` | 本地编译版 Hook 转发器 |
| 前端 IPC | `src/services/tauriApi.ts` | 全部命令/事件的薄封装 |
| 前端事件 | `src/hooks/useTauri.ts` | 监听 `session-update` 等并同步 store |
| 前端会话状态 | `src/stores/sessionStore.ts` | sessions + baseLayer + overlayQueue 分层状态机 |
| 前端主 UI | `src/components/notch/NotchPanel.tsx` | 灵动岛外壳 + overlay 渲染 |
| 前端设置 | `src/components/settings/SettingsApp.tsx` | 设置窗口 + Section 路由 |
