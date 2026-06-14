# 07 — 证据索引

> 评测模型：GLM-5.1 · 生成日期：2026-06-13
> 列出本次评测实读/引用的关键文件。"读法"列：**直读**=本会话用 Read/Bash 直接读取；**聚焦读**=由子代理按指令聚焦读取并回传带行号的结论，关键事实已由本会话 grep 二次验证；**grep 验证**=仅以 grep/sed 抽取行号佐证。

---

## A. 配置与文档（直读）

| 文件 | 所属模块 | 从中得到的结论 | 引用章节 | 读法 |
| --- | --- | --- | --- | --- |
| `package.json` | 工程根 | 版本 1.1.3；脚本 `dev/tauri:dev/build/test:run`；依赖 React19/Zustand5/Tauri2 | 01, 06 | 直读 |
| `README.md` | 文档 | 灵动岛定位；声称 Monitor/Switch/Skills 不在公开菜单（:34）；端口 17894（:76）；主题表 | 01, 04, 05-风险1 | 直读 |
| `AGENTS.md` | 文档 | 跨 Agent 协作约定；本地命令；扩展点表 | 01, 06 | 直读 |
| `.claude/CLAUDE.md` | 项目指令 | 扩展点路径；"7 个必需方法"（过时）；端口 17892（过时）；禁区 | 01, 05-风险2/3, 06 | 直读 |
| `src-tauri/Cargo.toml` | 后端清单 | `[[bin]] agentbro-bridge`；rusqlite/reqwest/axum/tokio-tungstenite 等依赖；macOS objc2 | 01, 02, 03 | 直读 |
| `src-tauri/tauri.conf.json` | 打包配置 | 三窗口中 notch 透明置顶；deep-link schemes `agentbro`/`ccswitch`；resource `pets/themes/agentbro-bridge` | 01, 02 | 直读 |

## B. Rust 后端核心（直读）

| 文件 | 所属模块 | 结论 | 引用章节 | 读法 |
| --- | --- | --- | --- | --- |
| `src-tauri/src/lib.rs` | 入口+注册表 | `invoke_handler` 命令清单（:5058-5302）；Hook 安装/远程/Webhook/Skill/Switch 命令；`stop_project_scan` no-op（:2609）；skill 命令块（:2479+） | 01, 02, 03, 04, 05-5 | 直读(部分页) |
| `src-tauri/src/hook_endpoint.rs` | Hook 端点 | socket `/tmp/agentbro-<uid>.sock`、TCP 17894、env `AGENTBRO_HOOK_SOCKET/PORT` | 01, 02, 03-5, 05-3 | 直读 |
| `src-tauri/src/hooks/server.rs` | HookServer | `handle_connection`/`parse_with_adapters`/`process_event`；阻塞交互 oneshot；webhook 延迟；`is_remote_hook_event` | 02, 03-1/2/3/4/5, 05-10/11 | 直读(部分页) |
| `src-tauri/src/hooks/session_store.rs` | 会话状态 | `emit_update`→`emit("session-update")`（:874-893）；~20 个 mutator | 02, 03-2 | grep 验证 |
| `src-tauri/src/agents/traits.rs` | 适配器 trait | **8 个必需方法** + `hooks_installed`/`detect_status_now` 默认 | 01, 02, 05-2, 06-4 | 直读 |
| `src-tauri/src/agents/mod.rs` | 适配器注册 | `all_adapters()` **22 个**（:214-239） | 01, 03-4, 05-4, 06-4 | 直读 |
| `src-tauri/src/agents/detection.rs` | CLI 探测 | `detect_installed_tools` 仅 **11 项** | 03-4, 05-4 | 直读 |
| `src-tauri/src/commands/mod.rs` | IPC+AppState | `AppState`（:52-70）；`respond_permission` 三级回退（:2352-2438）；Codex app-server bridge；`send_message` 多路径 | 02, 03-3/6, 06-3 | 直读(部分页) |

## C. Rust 后端子系统（聚焦读 + grep 验证）

