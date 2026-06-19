# 05 — 全库 Code Review

> 评测模型：GLM-5.1 · 生成日期：2026-06-13
> 以下 12 个风险点均经代码验证（文件:行可复现）。严重程度为本评测判断。

---

## 风险 1 · README 与代码严重漂移：Monitor/Switch/Skills 实际可达，文档声称"不在公开菜单"

- **严重程度：中**（影响用户预期与贡献者认知）
- **相关文件**：`README.md:34`（声称不在公开版菜单）；`src/components/settings/SettingsApp.tsx:139-165`、`SettingsSidebar.tsx`、`sections/AgentMonitorSection.tsx`、`sections/SwitchSection.tsx`、`sections/AgentsSection.tsx`、`components/skills/*`
- **为什么是风险**：README 是用户/贡献者的第一信息源。代码实际渲染了 `monitor`、`switch`、`agents`(含 Skills 全套) 三个 Section，与文档矛盾，导致功能被低估、贡献者按过时描述判断范围。
- **如何验证**：打开 `SettingsApp.tsx` 看 `activeSection` 分支（139-165 行确实有 `monitor`/`switch`/`agents`）；运行 `pnpm tauri:dev` 打开设置可见这三个导航项。
- **建议修复方向**：更新 README「主要功能」与路线图措辞，标注这些模块"已实现但在持续打磨"，或加 feature-flag 说明当前可见性策略。

## 风险 2 · `.claude/CLAUDE.md` 扩展点描述与 trait 实际不符

- **严重程度：中**（直接影响新增 Agent 的贡献者）
- **相关文件**：`.claude/CLAUDE.md` §3.1「实现 7 个必需方法」；`src-tauri/src/agents/traits.rs:7-33`；`AGENTS.md:31`
- **为什么是风险**：trait 实际有 **8 个必需方法**（多了 `hook_config_paths`）+ `detect_status_now` 默认实现（注释要求覆盖）。按文档只写 7 个会编译失败或漏 `detect_status_now`。
- **如何验证**：`sed -n '1,34p' src-tauri/src/agents/traits.rs` 看到 8 个无默认体的方法。
- **建议修复方向**：CLAUDE.md/AGENTS.md 改为"8 个必需方法（含 `hook_config_paths`）+ 建议覆盖 `detect_status_now`"，并更新示例引用。

## 风险 3 · Hook 端点文档与代码不一致（端口/路径）

- **严重程度：低**
- **相关文件**：`.claude/CLAUDE.md` §1（`/tmp/agentbro.sock` 或 `127.0.0.1:17892`）；`src-tauri/src/hook_endpoint.rs:32-48`（实际 `/tmp/agentbro-<uid>.sock` 与端口 **17894**）；`README.md:76`（写法正确）
- **为什么是风险**：贡献者/排查者按 CLAUDE.md 的 17892 找不到监听端口，按 `/tmp/agentbro.sock` 找不到 socket 文件，徒增排障成本。
- **如何验证**：`grep -n "1789" src-tauri/src/hook_endpoint.rs` → 17894；`grep -rn "agentbro.sock" .claude/`。
- **建议修复方向**：同步 CLAUDE.md 的端点描述与 hook_endpoint.rs 一致。

## 风险 4 · Agent 检测覆盖缺口：22 个适配器只自动探测 11 个

- **严重程度：中**
- **相关文件**：`src-tauri/src/agents/mod.rs:214-239`（22 个适配器）；`src-tauri/src/agents/detection.rs:18-37`（`detect_installed_tools` 仅 11 项）
- **为什么是风险**：前端"自动检测已安装 Agent"若调用 `detect_tools`→`detect_installed_tools`，会漏报 cline/cursor-cli/qoder-cli/codebuddycn/droid/stepfun/antigravity/workbuddy/hermes/pi/kiro 这 11 个；用户以为不支持。
- **如何验证**：对比 `all_adapters()` 列表与 `detect_installed_tools()` 列表。
- **建议修复方向**：让 `detect_installed_tools` 遍历 `all_adapters()` 或补充缺失项；或文档说明这些适配器靠各自 `status()` 自检。

