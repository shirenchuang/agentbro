## Evobase 知识检索
当进行知识库内容搜索时，若以下导航文件存在，**必须先读取**，了解知识库目录结构与检索优先级，再执行检索：
- `/Users/mac/code/szzgithub/agentbro/.evobase/.repos.json` 中 `repos` 字段的每个 key 即为 repo 名称，对应导航文件路径为 `/Users/mac/code/szzgithub/agentbro/.evobase/<repo-name>/AGENTS.md`

知识库内容检索时，使用 knowledge-search skill.

在对.evobase下的知识库进行拉取、推送等操作时，优先使用evobase的api，除非出错否则禁止使用原生git命令操作。

<!-- AUTO-KB-LIST 生成于 2026-06-16 17:15，请勿手工编辑此标记块 -->
## 本地已同步知识库

> 当前项目尚未拉取任何知识库。运行 `/kpull` 或 `/knowledge-init` 添加。
<!-- /AUTO-KB-LIST -->
