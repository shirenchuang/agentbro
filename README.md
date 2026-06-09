<div align="center">
  <img src="public/agentbro-logo.png" alt="AgentBro Logo" width="148" />

  <h1>AgentBro</h1>

  <p><strong>让 Agent 更好用</strong></p>

  <p>
    面向 AI 编程 Agent 的 macOS 灵动岛。<br />
    把权限请求、问题、计划、工具调用、快速回复、远程会话和完成提醒，收进一个轻巧的桌面浮窗。
  </p>

  <p>
    <a href="https://www.agentbro.net">官网</a>
    ·
    <a href="https://github.com/shirenchuang/agentbro/releases">下载</a>
    ·
    <a href="docs/privacy-policy.md">隐私</a>
    ·
    <a href="README.en.md">English</a>
  </p>

  <p>
    <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-111820" />
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS-f5b84b" />
    <img alt="Built with Tauri" src="https://img.shields.io/badge/Tauri-React%20%2B%20Rust-0c6b63" />
  </p>
</div>

## AgentBro 是什么？

AgentBro 是一个悬浮在编辑器和终端上方的原生 macOS 应用。它会实时观察 Claude Code、Codex、Gemini CLI 等 AI 编程 Agent 的会话状态，把最容易打断心流的事情集中到一个灵动岛里处理。你可以直接在弹窗里批准权限、回答问题、回复消息，也可以把 SSH 远程机器上的 Agent 会话转发回本机查看。

第一个开源版本聚焦在 **灵动岛模块**。Agent Monitor、Agent Switch、Skills 管理等更大的模块暂时不会出现在公开版菜单里，后续会结合实际使用和社区反馈逐步开放。

## Logo 含义

AgentBro 的 Logo 中间是一个握手造型，代表人和 AI Agent 之间的协作关系：不是替代，也不是遥控，而是像 bro 一样在旁边接力、提醒、兜底。外层的 `A` / `B` 结构来自 AgentBro 的首字母，也像两个 Agent 节点连接在一起。

## 演示视频

### 交互演示

https://github.com/user-attachments/assets/df857822-ea0a-4745-a0b9-80f265f30dc6

### 多主题演示

https://github.com/user-attachments/assets/374d6e53-c126-41be-a593-4e5f63485602

## 支持的主题

| 主题 | ID | 风格 |
| --- | --- | --- |
| 午夜 | `midnight` | 默认深色主题，适合长时间编码和夜间使用。 |
| AgentBro 经典 | `ink-amber` | 品牌经典暖色主题，强调墨色与琥珀色对比。 |
| 磨砂玻璃 | `frosted-glass` | 轻量浅色玻璃质感，适合明亮桌面环境。 |
| 苹果 | `apple` | 干净的 macOS 系统风格，低干扰、偏原生。 |
| 烟灰 | `smoke` | 中性浅色主题，降低色彩刺激，适合持续监控。 |
| 海雾 | `ocean-mist` | 冷色浅色主题，以蓝色强调状态与操作。 |
| 暖纸 | `warm-paper` | 温暖纸感主题，适合偏柔和的桌面搭配。 |
| 柔薰衣草 | `soft-lavender` | 柔和紫色主题，偏轻盈、低对比。 |
| 跟随系统 | `system` | 跟随系统浅色 / 深色外观自动切换。 |

## 主要功能