## 风险 5 · `stop_project_scan` 是空实现（no-op stub）

- **严重程度：中**
- **相关文件**：`src-tauri/src/lib.rs:2609-2611`（`async fn stop_project_scan() -> Result<(), String> { Ok(()) }`）；`src/services/skillApi.ts`（`stopProjectScan`）；`src/components/skills/DiscoverView.tsx`
- **为什么是风险**：UI"停止扫描"按钮点了无任何效果。`discover_project_skills` 是同步阻塞 walk（无任务句柄/取消令牌），用户无法中止大型扫描。
- **如何验证**：读 `lib.rs:2609`；`grep -rn "stop_project_scan"`。
- **建议修复方向**：要么实现真正的可取消扫描（后台任务 + CancellationToken），要么移除前端按钮以免误导。

## 风险 6 · Switch usage 用量统计是死写路径（UI 永远显示 0）

- **严重程度：中**
- **相关文件**：`src-tauri/src/switch/usage.rs:51-68`（`record_usage` 定义，表 `usage_logs` + 4 个聚合查询齐全）；`lib.rs:5295-5300`（usage 查询命令已注册）；`src/components/settings/sections/switch/SwitchUsagePanel.tsx`
- **为什么是风险**：`record_usage` **无任何调用方**（grep 全仓仅定义处），意味着 token/成本从不写入；`SwitchUsagePanel` 永远空数据。
- **如何验证**：`grep -rn "record_usage" src-tauri/src/` → 只有 `fn record_usage` 一处。
- **建议修复方向**：在 HookServer 的 `TokenUsage`/`RateLimitUpdate` 分支（`hooks/server.rs:1928-1978`）接入 `record_usage`，或显式标注该功能未上线并隐藏 UI。

## 风险 7 · Switch deep-link 后端 emit 但前端无监听

- **严重程度：低-中**
- **相关文件**：`src-tauri/src/lib.rs:5044-5052`（emit `"switch-deep-link"`）；`src-tauri/src/switch/deeplink.rs:10-42`（解析 `agentbro://` 与 `ccswitch://`）；`tauri.conf.json:62-66`（schemes）；`src/`（无 `listen('switch-deep-link')`）
- **为什么是风险**：通过链接导入 provider 的流程在后端打通但前端无人消费，`ccswitch://` 兼容链接形同虚设；用户分享的链接点了无反应。
- **如何验证**：`grep -rn "switch-deep-link\|switchDeepLink" src/` → 0 命中。
- **建议修复方向**：在 `switchStore`/`SwitchSection` 加 `listen('switch-deep-link')`，调用 `importPreview`/`import` 流程。

## 风险 8 · 远程 SSH 转发无应用层认证，远程 socket 在 /tmp 可被抢占

- **严重程度：中**（安全，场景：多用户远程主机）
- **相关文件**：`src-tauri/src/remote/ssh_tunnel.rs:104-141`（`StreamLocalBindMask=0000` + `StreamLocalBindUnlink=yes`）、`:109-119`；`src-tauri/src/remote/installer.rs:1140`（daemon socket `0o700` 但在 `/tmp` 下）；`src-tauri/src/remote/attach.rs:110-141`（`BatchMode=yes`、`StrictHostKeyChecking=accept-new`，无 token/HMAC）
- **为什么是风险**：newline-JSON 协议无签名/序列号；多用户远程主机上，能写 `/tmp` 的攻击者可抢占 `agentbro-remote.sock` 路径，伪造/拦截 Hook 事件或回应。单用户开发机风险低。
- **如何验证**：读 `ssh_tunnel.rs:104-141` 与 `installer.rs` 的 daemon socket 创建处。
- **建议修复方向**：远程 socket 放到 `$XDG_RUNTIME_DIR` 或用户私有目录（0700 父目录）；协议加 HMAC/nonce；文档明确"仅信任单用户主机"。

## 风险 9 · `cursor` 与 `cursor-cli` 写同一配置文件 `.cursor/settings.json`，安装策略不同