| 文件 | 模块 | 结论 | 引用章节 | 读法 |
| --- | --- | --- | --- | --- |
| `src-tauri/src/agents/profiles.rs` | Hook 配置 | `AgentIntegrationProfile`、profile_for_agent、事件描述符、JSON/YAML/TOML 分发、cursor/cursor-cli 同文件 | 02, 03-4, 05-9 | 聚焦读 |
| `src-tauri/src/agents/hook_manager.rs` | Hook 注入 | JSON/YAML/TOML 注入原语 + bridge 部署命令构造 | 02, 03-4 | 聚焦读 |
| `src-tauri/src/agents/toml_hooks.rs` | TOML 注入 | 段解析器，仅 kimi/deepseek 走此 | 02, 03-4 | 聚焦读 |
| `src-tauri/src/agents/executable.rs` | 二进制查找 | PATH 合并（login shell/homebrew/nvm/volta/mise/cargo） | 03-4 | 聚焦读 |
| `src-tauri/src/agents/{kimi,claude_code,codex_app_server}.rs` | 适配器参考 | 最简/复杂参考；codex app-server 是非适配器 helper | 06-4, 02-4 | 聚焦读 |
| `src-tauri/src/skills/{mod,scanner,installer,registry,marketplace,sync,explanation,agent_paths}.rs` | 技能后端 | 8 文件职责；安装路径映射（37 Agent）；metadata.json 真相源；市场仅 skill 类目；explanation 走 OpenAI | 04 全 | 聚焦读 |
| `src-tauri/src/remote/{mod,manager,attach,ssh_tunnel,installer,ssh_config,path}.rs` | 远程转发 | attach 主/ssh-R 回退；内嵌 Python；无应用层认证 | 03-5, 05-8 | 聚焦读 |
| `src-tauri/src/bridge/main.rs` | Bridge 二进制 | 本地 Hook 转发器，不参与远程 | 01, 02, 03-1 | 聚焦读 |
| `src-tauri/src/switch/{commands,db,schema,providers,prompts,presets,pricing,usage,health,live_writer,migration,deeplink,app_type,mod}.rs` | Agent Switch | SQLite 切多 Provider；live_writer 写各 Agent 配置；usage::record_usage 无调用方；Hermes stub | 05-6/7, (背景) | 聚焦读 |

## D. 前端（直读 / 聚焦读）

| 文件 | 模块 | 结论 | 引用章节 | 读法 |
| --- | --- | --- | --- | --- |
| `src/main.tsx` | 入口 | → `App` | 01, 06-3 | 直读 |
| `src/App.tsx` | 窗口路由 | 三窗口按 label/hash 分发；`BACKEND_MANAGED_CONFIG_KEYS`（:23-46） | 01, 02, 03-7, 06-7 | 直读 |
| `src/services/tauriApi.ts` | IPC 封装 | 1682 行；`respondPermission`(:492)/`sendMessage`(:525)/远程(:1531+)/webhook(:1635) 等 | 02, 03-3/5 | grep 验证 |
| `src/hooks/useTauri.ts` | 事件监听 | `useSessionEvents`(:491) listen session-update(:512)→transformSession→replaceAllSessions；config/conversation/market hooks | 02, 03-2 | 直读+grep |
| `src/stores/sessionStore.ts` | 会话状态 | baseLayer+overlayQueue 分层；`activeOverlay=overlayQueue[0]`；pushOverlay 优先级 | 02, 03-2 | 直读+grep |
| `src/components/notch/NotchPanel.tsx` | 灵动岛 | 消费 select*；渲染 overlay 卡片；respond* 回写 | 02, 03-3 | 直读 |
| `src/components/settings/SettingsApp.tsx` | 设置路由 | 实际渲染 monitor/switch/agents（:139-165）→ 与 README 矛盾 | 01, 04, 05-1 | 直读 |
| `src/components/settings/sections/{AgentsSection,AgentMonitorSection,SwitchSection,IslandSection}.tsx` | 设置各域 | Agents 含 Skills 子视图；SwitchSection 三 tab；RemoteTab 在 IslandSection | 04, 06-6 | 聚焦读 |
| `src/components/settings/sections/SkillsSection.tsx` | 死代码 | 未被 SettingsApp 引用；CollectionsView/Obsidian 仅此可达 | 04, 05-1 | 聚焦读 |
| `src/services/skillApi.ts`、`src/stores/skillStore.ts`、`src/components/skills/*` | 技能前端 | ~55 方法；store 无持久化；18 组件 | 04 | 聚焦读 |
| `src/stores/switchStore.ts`、`src/services/switchApi.ts` | Switch 前端 | store+API 封装 26 命令 | (背景) | 聚焦读 |

## E. 验证性 grep 命中（支撑风险点）

| 事实 | 命令/位置 | 结论 |
| --- | --- | --- |
| `stop_project_scan` 为 stub | `lib.rs:2609` `Ok(())` | 05-风险5 |
| `record_usage` 无调用方 | `grep record_usage src-tauri/src/` 仅定义 | 05-风险6 |
| `switch-deep-link` 无前端监听 | `grep switch-deep-link src/` 0 命中 | 05-风险7 |
| `emit("session-update")` 位置 | `session_store.rs:892` | 03-2 |
| 适配器数 vs 探测数 | `mod.rs:214-239`(22) vs `detection.rs:18-37`(11) | 05-风险4 |

---

### 关于推断的说明

本套文档中标注"推断"的少数结论集中在：
- `network_monitor.rs`/`telemetry.rs`/`commands/monitor.rs` 的具体触发与装配细节（02-5）。
- 远程 socket 在 `/tmp` 的抢占风险（03-5、05-8，基于 `StreamLocalBindMask=0000` 与 socket 路径推导）。
- `scan_all` 全量重扫性能（04-7，基于函数结构未见增量缓存）。
- webhook 无重试退避（05 附，基于未见重试逻辑）。
其余结论均直接对应代码行。
