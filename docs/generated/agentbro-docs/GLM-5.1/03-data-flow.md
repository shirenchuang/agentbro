# 03 — 关键数据流

> 评测模型：GLM-5.1 · 生成日期：2026-06-13
> 每条数据流标注涉及的关键文件:行号。代码已确认与推断分开标注。

---

## 3.1 数据流 A：Hook 事件如何从 Agent 侧进入 AgentBro

```
Agent 触发 Hook
  → 本地: agentbro-bridge（读 stdin JSON, stamp agent 字段, 发 socket）
  → 远程: remote-hook.py → remote-agent.py daemon → ssh attach / ssh -R
  → HookServer.handle_connection 读一行 JSON
  → parse_with_adapters（精确匹配 raw["agent"]，回退遍历适配器）
  → AgentEvent
```

关键文件：
- 本地 Bridge：`src-tauri/src/bridge/main.rs:440-480`（连接）、`:681-744`（main）、`:747`（stamp `agent`）；声明 `src-tauri/Cargo.toml:14-16`。
- 接入端点解析：`src-tauri/src/hook_endpoint.rs:18-48`（socket `/tmp/agentbro-<uid>.sock`、TCP 17894）。
- HookServer：`src-tauri/src/hooks/server.rs:769`（`start` 同时监听 socket+TCP）、`:873-930`（`handle_connection`）、`:1487-1506`（`parse_with_adapters` 两段式路由）。
- Hook 配置注入（决定 Agent 调用什么命令）：`src-tauri/src/agents/profiles.rs`、`hook_manager.rs`（JSON/YAML）、`toml_hooks.rs`（TOML 段解析）。
- 远程路径：`src-tauri/src/remote/installer.rs:564+`（hook 脚本）、`:1086+`（daemon）、`remote/attach.rs:143-294`（attach 流→本地 socket）、`remote/ssh_tunnel.rs:104-141`（`ssh -N -R` 回退）。

协议：**JSON-line**（一行一个 JSON 对象）。对 `PermissionRequest`/`AskQuestion`/`PlanApproval`，HookServer 会**阻塞等待** UI 回应后写回一行 JSON 给 Hook 脚本（`server.rs:1007-1067` 权限 oneshot 流程）。

---

## 3.2 数据流 B：Session 状态如何更新并展示到 UI

**后端侧（真相源在后端）**：
1. `HookServer.process_event`（`server.rs:1509-2169`）把 `AgentEvent` 写入 `SessionStore`（例如 `store.update_phase`、`store.set_pending_permission`、`store.add_tokens`、`store.set_rate_limits`）。
2. 每个 mutator 末尾 `emit_update()`（`hooks/session_store.rs:874`）→ `handle.emit("session-update", &payload)`（`:892`），payload 含全部 sessions + `suppressed` 标志。

**前端侧**：
3. `useTauri.ts:useSessionEvents()`（`:491`，`listen('session-update')` 在 `:512`） → `event.payload.sessions.map(transformSession)`（`BackendSession`→前端 `SessionState`，含 diff/toolInput 解析，`useTauri.ts:76-271` 附近）。
4. `store.replaceAllSessions(transformed, { suppressed })`（`sessionStore.ts`）重建 sessions 与 overlayQueue。
5. `NotchPanel.tsx` 经 `selectSessionList / selectActiveOverlay / selectPanelState`（`sessionStore.ts:369-370`）取数渲染。

**分层状态机**（`sessionStore.ts:27-72`）：
- `baseLayer: 'compact' | 'list' | 'detail'`：折叠条/列表/详情底层。
- `overlayQueue: OverlayItem[]`：权限/问题/计划/完成/压缩/回复卡片队列，按 `OVERLAY_PRIORITY`（`types/agent.ts`）排序，`activeOverlay = overlayQueue[0]`（`sessionStore.ts:753, 1173-1187`）。
- `updateSession(event)`（`sessionStore.ts` 内）在收到 `pendingPermission`/`pendingQuestion`/`planTitle`/completion 时 `pushOverlay`（`:555-728`）。

**持久化**：前端 `saveSessionsDebounced`（`sessionStore.ts:22-25`，1s 防抖）→ `tauriApi.saveSessions` → Rust `save_sessions`（`commands/persistence.rs`）。启动时 `get_sessions` 恢复。

---

## 3.3 数据流 C：权限/问题/计划/完成提醒 → Overlay/Notch

