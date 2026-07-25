# 贡献 AgentBro

感谢你愿意为 AgentBro 出一份力 🙌 不论是修 Bug、提建议、新增 Agent 适配,还是补一份翻译,我们都很欢迎。

> 本文档面向**所有贡献者**(包括用 AI 协作的)。AI Agent 还请先读 [`AGENTS.md`](AGENTS.md) 或 [`.claude/CLAUDE.md`](.claude/CLAUDE.md) 拿一份项目地图。
> English version: [`CONTRIBUTING.en.md`](CONTRIBUTING.en.md)。

---

## 目录

1. [行为准则](#行为准则)
2. [提 Issue 之前](#提-issue-之前)
3. [本地开发](#本地开发)
4. [分支与提交规范](#分支与提交规范)
5. [PR 流程](#pr-流程)
6. [常见贡献场景](#常见贡献场景)
7. [AI 协作贡献指引](#ai-协作贡献指引)
8. [品牌与商标](#品牌与商标)
9. [社区](#社区)

---

## 行为准则

请遵守 [Code of Conduct](CODE_OF_CONDUCT.md)(Contributor Covenant v2.1)。简单说一句:**对人保持基本尊重,对事保持技术诚实**。

---

## 提 Issue 之前

1. **先搜一下**:[Issues 列表](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue) 看是不是已经有人报过。
2. **确认版本**:用最新的 Release 重现。 老版本的 bug 大概率已经修了。
3. **跑过 Hook Doctor**:设置 → Island → Integration → Run Hook Doctor。很多 Hook / 权限问题这里就能定位。
4. **走对模板**:Bug 用 Bug 模板,新功能用 Feature 模板,新 Agent 适配用 Agent 请求模板。空白 Issue 已禁用。

---

## 本地开发

### 环境

- macOS(目前只支持 macOS)
- Node.js 20+
- pnpm 9+
- Rust 稳定版 + Cargo
- Tauri CLI:`cargo install tauri-cli` 或 `pnpm dlx @tauri-apps/cli`

### 启动

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev   # 完整原生应用(推荐)
# 或
pnpm dev         # 仅浏览器 UI,适合调样式 → http://localhost:1423
```

`pnpm tauri:dev` 会先 `cargo build` 一份 `agentbro-bridge` 二进制,再启 Vite + 原生窗口。第一次构建比较慢,Rust 依赖编译可能要 5-10 分钟,后续增量很快。

### 提交前必跑

```bash
pnpm lint
pnpm test:run
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

Claude Code 用户可以直接 `/check` 一条龙。

---

## 分支与提交规范

### 分支

- `main` —— release 分支,只接收 maintainer 发版合并
- `dev` —— **集成分支,所有 PR 都提到这里**
- 你的功能分支可以叫 `feat/<short-name>` / `fix/<short-name>` / `docs/<short-name>`

### 提交信息

走 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/):

```
feat: 新增对 Foo Agent 的 Hook 适配
fix(notch): 紧凑模式下右侧 icon 溢出
docs: 修正 README 主题表
refactor(agents): 抽出共享的 hook 事件解析
test: 给 codex parse_event 加边界用例
chore: 升级 vitest 到 4.2
```

一次 commit 只做一件事;PR 里如果出现"顺手清理了 xxx",reviewer 会把它请回去拆 PR。

### 版本号

**不要在 PR 里改版本号。** 版本号由 maintainer 在发版时统一更新,且必须在四个文件里同步:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`

`pnpm release:check` 会校验一致性,不一致会让 CI 红。

---

## PR 流程

1. 先创建或认领一个与改动范围一致的 Issue。
2. Fork → 在 `dev` 上拉分支 → 改动 → 跑完上面四条检查。
3. PR base 选 `dev`,标题写清楚改了什么(中英都可)。
4. 按 PR 模板填写内容，并在正文保留 `Closes #<Issue 编号>`；`PR policy / Issue link` 会校验它。
5. CI 会跑 `ci.yml`(lint + test + cargo check/clippy/fmt + cargo test)和 `build.yml`(macOS 双架构构建)。两个都得绿。
6. Review 节奏:工作日基本能在 48 小时内给到第一轮反馈。如果一周没人理,可以在 PR 里 @maintainer 提醒一下。
7. Merge 用 squash；仓库成员可预先开启 auto-merge，所有必需检查通过后会自动合并并删除任务分支。

---

## 常见贡献场景

### 修 Bug

- 先在 Issue 里复现 → 在对应模块的测试目录(`src/test/` 或 Rust 文件内 `#[cfg(test)] mod tests`)写一个能稳定复现的失败用例 → 改实现让它通过。

### 新增 Agent 适配

参考 [`.claude/commands/add-agent.md`](.claude/commands/add-agent.md) 里的步骤(就算你不用 Claude Code,这份步骤也是人类可读的)。简版:

1. `src-tauri/src/agents/<name>.rs`:实现 `AgentAdapter` trait(参考 `kimi.rs`)。
2. `src-tauri/src/agents/mod.rs`:在 `all_adapters()` 和 `impl_default_adapter!` 里注册。
3. `src-tauri/src/agents/profiles.rs`:加 `<name>_profile()` 并在 `profile_for_agent()` 里登记。
4. 给 `parse_event` 写测试。
5. `src/components/notch/AgentIcon.tsx` 加 icon 映射 / `src/i18n/locales/*.json` 加展示名 / README 表加一行。

### 新增主题

`src/themes/` 新增主题文件 → README 主题表加一行(中英)→ `src/i18n/locales/*.json` 加翻译。

### 新增翻译

`src/i18n/locales/{en,zh,ja,ko,tr}.json` **五份必须同步加键**。漏一种语言会触发兜底,体验不一致。

### 写文档

直接改 `README.md` / `README.en.md` / `docs/*.md`。不要新增独立 README;改进现有的更受欢迎。

---

## AI 协作贡献指引

我们鼓励用 AI 协作 —— AgentBro 自己就是为 AI Agent 服务的工具,理念一致。

- **善用项目级配置**:Claude Code 用户进仓库就能加载 [`.claude/CLAUDE.md`](.claude/CLAUDE.md);Codex / Cursor / Aider / Copilot / Gemini CLI 等读 [`AGENTS.md`](AGENTS.md)。先让 Agent 读这两份再开始改代码,能省大量瞎猜成本。
- **代码改动先建 Issue**:Agent 必须在写文件前创建或关联 Issue,使用独立任务分支,并在 PR 中用 `Closes #<编号>` 建立闭环。纯问答和只读分析不需要制造 Issue。
- **PR 描述请如实标注**:如果整份 PR 主要由 AI 生成,在描述里加一句"Co-authored with <Agent 名>"或类似措辞。我们不歧视 AI,但要求诚实。
- **AI 生成的代码也得自测**:`pnpm lint`、`pnpm test:run`、`cargo check` 全绿再提。AI 跳过测试 / 没看清错误 / 改完不验证的 PR 会被退回。
- **不要让 AI 改品牌资产、签名配置、发布流程**。这些有商标和安全含义,需要人工判断。具体见 [禁区列表](.claude/CLAUDE.md#6-禁区不要动)。
- **Agent 决策权**:涉及架构变更、新依赖、改动跨 5+ 文件时,人类先在 Issue 或 PR 描述里和 maintainer 对齐方案,再让 AI 实施;不要直接让 AI 大改后丢一个 200 行 PR 过来。

---

## 品牌与商标

AgentBro **代码** 走 [Apache License 2.0](LICENSE)。但是 **名称 "AgentBro"、Logo、应用图标、官网视觉** 不在代码授权范围内。

如果你 Fork 后:
- 自用、改着玩、提 PR 给上游 —— 没问题。
- 准备分发自己的版本 —— **必须改名**,不要让用户以为你的版本是官方 AgentBro。

详见 [`TRADEMARKS.md`](TRADEMARKS.md) 和 [`NOTICE`](NOTICE)。

---

## 社区

- 微信群:扫 [README](README.md#加入交流群) 里的二维码,备注 **AgentBro 交流群**。
- Releases:https://github.com/shirenchuang/agentbro/releases
- 官网:https://www.agentbro.net

有想做但不确定要不要做的 idea,可以先在 Issue 里开一个 Discussion 性质的帖子,聊清楚再动工 —— 比写完 PR 被请回去返工友好很多。