| 功能 | 说明 |
| --- | --- |
| 灵动岛浮窗 | 支持紧凑、悬停、展开和详情视图，随时查看 Agent 状态。 |
| 即时处理 | 在浮窗中处理权限请求、问题、计划审批、完成提醒和回复卡片。 |
| 快速回复 | 不切回终端，也可以直接在弹窗里输入消息，继续和 Agent 对话。 |
| 任务感知 | 展示工具调用、Subagent 活动、任务摘要，以及支持场景下的 Token / Rate Limit 信息。 |
| 宠物模式 | 把灵动岛切换成宠物状态面板，宠物活力随上下文压力和 Token 用量变化。 |
| 宠物市场 | 浏览社区宠物、一键安装，由 abpets CLI 驱动。详见 [www.agentbro.net/pets](https://www.agentbro.net/pets)。 |
| Hook 集成 | 一键安装 Hook，内置 Hook Doctor 诊断，支持自定义 CLI Hook 模板。 |
| 桌面体验 | 支持全局快捷键、声音、通知、主题、显示器位置和终端焦点智能降噪。 |
| 本地优先 | Hook Server 默认运行在本机，支持 `/tmp/agentbro-<uid>.sock` 或 `127.0.0.1:17894`。 |
| SSH Remote | 支持把远程 SSH 机器上的 Agent 事件转发回本机灵动岛，适合远程开发场景。 |
| Webhook 通知 | 支持钉钉 / 飞书 Webhook 通知。 |

## 宠物市场

除了灵动岛,AgentBro 还可以把浮窗切换成 **宠物状态面板**:一只桌面宠物会跟随当前活跃的 Agent,它的活力会随上下文压力和 Token 用量实时变化,让你一眼看出会话是轻松还是吃紧。

**宠物市场** 让你浏览社区贡献的宠物并一键安装,整个流程由 [`abpets`](https://www.npmjs.com/package/abpets) CLI 驱动(基于 Node.js v18+)。在设置面板的 **Island -> 宠物市场** 即可打开,也可以在网页上预览全部宠物:

👉 **[www.agentbro.net/pets](https://www.agentbro.net/pets)**

想自己创作宠物，可以使用 [`shirenchuang/agentbro-pet`](https://github.com/shirenchuang/agentbro-pet) Skill：它会把角色概念、品牌线索或参考图生成 AgentBro 可用的 `pet.json` + `spritesheet.webp` 宠物包，并支持接入不同的生图后端。可通过 `npx skills add https://github.com/shirenchuang/agentbro-pet.git` 安装，也可以直接克隆；Codex、Claude Code、Cursor、Gemini CLI 等任意能运行脚本和生成图片的 Agent 都可以使用。

<img src="https://github.com/user-attachments/assets/53a17db6-54c4-40f1-95b6-89a7f1977f00" alt="AgentBro 宠物模式" width="100%" />

<img src="https://github.com/user-attachments/assets/efd1acc8-67bb-460f-b7c9-3faa490611f5" alt="AgentBro 宠物市场" width="100%" />

## 支持的 Agent

AgentBro 内置了以下 Agent 的适配器和 Hook 管理能力：

| Agent | 支持状态 |
| --- | --- |
| Claude Code | 完整接入 |
| Codex | 完整接入 |
| Gemini CLI | 完整接入 |
| Cursor / Cursor CLI | 完整接入 |
| GitHub Copilot | 完整接入 |
| Trae / Trae CN | 完整接入 |
| Qoder / Qoder CLI | 支持 |
| CodeBuddy / CodeBuddy CN | 支持 |
| Qwen | 支持 |
| Kimi | 支持 |
| OpenCode | 支持 |
| Droid | 支持 |
| Factory | 支持 |
| StepFun | 支持 |
| AntiGravity | 支持 |
| WorkBuddy | 支持 |
| Hermes | 支持 |
| Pi | 支持 |
| Kiro | 支持 |

## 路线图

AgentBro 会坚持本地优先：第一个公开版本先把灵动岛、Hook 集成、快速处理和 SSH Remote 做扎实。后续希望继续探索：

- 远程同步：跨设备同步设置、Hook、主题、Prompt、Skills 和远程主机配置。
- 技能社区：发现、安装、分享和更新面向不同 Agent 的 Skill Pack。
- 宠物生态：上架更多社区宠物、丰富宠物市场,并开放自定义宠物的创作与分享流程。
- 团队协作：共享配置、团队 Skill 包、权限控制和更清晰的协作视图。

## 加入交流群

如果你正在使用 AgentBro，或者想参与后续 Windows、Agent Monitor、Agent Switch、Skills 等模块讨论，可以扫码添加微信，备注 **AgentBro 交流群**，或直接扫码加入 **AgentBro 开源社区** 群聊。

<div align="center">
  <table>
    <tr>
      <td align="center">
        <img src="public/agentbro-wechat-qr.jpg" alt="AgentBro 交流群微信二维码" width="260" /><br />
        <sub>添加微信备注 <b>AgentBro 交流群</b></sub>
      </td>
      <td align="center">
        <img src="public/agentbro-group-qr.png" alt="AgentBro 开源社区微信群二维码" width="260" /><br />
        <sub>群聊：<b>AgentBro 开源社区</b>（二维码 7 天有效，过期后请联系微信邀请）</sub>
      </td>
    </tr>
  </table>
</div>

## 平台支持

AgentBro 当前优先开发和测试 **macOS** 版本，并提供早期 **Windows MVP** 安装包。

Windows 版本已经具备基础悬浮窗口、Hook 传输、路径探测和安装包构建能力，但仍属于早期支持；签名、SmartScreen 体验、自动更新和部分 Agent 深度集成还会继续完善。

Linux 后续也可以支持，但不属于第一个公开版本的目标。

## 安装

### Homebrew Cask

一行安装：

```bash
brew tap shirenchuang/tap && brew install --cask agentbro
```

分步安装：

```bash
brew tap shirenchuang/tap
brew install --cask agentbro
```

### 下载发行版

- 🇨🇳 macOS 国内直链（推荐，速度快）：[下载最新 DMG](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)
- Windows 安装包：在 [GitHub Releases](https://github.com/shirenchuang/agentbro/releases) 下载 `AgentBro_latest_x64-setup.exe` 或 `AgentBro_latest_x64.msi`
- 🌍 海外 / 全部版本：[GitHub Releases](https://github.com/shirenchuang/agentbro/releases)

## 本地开发

### 环境要求

- macOS
- Node.js
- pnpm
- Rust toolchain + Cargo
- Tauri CLI：`cargo tauri --version`

### 启动项目

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev
```

`pnpm tauri:dev` 会启动 `http://localhost:1423` 上的 Vite 开发服务，并打开 AgentBro 原生窗口。

### 只调试浏览器 UI

```bash
pnpm dev
```

打开：

- 灵动岛 UI：`http://localhost:1423`
- 设置面板：`http://localhost:1423/#settings`

浏览器开发模式内置了 Claude Hook UI Lab，可以切换权限请求、计划审批、问题、完成提醒、紧凑模式、列表模式和详情模式等静态场景。

### 常用命令

```bash
pnpm test:run      # 运行测试
pnpm test          # 监听模式运行测试
pnpm lint          # ESLint
pnpm build         # 类型检查并构建前端
cargo check        # 检查 Rust 后端
pnpm tauri:build   # 构建 Tauri 应用
pnpm tauri:build:windows # 构建 Windows NSIS/MSI 安装包
./build.sh         # 构建通用 macOS DMG
```

## 接入 Agent

1. 打开 AgentBro 设置。
2. 进入 **Island -> Integration**。
3. 运行 **Hook Doctor**。
4. 点击 **Install All Hooks**，或只安装你正在使用的 Agent Hook。
5. 重启对应的 CLI 会话。
6. 启动 Claude Code、Codex、Gemini CLI 或其他支持的 Agent。

之后 AgentBro 会在灵动岛中展示会话状态、工具调用、权限请求、问题、计划和完成提醒。

## 参与贡献

欢迎提交 Issue 和 Pull Request！

- 贡献指南：[CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则：[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- AI Agent 协作指南：[AGENTS.md](AGENTS.md)
- Claude Code 项目配置：[.claude/CLAUDE.md](.claude/CLAUDE.md)

PR 请提到 `dev` 分支，提交前跑一遍 `pnpm lint && pnpm test:run && pnpm build && cargo check --manifest-path src-tauri/Cargo.toml`。

## 发布

发布说明和签名要求见 [`docs/release.md`](docs/release.md)。

- 官网：[www.agentbro.net](https://www.agentbro.net)
- 国内直链：`https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`
- GitHub Releases：`https://github.com/shirenchuang/agentbro/releases`

## 开源协议

AgentBro 代码基于 [Apache License 2.0](LICENSE) 开源。

AgentBro 名称、Logo、应用图标、官网视觉和其他品牌资产不随代码授权开放。修改版或分发版请使用不同名称，避免和官方项目产生混淆，并遵守 [NOTICE](NOTICE) 和 [TRADEMARKS.md](TRADEMARKS.md)。