阻塞型交互（permission/question/plan）有**同步回环**：

```
HookServer 收 PermissionRequest
  → set_pending_permission(session, Some(...))  → emit "session-update"
  → 建 oneshot::channel, 存入 pending_permissions, 阻塞等待 rx
前端 sessionStore 收到 pendingPermission → pushOverlay(permission)
  → NotchPanel 渲染 PermissionCard
用户点 Approve/Deny
  → respondPermission(sessionId, allowed)  (tauriApi.ts:492)
  → Rust respond_permission (commands/mod.rs:2352)
      → 先 CodexAppServerBridge.respond_permission (commands/mod.rs:2367-2379)
      → 再 HookServer.respond_permission（oneshot tx 发 PermissionReply）
      → 失败回退 tmux send-keys (commands/mod.rs:2396-2434)
  → HookServer 写回 JSON 给 Hook 脚本 (server.rs:1027-1058)
  → set_pending_permission(None), update_phase(Processing)
```

- 权限：`server.rs:931-1068`；命令 `respond_permission` `commands/mod.rs:2352-2438`；UI `components/overlay/PermissionCard.tsx`。
- 问题：`server.rs:1069-1181`；命令 `respond_question`；UI `QuestionCard.tsx`。
- 计划：`server.rs:1182-1263`；命令 `respond_plan`；UI `PlanApprovalCard.tsx`。
- 完成/回复/压缩（非阻塞）：`process_event` 的 `TaskComplete`/`AssistantResponseComplete` 分支（`server.rs:1808-1901`）→ 前端 `pushOverlay(completion/response/compacting)`。

**抑制（低打扰）**：`check_suppression`（`server.rs` 内）检查终端是否聚焦（`terminal/suppression.rs`），若聚焦则 `emit_update_suppressed(true)` 并发系统通知而非浮窗；前端收到 `suppressed` 时不自动展开（`useTauri.ts:521-530`）。

**Webhook 旁路**：`dispatch_webhook_event`（`server.rs:570-642`）在权限/问题/计划/完成时按 `webhook_configs` 过滤转发钉钉/飞书，支持延迟提醒（`schedule_delayed_webhook` `server.rs:645-698`，仅在仍 pending 时发送）。

---

## 3.4 数据流 D：Agent 检测与适配器如何工作

**检测（哪些 Agent 已安装）**：
- `detect_installed_tools()`（`agents/detection.rs:18-37`）探测 **11 个** Agent（claude-code, codex, gemini, cursor, copilot, qoder, codebuddy, qwen, kimi, deepseek, opencode）。
- `find_binary`（`agents/executable.rs`）合并进程 PATH + 登录 shell PATH（`$SHELL -lc`）+ homebrew/nvm/volta/mise/cargo 目录。
- `detect_status_now()`（`traits.rs:31-33`）在安装入口前重新探测（避免缓存值阻止新装 CLI 安装），`lib.rs:387-397` 的 `ensure_installable` 用它。

> ⚠️ 覆盖缺口：注册了 22 个适配器（`agents/mod.rs:214-239`），但 `detect_installed_tools` 只覆盖 11 个（见 05-code-review 风险 4）。其余 11 个（cline/cursor-cli/qoder-cli/codebuddycn/droid/stepfun/antigravity/workbuddy/hermes/pi/kiro）靠各自 `status()`/`is_installed()` 自检，不出现在该函数结果里。

**事件解析（adapter.parse_event）**：
- `parse_with_adapters`（`server.rs:1487-1506`）：若 `raw["agent"]` 存在，`canonical_agent_id` 归一化（如 `claude→claude-code`，`server.rs` 内）后精确匹配；否则回退遍历所有适配器取第一个 `Ok`。
- 约定：各适配器 `parse_event` 应在 `agent` 字段不匹配时**早返回 Err**，避免在回退遍历里"抢"到别的 Agent 的事件（`claude_code.rs` 即如此，参考实现）。

**Hook 安装/卸载**：
- `install_agent_hook`（`lib.rs:405-464`）/ `uninstall_agent_hook`（`:553-600`）/ `get_all_hook_status`（`:652-777`，含自定义 hook 条目）。
- 走 `profiles.rs` 的 `AgentIntegrationProfile`，JSON/YAML/TOML 分别由 `hook_manager.rs` / `toml_hooks.rs` 处理。

---

## 3.5 数据流 E：远程 SSH / forwarding 链路

