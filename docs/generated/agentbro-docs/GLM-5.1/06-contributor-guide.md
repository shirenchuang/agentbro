# 06 — 贡献者上手指南

> 评测模型：GLM-5.1 · 生成日期：2026-06-13
> 与 `.claude/CLAUDE.md`、`AGENTS.md`、`CONTRIBUTING.md` 一致，并补充实读代码后的精确路径。

---

## 6.1 如何启动项目

环境（`README.md:181-187`、`AGENTS.md:11-23`）：macOS + Node.js + pnpm + Rust toolchain + Tauri CLI。

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev      # 完整原生应用（先 build:bridge 再起 Vite + 原生窗口）
# 或仅调试浏览器 UI：
pnpm dev            # http://localhost:1423  /  #settings  /  #pet
```

`pnpm tauri:dev` 等价 `pnpm build:bridge && cargo tauri dev`（`package.json:11`）。`build:bridge` 编译 `agentbro-bridge` 二进制（`scripts/build-bridge.mjs`）并作为 Tauri resource 打包。

三个窗口（`App.tsx:60-90` 按 URL hash/窗口 label 区分）：
- `notch`（默认）：灵动岛 → `NotchPanel`
- `#settings`：设置面板 → `SettingsApp`
- `#pet`：宠物 → `PetApp`

浏览器开发模式内置 `ClaudeHookUiLab`（`src/components/dev/ClaudeHookUiLab.tsx`），可切换权限/计划/问题/完成/紧凑/列表/详情等静态场景。

---

## 6.2 如何运行测试与检查

提交/PR 前必绿（`README.md:246`、`AGENTS.md:25`）：

```bash
pnpm lint
pnpm test:run          # vitest 一次性
pnpm build             # tsc -b && vite build
cargo check --manifest-path src-tauri/Cargo.toml
```

或直接 `/check`（已封装）。Rust 测试/格式/lint：

