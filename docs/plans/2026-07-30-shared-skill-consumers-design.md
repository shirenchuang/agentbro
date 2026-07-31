# Shared Skill Consumers Design

## Context

AgentBro inventories `~/.agents/skills` once under the internal `agents`
source. Several coding agents can load that shared directory in addition to
their own private Skills directory:

- Codex
- Kimi Code
- OpenClaw
- ZCode

The Agent detail UI currently treats those shared Skills inconsistently.
ZCode receives a dedicated read-only inherited scope, Codex receives the same
inventory through a frontend unmanaged-item exception, and Kimi Code and
OpenClaw do not receive the same presentation.

## Goals

- Represent `~/.agents/skills` as inherited capability for every supported
  consumer.
- Keep the shared `agents` inventory as the single source of truth.
- Keep inherited items visually and behaviorally separate from private,
  Agent-owned installations.
- Avoid showing the same shared item as both inherited and unmanaged.

## Non-goals

- Changing how Skills are installed or distributed.
- Creating one target or unmanaged row per consuming Agent.
- Supporting project-local `.agents/skills` roots in this change.
- Inferring shared-directory support from arbitrary folders at runtime.

## Options

### A. Continue frontend Agent-ID exceptions

The frontend can expand the existing `zcode` and `codex` checks to four Agent
IDs. This is small, but it duplicates capability knowledge across scanning,
filtering, counts, notices, and rendering. Future consumers can easily drift.

### B. Central backend consumer capability

The backend owns the supported consumer set and returns inherited Skills for
those Agents. The frontend renders the inherited scope whenever the returned
field is present for a consumer. Re-scan behavior uses the same consumer
capability instead of another independent list.

This is the selected approach because the backend already owns Agent path and
inventory semantics.

### C. Duplicate shared rows into each Agent inventory

Scanning could create unmanaged or target rows for every consumer. This makes
existing queries simpler, but duplicates ownership, hashes, diagnostics, and
cleanup actions. It also incorrectly suggests that a shared item belongs to
each Agent.

## Design

### Capability model

Add one backend predicate for Agents that inherit the global shared directory.
The initial consumer set is Codex, Kimi Code, OpenClaw, and ZCode.

The predicate is the only policy source used to:

- project shared inventory into `AgentDetail.inherited_skills`;
- expose whether the selected Agent consumes the shared directory;
- decide whether a detail re-scan must also refresh the internal `agents`
  inventory.

The existing `.agents` source remains hidden from the Agent sidebar.

### Backend projection

For a shared consumer, `AgentDetail` receives a read-only projection of skill
targets and unmanaged items owned by the internal `agents` source. The
projection preserves:

- the logical path under `~/.agents/skills`;
- the resolved path when the entry is a symlink;
- the inferred or managed Skill ID.

Rows are deduplicated by logical path and sorted by Skill ID. Non-consumers
receive an empty inherited collection.

Shared discovery also traverses bounded package-manager wrapper directories,
including `node_modules`, because installed Skill packages can place the real
`SKILL.md` below a top-level package shell. Canonically identical aliases are
deduplicated.

Agent-owned scans must not record the global shared root as that Agent's
unmanaged inventory. OpenClaw still scans its private workspace and built-in
roots, but the global `~/.agents/skills` root is represented only through the
shared projection.

### Frontend behavior

The Skills total, summary strip, overview capability count, and
`共享继承` scope use the inherited collection for every supported consumer.
The scope remains visible with a zero count so users can understand the
discovery rule before installing shared Skills.

The notice is Agent-aware:

> {{agent}} 默认读取 `~/.agents/skills`。这里展示该共享目录中当前可用的
> Skill；它们也可能被其他 Agent 使用。此处仅展示继承关系，不会将其视为
> {{agent}} 私有安装项。

Inherited cards and list rows remain read-only. They support search, paging,
card/list switching, and opening fallback details, but do not expose adopt,
delete, distribution, or pack mutation actions.

### Re-scan

Re-scanning a shared consumer refreshes both:

1. the selected Agent's private inventory;
2. the internal `agents` shared inventory.

The UI then reloads unmanaged inventory, Agent detail, and the overview.

### Compatibility and errors

`inheritedSkills` stays optional in the TypeScript transport type so older or
remote backends render an empty collection. A missing or unreadable shared
directory produces zero inherited items through the normal scan behavior.

## Tests

Rust regression tests cover:

- all four consumers receiving direct and symlinked shared Skills;
- non-consumers receiving no inherited Skills;
- no duplicate target or unmanaged rows for consumers;
- consumer capability metadata.

Frontend regression tests cover:

- Codex, Kimi Code, OpenClaw, and ZCode showing the inherited tab and dynamic
  notice;
- shared items not appearing under unmanaged;
- totals, summary, search, and view switching;
- re-scan refreshing both the selected Agent and `agents`;
- a non-consumer not showing the inherited scope.

All five locale files are updated together.