代码中**确认存在**完整的远程转发（`src-tauri/src/remote/*`）：

**远程侧部署物（经 SSH base64 上传）**：
- `~/.agentbro/remote-agent.py`：常驻 daemon，在远程开两个 socket——hook socket（事件入口）与 control socket（本地 attach 连接点），权限 `0o700`（`installer.rs:1086+`、`:1140`）。
- `~/.agentbro/remote-hook.py`：每个事件调用，读 stdin，发 JSON 到远程 hook socket，等待可选回应（`installer.rs:564+`）。
- 远程 Agent 配置注入：`configure_hooks`（`installer.rs:368-455`）把 `AGENTBRO_SOCKET=<remote_socket> AGENTBRO_HOST_ID/NAME/AGENT=... python3 remote-hook.py` 写进远程配置。

**两条传输路径**（`manager.rs:171-272`）：
1. **attach（主）**：`ensure_remote_agent_running` 启远程 daemon → `attach.connect` spawn `ssh <target> python3 remote-agent.py --mode attach`，attach 进程把 control socket 的 JSON 经 ssh stdout 流回本地 → `attach.rs:143-294` 读行 → `forward_to_local_hook_server` 连**本地** socket（`attach.rs:250-294`）。回应经 `send_attach_response` 写回 ssh stdin。
2. **ssh -R 反向隧道（回退）**：daemon/attach 失败时 `cleanup_remote_socket` 后 `tunnel.connect` spawn `ssh -N -R <remote_socket>:<local_socket>`（`ssh_tunnel.rs:104-141`）。

**接入本地 HookServer**：两条路径最终都把 payload 送到 `local_socket_path`（= `hook_endpoint::current().socket_path`，`lib.rs:4977-4978` 注入 `RemoteManager::new`），后续与本地 Hook 完全相同的处理链。远程事件通过 `_remote_host_id`/`_remote_host_name` 字段标记（`server.rs:2820-2822` 的 `is_remote_hook_event`），并被排除本地 PID/TTY 校验（`server.rs:2727-2731`）。

**前端**：`services/tauriApi.ts:1499-1611`（RemoteHost 类型与 13 个远程命令封装）、`stores/configStore.ts`（`remoteHostEntries` 状态）、`components/settings/sections/IslandSection.tsx` 的 `RemoteTab`（`:2492+`，新增主机默认 `remoteSocketPath: '/tmp/agentbro-remote.sock'` `:2617`）。

**安全（确认）**：
- 所有 ssh 用 `BatchMode=yes` + `StrictHostKeyChecking=accept-new`，认证完全委托 SSH key/agent，**无应用层 token/HMAC**。
- 正常路径不暴露本地 TCP 17894（目标是本地 Unix socket；TCP 分支仅 `#[cfg(not(unix))]`，`attach.rs:259-265`）。
- **推断风险**：远程 socket 默认在 `/tmp`，ssh `-R` 用 `StreamLocalBindMask=0000`（`ssh_tunnel.rs:118-119`），多用户远程主机上能写 `/tmp` 者可能抢占/伪造该 socket 路径，详见 05-code-review 风险 8。

---

## 3.6 数据流 F：Codex app-server 第二链路（仅 Codex.app）

`commands/mod.rs:304-351`（启动）、`:1010-1107`（监控循环）、`agents/codex_app_server.rs`（spawn `codex app-server --listen ws://...`）：
- 持久 WebSocket 同步 Codex.app 桌面 thread（`thread/list`、`thread/status/changed`、`thread/started` 等 notification，`commands/mod.rs:1195-1255`）。
- 权限审批请求（`item/commandExecution/requestApproval` 等）经 `handle_codex_app_server_request`（`:1257-1382`）写入 `SessionStore.pending_permission`。
- 回应经 `CodexAppServerBridge` 的 mpsc/oneshot（`:97-209`），优先于 Hook 链路。

---

## 3.7 数据流 G：Config 多窗口同步

- `update_config` / `set_*` 命令写 `ConfigStore`（`config/mod.rs`）后 emit `config-changed`。
- 前端 `useConfigSync()`（`useTauri.ts:547`）监听并更新 `configStore`。
- 因 notch 与 settings 两个窗口各自运行 React，`App.tsx` 维护 `BACKEND_MANAGED_CONFIG_KEYS`（`App.tsx:23-46`）避免 localStorage 跨窗口快照回放覆盖后端管理的字段。