```bash
cargo test  --manifest-path src-tauri/Cargo.toml
cargo fmt   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

---

## 6.3 如何理解主要模块（建议阅读顺序）

1. **入口与窗口路由**：`src/main.tsx` → `src/App.tsx`（三窗口分发）。
2. **后端骨架**：`src-tauri/src/lib.rs` 的 `run()` 与 `invoke_handler`（`:5058-5302`）→ 看清全部命令；`commands/mod.rs` 的 `AppState`（`:52-70`）→ 看清所有服务句柄。
3. **事件主链路**：`hooks/server.rs:handle_connection`（`:873`）→ `parse_with_adapters`（`:1487`）→ `process_event`（`:1509`）→ `session_store.rs:emit_update`（`:874`）。
4. **适配器扩展点**：`agents/traits.rs`（trait）+ `agents/mod.rs:all_adapters()`（注册）+ `agents/kimi.rs`（最简参考）+ `agents/claude_code.rs`（复杂参考）。
5. **前端主链路**：`services/tauriApi.ts` → `hooks/useTauri.ts:useSessionEvents`（`:491`）→ `stores/sessionStore.ts`（分层状态机）→ `components/notch/NotchPanel.tsx` + `components/overlay/*`。
6. **Hook 安装**：`agents/profiles.rs` + `hook_manager.rs` + `toml_hooks.rs`。

---

## 6.4 新增一个 Agent adapter，应该改哪些文件

> ⚠️ 注意 `.claude/CLAUDE.md` §3.1 写"7 个必需方法"，实际是 **8 个必需 + 建议覆盖 `detect_status_now`**（见风险 2）。

1. 新建 `src-tauri/src/agents/<name>.rs`，实现 `AgentAdapter` 的 8 个必需方法：`name`、`display_name`、`icon`、`install_hooks`、`remove_hooks`、`status`、`parse_event`、`hook_config_paths`，并覆盖 `detect_status_now`。参考 `kimi.rs`。
2. `src-tauri/src/agents/mod.rs`：顶部 `pub mod <name>;`；`all_adapters()`（`:214-239`）末尾加 `Box::new(<name>::<Name>Adapter::new())`；`impl_default_adapter!` 宏调用里加。
3. `src-tauri/src/agents/profiles.rs`：加 `fn <name>_profile() -> AgentIntegrationProfile`，在 `profile_for_agent()` match 注册；复用 `BASIC_AGENT_EVENTS`/`SESSION_TOOL_EVENTS`/`CODEBUDDY_EVENTS`/`KIMI_EVENTS` 而非重复定义。
4. **（建议）补 `agents/detection.rs:detect_installed_tools`**——否则不会被自动探测（见风险 4）。
5. 给 `parse_event` 写单元测试（模仿 `kimi.rs` 的 `mod tests`，见风险 12）。
6. 前端 icon：`src/components/notch/AgentIcon.tsx` 或对应映射 + `src/assets/cli-icons/<name>.png`；i18n 名称同步 `src/i18n/locales/{en,zh,ja,ko,tr}.json` 五份。
7. 跑 `/check`。也可 `/add-agent <name>` 引导。

---

## 6.5 新增一种 Overlay 卡片 / Hook event，应该改哪些文件

**新增 Hook event（后端→适配器→事件）**：
1. `src-tauri/src/agents/mod.rs`：在 `AgentEvent` 枚举加变体（注意 `server.rs` 多处 `match AgentEvent` 是穷尽的，需同步各分支）。
2. 对需要的适配器在 `parse_event` 产出该事件。
3. `src-tauri/src/hooks/server.rs:process_event`（`:1509`）加处理分支（更新 `SessionStore`、声音、webhook）。
4. 前端 `src/types/agent.ts` 同步 `AgentEvent`/`SessionState` 字段；`src/hooks/useTauri.ts` 的 `transformSession` 映射；`src/stores/sessionStore.ts:updateSession` 决定是否 `pushOverlay`。

**新增 Overlay 卡片**：
1. `src/components/overlay/` 新建 `<Name>Card.tsx` + `.css`（plain CSS + BEM，颜色走 `var(--*)`）。
2. `src/types/agent.ts` 的 `OverlayItem['type']` 加类型 + `OVERLAY_PRIORITY`（`types/agent.ts`）定优先级。
3. `src/stores/sessionStore.ts` 在 `updateSession`/`replaceAllSessions` 中 `pushOverlay`。
4. `src/components/notch/NotchPanel.tsx` 引入并按 `selectActiveOverlay` 渲染。
5. i18n 五语言同步。

---

## 6.6 新增一个 Skill 管理能力，应该改哪些文件

1. 后端：`src-tauri/src/skills/` 对应文件（scanner/installer/registry/marketplace/sync 之一）加函数 + 类型（`mod.rs`）。
2. `src-tauri/src/lib.rs`：「Skill Management Commands」区（约 `:2479` 起）加 `#[tauri::command]`，并在 `invoke_handler`（`:5203-5266`）注册。
3. 前端 service：`src/services/skillApi.ts` 的 `skillApi` 对象加方法（包 `invoke`）。
4. store：`src/stores/skillStore.ts` 加状态/action（`loadAll` 调用）。
5. UI：`src/components/skills/` 加组件；并在 `src/components/settings/sections/AgentsSection.tsx` 的视图切换（`:769-782`）接线——**注意别接到死代码 `SkillsSection.tsx`**（见风险 1/04 章）。
6. i18n 五语言同步；跑 `/check`。

---

## 6.7 常见开发注意事项

- **分支与提交**：PR 提到 `dev`（不是 `main`）；Conventional Commits（`feat/fix/docs/refactor/test/chore`）；不在 PR bump 版本号（maintainer 统一同步 `package.json`/`tauri.conf.json`/`Cargo.toml`/`Cargo.lock` 四处，`pnpm release:check` 校验）。
- **代码风格**：默认不写注释（只写 WHY）；bug fix 只改 bug，不顺手重构；前端 hooks 优先、无 `any`、复用现有 Zustand store；Rust 错误用 `Result<_, String>` 透传到 Tauri 边界，`unwrap()` 仅测试用；不引入新依赖前先在 Issue/PR 说明动机（Tauri crate 尤其要克制，影响包体积）。
- **样式**：plain `.css` + BEM；颜色走 CSS 变量（`var(--island-bg)` 等），不硬编码，否则主题失效；不引入 CSS Modules/Tailwind/styled-components。
- **i18n**：`src/i18n/locales/{en,zh,ja,ko,tr}.json` 五份同步加键，漏一种触发 fallback 体验差。
- **禁区**（`.claude/CLAUDE.md` §6）：品牌资产（`public/agentbro-*`、`docs/brand/`、`src-tauri/icons/`）、法律（`LICENSE`/`NOTICE`/`TRADEMARKS.md`）、签名（`Entitlements.plist`、`*.key/*.p12/*.pem/*.mobileprovision`）、发布配置（`.github/workflows/release.yml`、`homebrew/`、`docs/release.md`）、编译产物（`target/`、`dist/`、`output/`）。Fork 分发必须改名。
- **多窗口状态**：notch 与 settings 是两个独立窗口，配置经 `config-changed` 事件同步；改 `configStore` 时注意 `App.tsx` 的 `BACKEND_MANAGED_CONFIG_KEYS`（`:23-46`）——后端管理的字段不能被 localStorage 快照回放覆盖。
- **Hook 解析 bug**：先在对应 `<agent>.rs` 的 `mod tests` 加回归测试再改实现（CLAUDE.md §8）。
