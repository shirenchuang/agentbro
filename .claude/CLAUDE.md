# AgentBro — Claude Code 项目级指令

> 这份文件会被 Claude Code 在每次会话开始时自动加载。**请先读一遍再开始改代码。**
> 通用的 AI Agent 约定见根目录 [`AGENTS.md`](../AGENTS.md);
> 贡献流程、PR 规范见 [`CONTRIBUTING.md`](../CONTRIBUTING.md)。

---

## 1. 项目画像

AgentBro 是一个 **macOS 桌面悬浮窗(灵动岛)**,用来收纳 Claude Code / Codex / Gemini CLI / Cursor / Copilot 等 AI 编程 Agent 的运行状态。技术栈:

- **前端**:React 19 + TypeScript + Vite + Zustand + i18next + framer-motion
- **后端**:Rust + Tauri 2(macOS only,Windows 在路线图)
- **构建产物**:macOS 通用二进制 `.dmg`(arm64 + x86_64)
- **本地 Hook Server**:`/tmp/agentbro.sock` 或 `127.0.0.1:17892`

第一性原则是 **本地优先 + 低打扰**。所有改动都要服务于这两个目标。

---

## 2. 目录速查表

| 路径 | 作用 |
| --- | --- |
| `src/` | 前端 React 应用(灵动岛 UI、设置面板) |
| `src/components/notch/` | 灵动岛主界面组件(折叠条、悬停列表、聊天面板、权限审批…) |
| `src/components/settings/` | 设置面板各 Section |
| `src/components/shared/` | 跨模块复用组件 |
| `src/stores/` | Zustand 状态:`sessionStore`、`configStore`、`agentStore`、`themeStore`、`petStore` |
| `src/services/tauriApi.ts` | 前端调用 Tauri 命令的统一入口 |
| `src/hooks/useTauri.ts` | 监听后端 IPC 事件的 React Hook 集合 |
| `src/i18n/locales/{en,zh,ja,ko,tr}.json` | 五语言资源,必须同步更新 |
| `src/themes/` | 主题定义(midnight、ink-amber、apple…) |
| `src-tauri/` | Rust 后端 |
| `src-tauri/src/agents/` | **每个支持的 AI Agent 一个 `.rs` 文件**(扩展点 #1) |
| `src-tauri/src/agents/traits.rs` | `AgentAdapter` trait 定义 |
| `src-tauri/src/agents/mod.rs` | Agent 注册中心(见第 214 行 `all_adapters()`) |
| `src-tauri/src/agents/profiles.rs` | Hook 安装配置(`AgentIntegrationProfile` + 事件描述符) |
| `src-tauri/src/agents/hook_manager.rs` | JSON/YAML/TOML Hook 注入与卸载 |
| `src-tauri/src/commands/` | Tauri IPC 命令(`#[tauri::command]`) |
| `src-tauri/src/lib.rs` | 应用入口 + 命令注册表(`invoke_handler`) |
| `src-tauri/src/hooks/` | Hook Server 与恢复逻辑 |
| `src-tauri/src/remote/` | SSH Remote 转发 |
| `src-tauri/tauri.conf.json` | Tauri 打包/权限配置 |
| `docs/release.md` | 发布与签名流程(maintainer 用) |

---

## 3. 核心扩展点

### 3.1 新增 / 修改 Agent 适配器

参考最简实现:`src-tauri/src/agents/kimi.rs`;复杂实现:`src-tauri/src/agents/claude_code.rs`。

步骤:
1. 在 `src-tauri/src/agents/` 新建 `<agent_name>.rs`,实现 `AgentAdapter` trait 的 7 个必需方法。
2. `src-tauri/src/agents/mod.rs`:
   - 文件顶部加 `pub mod <agent_name>;`
   - 在 `all_adapters()`(~第 214 行)末尾加 `Box::new(<agent_name>::<Name>Adapter::new())`
   - 在 `impl_default_adapter!` 宏调用里加上 `<agent_name>::<Name>Adapter`
3. 如果该 Agent 走标准 Hook 安装流程,在 `src-tauri/src/agents/profiles.rs` 加一个 `fn <name>_profile() -> AgentIntegrationProfile`,并在 `profile_for_agent()` match 里注册。
4. 给 `parse_event()` 写单元测试,模仿 `kimi.rs` 第 231 行起的 `mod tests`。
5. 前端 icon:在 `src/components/notch/AgentIcon.tsx` 或对应映射里加一行;i18n 名称同步五份 `locales/*.json`。
6. 跑 `/check` 验证。

也可以直接用 slash 命令 `/add-agent <name>` 引导。

### 3.2 新增 Tauri 命令(后端→前端)

1. 在 `src-tauri/src/commands/` 对应子模块或新文件里加 `#[tauri::command] pub async fn ...`。
2. 在 `src-tauri/src/lib.rs` 的 `invoke_handler(tauri::generate_handler![...])` 里追加函数名。
3. 前端在 `src/services/tauriApi.ts` 写一层薄封装(类型签名 + 错误处理)。
4. 需要异步事件推送的话,在 `src/hooks/useTauri.ts` 里加一个 `useXxxEvents` Hook,模式参考第 436 行的 `useSessionEvents`。

错误统一用 `Result<T, String>` 跨 Tauri 边界(序列化代价小)。

### 3.3 新增前端组件

- 放在 `src/components/{notch|settings|shared|overlay}/` 对应域。
- 样式:**plain `.css`** + 同名文件 + BEM 命名(如 `.approval-bar__warning`)。**不要引入 CSS Modules、Tailwind 或 styled-components**。
- 颜色全部走 CSS 变量(`var(--island-bg)`、`var(--text-primary)`…),不要硬编码,否则主题会失效。
- 状态优先用现有 Zustand store。新 store 也放 `src/stores/` 并走 `create<State & Actions>()` 的模式。

### 3.4 新增主题

`src/themes/` 加主题定义,同时更新 `README.md` 与 `README.en.md` 的主题表、`src/i18n/locales/*.json` 的主题名翻译。

### 3.5 新增国际化字符串

`src/i18n/locales/{en,zh,ja,ko,tr}.json` **同步加键**。漏一种语言会触发兜底 fallback,体验差。

---

## 4. 必跑的本地检查

在提交 / 提 PR **之前**,这四个命令必须全绿:

```bash
pnpm lint
pnpm test:run
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

也可以直接 `/check`(已封装上面这条流水线)。

Rust 单元测试:`cargo test --manifest-path src-tauri/Cargo.toml`。
Rust 格式与 lint:`cargo fmt --manifest-path src-tauri/Cargo.toml`、`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`。

启动调试:
- 完整 Tauri 应用:`pnpm tauri:dev`(会先 `cargo build` bridge 二进制,再起 Vite + 原生窗口)
- 只跑浏览器 UI:`pnpm dev` → http://localhost:1423 / `#settings`

---

## 5. 提交与分支

- **所有会修改仓库文件的任务必须先走 Issue-first 流程。** Bug、需求、重构、测试、文档和配置修改都包含在内；纯问答、只读分析和 code review 不需要，除非后续转为实际修改。
- 写文件前先搜索是否有同范围 Issue；没有就创建包含背景、目标、验收标准和约束的 Issue。Issue 编号确定之前不得开始修改。
- 从最新 `origin/dev` 创建独立分支，命名为 `claude/issue-<编号>-<slug>`。如果当前分支属于其他任务，保留现场并使用独立 git worktree。
- **PR 提到 `dev` 分支**,不是 `main`。`dev` 是集成分支,`main` 是 release 分支。
- 提交信息走 [Conventional Commits](https://www.conventionalcommits.org/):`feat:`、`fix:`、`docs:`、`refactor:`、`test:`、`chore:`。
- 完成本地检查后提交并推送；PR 正文必须写 `Closes #<Issue 编号>`。
- 创建 PR 后执行 `gh pr merge --auto --squash --delete-branch`，持续跟进 CI；失败就修复同一分支，禁止绕过检查或合并红色 PR。只有 PR 已合并，或已明确报告外部阻塞，任务才算结束。
- 不要为了增加活跃度创建重复、空白或与改动无关的 Issue；每个 Issue 和 PR 都必须对应真实、可审查的工作。
- 不要在 PR 里 bump 版本号。版本号统一由 maintainer 在出 release 时更新,**且要四处同步**:
  - `package.json` 的 `version`
  - `src-tauri/tauri.conf.json` 的 `version`
  - `src-tauri/Cargo.toml` 的 `[package].version`
  - `src-tauri/Cargo.lock` 里 `agentbro` 条目
  - `pnpm release:check` 会校验这四处一致。
- **不要** 跳 hooks(`--no-verify`)、不要 force-push 别人的分支、不要改 `.github/workflows/release.yml` 除非你跟 maintainer 对齐过。

---

## 6. 禁区(不要动)

以下文件 / 目录 **不要修改、复制或重新生成**,除非用户明确要求:

- 品牌资产:`public/agentbro-*.png`、`public/agentbro-*.jpg`、`docs/brand/`、`src-tauri/icons/`
- 法律文本:`LICENSE`、`NOTICE`、`TRADEMARKS.md`
- 签名相关:`src-tauri/Entitlements.plist`、所有 `*.key` / `*.p12` / `*.pem` / `*.mobileprovision`
- Maintainer 发布配置:`.github/workflows/release.yml`、`.github/release-notes.md`、`homebrew/`、`docs/release.md`
- 已编译产物:`src-tauri/target/`、`dist/`、`output/`

如果你 Fork 后准备分发,**必须改名**,见 [`TRADEMARKS.md`](../TRADEMARKS.md)。Agent 名字保留 "AgentBro" 字样的修改版会引起混淆。

---

## 7. 代码风格约定

- **默认不写注释。** 只在 WHY 不明显(隐藏约束、历史 bug、反直觉的实现)时写一两行。不要解释 WHAT 已被命名表达的事。
- 不做未要求的 refactor。bug fix 就只改 bug,不要顺手"清理"周边代码。
- 不引入新依赖前先在 Issue 或 PR 描述里说明动机。Tauri 端的新 crate 尤其要克制(影响包体积和构建时间)。
- 前端:hooks 优先,避免 class 组件;props 类型显式声明;不要 `any`。
- Rust:错误用 `Result<_, String>` 透传到 Tauri 边界;长函数拆成小函数;`unwrap()` 仅限测试。
- 不要在 PR 里掺杂无关的格式化变更(导致 diff 变大、review 困难)。

---

## 8. 用 AI 协作时的小贴士

- 这份 CLAUDE.md 已经把扩展点位置都标好了,**不要 grep 一通乱猜**,先按这里给的文件路径直接读。
- 涉及多个 Agent 的改动,优先复用 `profiles.rs` 已有的事件描述符(`BASIC_AGENT_EVENTS`、`SESSION_TOOL_EVENTS`、`CODEBUDDY_EVENTS`、`KIMI_EVENTS`),不要重复定义。
- 修复 Hook 解析 bug 时,先在对应 `<agent>.rs` 的 `mod tests` 加一个回归测试,再改实现。
- 改完跑 `/check`,不要把锅推给 CI。
