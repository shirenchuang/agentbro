# Layered Agent Skill Scopes Design

## Context

AgentBro currently renders `已管理`, `未管理`, `共享继承`, and optional
`内置只读` entries in one segmented control. This mixes two independent
dimensions:

- source or ownership: Agent-specific versus inherited from
  `~/.agents/skills`;
- management state: managed versus unmanaged.

The mismatch is visible for Codex: its 43 shared inherited Skills contain one
managed entry and 42 unmanaged entries, but the current shared view does not
show those states.

Codex, Kimi Code, OpenClaw, and ZCode consume the shared directory. Other
Agents do not and should not receive an extra source-selection layer.

## Goals

- Separate source selection from management-state selection.
- Render the source layer only for Agents that inherit `~/.agents/skills`.
- Show managed and unmanaged states inside the shared inherited source.
- Preserve a direct, compact status control for non-consuming Agents.
- Allow a shared unmanaged Skill to be adopted safely into the center library.
- Keep shared inventory ownership centralized under the virtual `agents`
  source.

## Non-goals

- Adding an `全部` status tab.
- Duplicating shared rows into every consuming Agent.
- Keeping an adopted Skill in `~/.agents/skills`.
- Changing the existing diagnosis that treats managed shared-directory copies
  as cleanup candidates.
- Adding new dependencies or redesigning the surrounding Agent management
  page.

## Alternatives Considered

### Flat tabs with clearer labels

The existing row could become `专属已管理`, `专属未管理`, and `共享继承`.
This is the smallest frontend change, but it still combines source and status
in one control and requires a special nested filter only for shared Skills.

### Source and status dropdowns

Two dropdowns would model the dimensions correctly and save space. They make
important counts less visible and add interaction cost for a small fixed set
of choices.

### Conditional two-layer tabs

The selected design uses one source row and one status row for shared
consumers. Non-consumers omit the source row. It keeps counts visible, models
the data directly, and scales to future shared consumers without Agent-ID
exceptions in the frontend.

## Interaction Design

### Shared consumers

For an Agent that reports `inheritsSharedSkills = true`, the Skills view shows:

```text
[ Agent 专属 76 ] [ 共享继承 43 ]
[ 已管理 76 ] [ 未管理 0 ]
```

The initial selection is `Agent 专属` and `已管理`.

Selecting `共享继承` changes only the second row:

```text
[ Agent 专属 76 ] [ 共享继承 43 ]
[ 已管理 1 ] [ 未管理 42 ]
```

The source row remains visible when the shared directory is empty because it
communicates an Agent capability, not merely the presence of current data.

### Non-consumers

For an Agent that does not inherit the shared directory, no source row is
rendered:

```text
[ 已管理 N ] [ 未管理 M ]
```

The initial selection is `已管理`.

### Built-in read-only Skills

When the selected Agent source contains built-in read-only Skills, an
`内置只读` status entry is appended to the status row. It is omitted when its
count is zero. Shared inherited items do not use this status unless the
backend explicitly projects a read-only shared item in the future.

### Counts and search

Source and status counts use the complete unfiltered collections. Search
filters only the visible content and never changes tab counts.

`Agent 专属` counts managed, unmanaged, and built-in read-only items owned by
the selected Agent. `共享继承` counts the deduplicated shared projection.

The existing top-level `Skills (N)` count continues to count unique usable
Skills. Shared entries are not counted again under Agent-specific managed or
unmanaged states.

### State reset

Changing the selected Agent resets the view:

- shared consumer: `Agent 专属` and `已管理`;
- non-consumer: `已管理`.

If the previous Agent used the shared source and the next Agent is a
non-consumer, the stale shared selection is discarded before rendering.

Installing a Skill into the selected Agent returns the view to
`Agent 专属` and `已管理`.

## Shared Skill Presentation

Every shared inherited row and card displays a management-state tag.

- Managed shared entries can open Skill details but expose no delete or
  distribution action from an individual consuming Agent.
- Unmanaged shared entries expose `接管到中心库`.
- Neither state is duplicated under the Agent-specific source.

Adoption uses the virtual `agents` owner and the raw unmanaged-item ID supplied
by the backend. Successful adoption copies the Skill into the center library
and removes the source from `~/.agents/skills`; the item therefore leaves the
shared inherited list instead of changing in place to managed.

The UI explains this cleanup behavior before confirmation and refreshes the
shared inventory plus the currently visible consuming Agent after completion.

## Data Contract

`InheritedSkillDetail` gains explicit fields:

```text
managed: boolean
targetId: string | null
unmanagedId: string | null
```

The existing composite `id` remains a stable UI key. The frontend must not
parse `target:` or `unmanaged:` prefixes to infer state or action ownership.

The backend projection returns:

- managed target rows with `managed = true`, `targetId`, and no
  `unmanagedId`;
- unmanaged inventory rows with `managed = false`, `unmanagedId`, and no
  `targetId`.

When stale data contains a managed target and unmanaged row for the same
logical path, the managed target wins deterministic path deduplication.

## Safety

Exposing shared adoption makes unmanaged-item ownership validation mandatory.
Before previewing or executing adoption, the backend verifies:

- the unmanaged row belongs to the requested owner;
- the row represents a supported Skill item type;
- destructive cleanup paths remain below a recognized owned Skill root;
- for the virtual `agents` owner, the path is strictly below
  `~/.agents/skills` and is not the root itself;
- symlink and path traversal protections remain in force.

A mismatched owner or unsafe path is rejected without copying or deleting any
file.

## Frontend Structure

The current single scope state is replaced with orthogonal state:

```text
source: agent | shared
status: managed | unmanaged | builtin
```

The source control is conditional on `inheritsSharedSkills`. Existing managed,
unmanaged, built-in, card/list, search, paging, batch management, and detail
components remain in place. Status-specific toolbars appear only for their
existing Agent-specific views.

The shared collection receives the projected state fields and an adoption
callback. Shared unmanaged cards and list rows show the action; managed rows
remain view-only.

## Error Handling

- An absent or unreadable shared directory produces zero shared counts and an
  empty shared state without hiding the source capability.
- A stale shared unmanaged ID reports the existing localized stale-inventory
  error and prompts a rescan.
- Adoption errors preserve the source file and leave the current filters
  selected.
- Failed detail refresh does not falsely report adoption success.

## Localization

All five locale files receive source, status, action, cleanup explanation, and
empty-state strings together:

- English
- Chinese
- Japanese
- Korean
- Turkish

## Tests

Rust regression coverage:

- managed and unmanaged shared rows project explicit IDs and state;
- managed rows win same-path deduplication;
- all four shared consumers receive the projection;
- non-consumers receive no shared projection;
- wrong-owner and unsafe-root adoption are rejected without deletion;
- valid shared adoption creates the center copy, removes the shared source,
  and refreshes inherited projection;
- nested package-wrapper shared Skills remain supported.

Frontend regression coverage:

- shared consumers render both source and status rows;
- consumers default to `Agent 专属` and `已管理`;
- shared status counts and filtering distinguish managed from unmanaged;
- there is no `全部` tab;
- shared unmanaged rows call adoption with owner `agents` and the raw
  unmanaged ID;
- shared managed rows expose no mutation action;
- empty shared consumers retain the source row;
- non-consumers render no source row;
- changing Agents resets stale source and status selection;
- search does not change tab counts;
- built-in read-only behavior remains available;
- card/list views, paging, and all five locales remain covered.