- **严重程度：中**
- **相关文件**：`src-tauri/src/agents/profiles.rs`（cursor 用 `CursorSettings`，cursor-cli 用 flat JSON hooks，二者 `configuration_path` 均指向 `.cursor/settings.json`）
- **为什么是风险**：两个适配器对同一文件用不同 install kind，`reinstall_all_hooks`/`uninstall_all_hooks`（`lib.rs:789-887`）顺序执行时可能互相覆盖/残留，导致 Hook 状态不一致。
- **如何验证**：对比 `cursor_profile` 与 `cursor_cli_profile` 的 `configuration_path` 与 `installation_kind`。
- **建议修复方向**：合并为一个适配器或明确两者职责互斥，并在 bulk 操作里去重同路径。

## 风险 10 · 阻塞型交互超时窗口差异大，易误判"卡住"

- **严重程度：低-中**
- **相关文件**：`src-tauri/src/hooks/server.rs:37-38, 304-318`（`DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS=300`，codex/claude/opencode 用 `HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS=21600` 即 6 小时）
- **为什么是风险**：同一 UI 对不同 Agent 的 pending 卡片，超时回收时机差 6h vs 5min；非 codex/claude 的 Agent 5 分钟后卡片消失但用户还没决定，体验不一致。
- **如何验证**：读 `interaction_response_timeout`（`server.rs:304-318`）。
- **建议修复方向**：统一可配置超时，或前端显式标注剩余时间。

## 风险 11 · 适配器 `parse_event` 回退遍历存在"抢占"风险，依赖约定而非强制

- **严重程度：中**
- **相关文件**：`src-tauri/src/hooks/server.rs:1487-1506`（`parse_with_adapters`：精确匹配失败后遍历所有适配器取首个 `Ok`）；各 `agents/*.rs` 的 `parse_event`
- **为什么是风险**：若某适配器 `parse_event` 对非自身 payload 也返回 `Ok`（如只看通用 `status` 字段），会在回退链里错误吞掉别的 Agent 的事件。目前靠"agent 字段不匹配早返回 Err"的**约定**保证（`claude_code.rs:222-224` 即此模式），无编译期强制。
- **如何验证**：审各适配器 `parse_event` 是否都遵守"非自身 agent 返回 Err"；`grep -n "agent" src-tauri/src/agents/*.rs`。
- **建议修复方向**：在 trait 层提供 `matches(raw)->bool` 显式门控，`parse_event` 仅在 `matches` 后调用；或加测试覆盖跨适配器 payload。

## 风险 12 · 测试覆盖缺口：Hook 解析边界与跨适配器路由缺回归测试

- **严重程度：中**
- **相关文件**：`src-tauri/src/agents/*.rs`（仅 `kimi.rs`、`claude_code.rs` 等有 `mod tests`）；前端 `src/test/`（覆盖 sessionStore/overlay/layout/i18n/priority 等，但跨"Hook→adapter→overlay"端到端映射测试稀薄）
- **为什么是风险**：CLAUDE.md §8 要求"修 Hook 解析 bug 先加回归测试再改实现"，但不少适配器（如 codebuddycn/droid/stepfun/antigravity/workbuddy/pi/kiro）无 `mod tests`，事件解析漂移难被捕获；`parse_with_adapters` 的回退路由无专项测试。
- **如何验证**：`grep -L "mod tests" src-tauri/src/agents/*.rs`；`cargo test --manifest-path src-tauri/Cargo.toml` 看覆盖的适配器。
- **建议修复方向**：为每个适配器补 `parse_event` 黄金路径 + 边界测试；为 `parse_with_adapters` 的精确/回退两段加 fixture 测试。

---

### 附：其它值得关注的轻量项（低）

- `MarketSection`/宠物市场与 `abpets` CLI 强耦合（`market/mod.rs`），CLI 缺失时报错路径较粗。
- `webhook` secret 经 `reqwest` 发送，错误仅 log（`webhook/forwarder.rs`），无重试/退避（推断，未见重试逻辑）。
- `commands/mod.rs` 单文件 7444 行、`lib.rs` 5306 行、`hooks/server.rs` 3616 行——超大文件增加维护与 review 成本（结构性，非缺陷）。
