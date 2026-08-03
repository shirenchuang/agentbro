# Shared Agent Skill Control Reuse Design

## Context

Issue #92 corrects an incomplete reuse introduced by the previous shared-Skill
UI work. Agent-specific managed and unmanaged Skills use
`ManagedSkillCollection` and `UnmanagedSkillCollection`, while inherited
Skills still pass through `InheritedSkillCollection`. The inherited collection
renders the same low-level card shell but owns a different set of actions and
states. As a result, shared managed Skills have no delete action and shared
unmanaged Skills have a one-off adoption action instead of the standard
management controls.

Shared inheritance is a data-source dimension, not a separate kind of Skill
management UI. This design removes the inherited-only rendering path and sends
shared data through the same managed and unmanaged collections used by an
Agent's private directory.

This document supersedes the read-only shared-presentation restriction in
`2026-07-31-layered-agent-skill-scopes-design.md`. It does not change that
document's source/status tab model.

## Goals

- Remove the inherited-only collection and action rendering path.
- Represent shared managed and unmanaged entries with the existing complete
  `SkillTargetDetail` and `UnmanagedItemDto` contracts.
- Reuse the same managed/unmanaged collections, cards, list rows, search,
  selection, paging, busy states, and error behavior for both sources.
- Provide single and batch deletion for shared managed and unmanaged Skills.
- Keep shared adoption on the standard unmanaged collection path.
- Make every shared destructive confirmation explain its global impact.
- Preserve the center-library Skill when a managed shared distribution is
  removed.
- Protect `~/.agents/skills` mutations with strict owner and path validation.

## Non-goals

- Adding an `全部` status tab.
- Adding another shared-specific card, list, toolbar, or collection component.
- Allowing center-library distribution, skill-pack application, or direct
  target-to-pack moves to the virtual `agents` owner. The backend deliberately
  rejects `agents` as a distribution target today, and defining shared pack
  application semantics is a separate product decision.
- Changing the existing adoption rule that moves an unmanaged shared Skill
  into the center library and removes its original shared entry.
- Redesigning the Agent management page outside the Skills tab.

## Alternatives Considered

### Add delete actions to `InheritedSkillCollection`

This is the smallest patch, but it preserves two rendering and behavior paths.
Future card, list, accessibility, and loading-state changes would continue to
drift. It is rejected.

### Synthesize standard frontend DTOs from `InheritedSkillDetail`

The current inherited DTO does not contain target mode, status, hashes,
timestamps, claims, unmanaged reason, or owner metadata. Filling those fields
with frontend defaults would make the cards look reusable while presenting
incorrect state and breaking pack/filter behavior. It is rejected.

### Return complete standard DTOs and use one collection path

This is the selected approach. The backend projects shared rows as complete
managed targets and unmanaged items. The frontend selects an active source and
passes those arrays to the existing collections. Source-specific behavior is
expressed only through callbacks and capabilities.

## Data Contract

`InheritedSkillDetail` is removed from the Agent detail contract. A consuming
Agent receives two source-specific arrays whose element types already exist:

```text
inheritedManagedSkills: SkillTargetDetail[]
inheritedUnmanagedSkills: UnmanagedItemDto[]
```

Every managed entry contains its real `agents` target data, including
`actualMode`, `status`, hashes, timestamps, and claims. Every unmanaged entry
contains its real unmanaged ID, owner `agentId = "agents"`, path, inferred Skill
ID, hash, reason, and read-only flag. The frontend must not manufacture missing
fields or parse composite IDs.

The backend keeps deterministic normalized-path deduplication. If stale data
contains both a managed target and an unmanaged row for the same shared path,
the managed target wins. Both arrays are sorted by Skill name and path so all
consuming Agents receive a stable projection.

Non-consuming Agents return empty arrays and `inheritsSharedSkills = false`.

## Frontend Architecture

`SkillsTab` keeps the orthogonal state introduced by the layered-source work:

```text
source: agent | shared
status: managed | unmanaged | builtin
```

It derives source-neutral active collections:

```text
activeManagedSkills = agent source
  ? detail.skills
  : detail.inheritedManagedSkills

activeUnmanagedSkills = agent source
  ? agent-owned unmanaged items
  : detail.inheritedUnmanagedSkills
```

Search, counts, filtering, paging, selection, and action state operate on the
active arrays. Rendering has one managed branch that calls
`ManagedSkillCollection` and one unmanaged branch that calls
`UnmanagedSkillCollection`. `InheritedSkillCollection` and
`openInheritedSkill` are deleted.

The existing `ManagedSkillCard`, `ManagedSkillListRow`, and unmanaged card/list
rendering remain the only domain-level presentation. Card and list actions keep
their current event propagation, keyboard, busy, and accessibility behavior.

The source notice remains above the shared collection to explain that the
selected Agent reads `~/.agents/skills` by default.

## Capability Model

Using the same component does not send unsupported mutations to a different
owner. The common surface receives capabilities derived from the active
source:

| Capability | Agent managed | Shared managed | Agent unmanaged | Shared unmanaged |
| --- | --- | --- | --- | --- |
| Search and card/list view | Yes | Yes | Yes | Yes |
| Single delete | Yes | Yes | Yes | Yes |
| Multi-select and batch delete | Yes | Yes | Yes | Yes |
| Adopt to center | N/A | N/A | Yes | Yes |
| Batch adopt | N/A | N/A | Yes | Yes |
| Skill-pack filter | Yes | Yes, using real claims | N/A | N/A |
| Add/distribute center Skill | Yes | No | N/A | N/A |
| Move direct target into pack | Yes | No | N/A | N/A |
| Apply/revoke pack rail | Yes | No | N/A | N/A |
| Quick takeover of center duplicate | N/A | N/A | Yes | No |

Unsupported controls are omitted by capability props on the same toolbar and
collection path. They are not replaced with shared-only JSX. Shared batch
adoption uses the normal batch adoption flow with each item's real owner set to
`agents` and the existing `import_cleanup` default.

## Deletion Semantics

### Shared managed Skill

The standard managed card/list delete action opens a source-aware destructive
confirmation before calling `deleteSkillTargetDistributions` with the real
target ID.

The confirmation states that:

- the entry will be removed from `~/.agents/skills`;
- every Agent inheriting that directory will stop seeing it;
- the center-library Skill is preserved and can be distributed again later.

Batch deletion uses the same API and confirmation semantics for all selected
shared target IDs. Partial failures remain visible through the existing failure
result and leave failed entries selected or available for retry.

### Shared unmanaged Skill

The standard unmanaged card/list exposes both `接管` and `删除`. Delete opens a
source-aware confirmation before calling `deleteUnmanagedAgentSkill` or its
batch variant with owner `agents` and the real unmanaged ID.

The confirmation states that the original directory under
`~/.agents/skills` will be permanently removed, all inheriting Agents are
affected, and the Skill is not copied to the center library. Adoption remains
the safe alternative when the user wants to keep the Skill.

Single-item failures keep the dialog open and display the error inline. Batch
failures use the existing per-item result handling.

## Filesystem Safety

Shared deletion is a global mutation and must fail closed. Before removing a
managed or unmanaged shared path, the backend verifies:

- the database row belongs to owner `agents`;
- the path is strictly below `~/.agents/skills`, never the root itself;
- normalized traversal cannot escape the owned root;
- neither the shared root nor its `.agents` parent is a symbolic link;
- a leaf symbolic link is allowed, but deletion unlinks only the leaf and never
  deletes its resolved target.

Managed deletion adds this validation before `remove_target_completely` touches
the filesystem. Unmanaged shared deletion uses the same strict shared-path
predicate instead of the more permissive generic Agent predicate. Validation
failure preserves both the filesystem entry and database row.

Non-shared target deletion keeps its current behavior in this focused change.

## Refresh and Error Handling

After a successful shared mutation, the page refreshes global unmanaged
inventory, the selected Agent detail, and overview counts. The active source
and status stay selected. Because every consumer derives its shared projection
from owner `agents`, the next detail load for Codex, Kimi Code, OpenClaw, or
ZCode reflects the same mutation.

An API failure does not optimistically remove a card. Busy state is scoped to
the affected IDs, destructive controls cannot be double-submitted, and action
buttons do not open the detail panel through event bubbling.

## Localization

New confirmation titles, global-impact explanations, preservation text, and
success/error labels are added together in all five locale files:

- English
- Chinese
- Japanese
- Korean
- Turkish

Existing generic action labels are reused where their meaning is unchanged.

## Tests

Frontend regression coverage verifies:

- shared managed entries use the standard managed card and list behavior;
- shared unmanaged entries use the standard unmanaged card and list behavior;
- `InheritedSkillCollection` is no longer part of the render path;
- both shared states expose single delete in card and list modes;
- shared managed deletion waits for global confirmation and sends the real
  target ID;
- shared unmanaged deletion waits for global confirmation and sends owner
  `agents` plus the real unmanaged ID;
- shared managed/unmanaged multi-select and batch actions use the standard
  controls;
- adoption and delete actions do not open Skill details;
- managed deletion preserves the center-library explanation, while unmanaged
  deletion explains permanent removal;
- failures retain the item and restore usable controls;
- the four shared consumers retain the two-layer tabs, while non-consumers do
  not gain a source row;
- empty, search, paging, runtime-switch, and five-locale behavior remain
  covered.

Rust regression coverage verifies:

- shared projection returns complete `SkillTargetDetail` and
  `UnmanagedItemDto` values with real claims and owner IDs;
- managed rows win normalized-path deduplication;
- deleting a shared managed leaf removes its target/link and database target
  while preserving the center Skill;
- deleting a shared unmanaged leaf removes only the shared source and does not
  import it;
- root, outside-root, traversal, and symlinked-parent paths are rejected for
  both shared managed and unmanaged deletion;
- deleting a leaf symlink removes only the link;
- batch deletion reports partial failure without deleting rejected paths;
- all consumers lose the deleted shared projection and non-consumers remain
  unaffected.

## Delivery Constraints

- No new dependency.
- No brand, release, signing, or generated-file changes.
- All required frontend and Rust checks must pass before the Issue #92 pull
  request is merged into `dev`.
