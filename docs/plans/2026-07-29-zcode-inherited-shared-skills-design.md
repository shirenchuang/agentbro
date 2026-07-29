# ZCode Inherited Shared Skills

Issue: <https://github.com/shirenchuang/agentbro/issues/76>

## Context

ZCode loads user skills from its private `~/.zcode/skills` directory and from
the shared `~/.agents/skills` directory. AgentBro currently shows only
ZCode-private targets in the ZCode Agent Management view. On a machine where
ZCode successfully lists dozens of shared skills, AgentBro can therefore report
`Skills (0)`.

AgentBro already inventories `~/.agents/skills` as a shared source. Those items
must remain owned by the shared inventory: showing them in ZCode must not turn
them into ZCode-private targets or expose duplicate adoption, deletion, or skill
pack actions.

## User Experience

The existing `Agent 管理 → ZCode → Skills` view gains a fourth scope beside
`已管理`, `未管理`, and `内置`:

`共享继承 <count>`

The Skills tab total and the ZCode summary strip include inherited skills. The
scope opens as a read-only collection using the existing card/list view switch
and search field.

The top of the inherited scope displays:

> ZCode 默认读取 `~/.agents/skills`。这里展示该共享目录中当前可用的 Skill；
> 它们可能同时被 Codex、Kimi 等 Agent 使用。此处仅展示继承关系，不会将其视为
> ZCode 私有安装项。

Each item is labeled `.agents 共享目录` and shows its logical shared path. A
symlinked skill may also expose its resolved source path in the detail view.
There are no select, adopt, delete, move-to-pack, or distribution controls in
this scope.

Plugin-provided skills remain represented by the existing Plugins surface and
are not mixed into `共享继承`.

## Data Flow

The backend continues scanning `~/.agents/skills` once as the dedicated shared
inventory. ZCode Agent detail derives a read-only inherited-skills projection
from those existing shared inventory rows.

The projection contains only display data:

- stable item ID
- skill name
- logical path under `~/.agents/skills`
- resolved source path when the logical path is a symlink

It does not create `skill_targets`, claims, pack membership, or a second
unmanaged record for ZCode.

The frontend renders the projection in its own `inherited` scope. It never
passes inherited items to existing mutation handlers.

## Semantics

| Scope | Meaning | Mutating actions |
| --- | --- | --- |
| 已管理 | AgentBro distributed the skill to `~/.zcode/skills` | Existing controls |
| 未管理 | A private ZCode skill exists outside AgentBro management | Adopt/delete |
| 共享继承 | ZCode can read the shared `~/.agents/skills` item | None |
| 内置 | Agent/runtime-provided read-only skill | None |

This preserves the distinction between availability and ownership. The ZCode
count reflects effective user skills without pretending AgentBro privately
installed them.

## Error Handling

- A missing or empty `~/.agents/skills` directory yields an empty inherited
  scope and no warning.
- Broken links are excluded by the existing shared inventory scan and remain
  diagnosable through the shared inventory/diagnosis surfaces.
- Duplicate skill names follow the shared inventory result; the ZCode view does
  not introduce a second deduplication or precedence system.
- Failure to resolve a symlink keeps the logical path available for display.

## Testing

Backend tests cover:

- ZCode Agent detail includes shared inventory items as inherited skills.
- Inherited items retain the logical `.agents` path and resolved source path.
- No ZCode target or duplicate unmanaged record is created.
- Other Agent details do not gain ZCode-only inherited projections.

Frontend tests cover:

- ZCode totals include inherited skills.
- `共享继承` shows the count, explanation, source label, and path.
- Inherited cards expose no mutating actions.
- Search, card/list mode, empty state, and tab switching remain functional.
- Existing managed, unmanaged, and built-in behavior remains unchanged.
