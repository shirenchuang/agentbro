# Agent Sync Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the `Agent 同步` install tab into a task-focused local sync page with a pending inbox, while preserving list/card views and existing adopt flows.

**Architecture:** Keep the implementation inside `AgentSyncPanel` in `src/components/skills-v2/InstallView.tsx`, with small private render helpers only if the JSX becomes hard to read. First derive pending/importable/conflict/managed datasets from the existing inventory, then reorganize the UI around a task header, summary panel, Agent strip, pending inbox, and advanced disclosure. Backend commands and adopt semantics stay unchanged.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, existing CSS in `src/components/skills-v2/SkillManagerV2.css`, existing `skillApiV2` inventory/adopt APIs.

---

### Task 1: Add Failing Coverage For The New Default Hierarchy

**Files:**
- Modify: `src/test/skillManagerV2View.test.tsx`
- Read: `docs/superpowers/specs/2026-06-18-agent-sync-redesign-design.md`

- [ ] **Step 1: Add a test for pending-only default content**

Add this test inside `describe('Agent sync local agent chips', () => { ... })` after the existing sorting/dropdown tests:

```tsx
it('shows a task-focused pending inbox by default and hides managed skills', async () => {
  const { skillApiV2 } = await import('../services/skillApiV2')
  const inventory: AgentSkillInventoryAgent[] = [
    {
      agentId: 'claude-code',
      displayName: 'Claude Code',
      iconKey: 'claude-code',
      skillsDir: '/Users/me/.claude/skills',
      installed: true,
      managedCount: 3,
      unmanagedCount: 2,
      importableCount: 1,
      items: [
        {
          id: 'managed-alpha',
          agentId: 'claude-code',
          skillId: 'managed-alpha',
          name: 'managed-alpha',
          path: '/Users/me/.claude/skills/managed-alpha',
          managed: true,
          canImport: false,
          status: 'managed',
          statusLabel: '已管理',
          reason: null,
          targetId: 'target-managed-alpha',
          actualMode: 'link',
          hash: 'hash-managed-alpha',
        },
        {
          id: 'local-alpha',
          agentId: 'claude-code',
          skillId: 'alpha',
          name: 'alpha',
          path: '/Users/me/.claude/skills/alpha',
          managed: false,
          canImport: true,
          status: 'unmanaged',
          statusLabel: '未管理',
          reason: null,
          targetId: null,
          actualMode: null,
          hash: 'hash-alpha',
        },
        {
          id: 'local-bird',
          agentId: 'claude-code',
          skillId: 'bird',
          name: 'bird',
          path: '/Users/me/.claude/skills/bird',
          managed: false,
          canImport: false,
          status: 'conflict',
          statusLabel: '未管理 · 同名冲突',
          reason: 'same_name_as_center_skill',
          targetId: null,
          actualMode: null,
          hash: 'hash-bird',
        },
      ],
    },
  ]
  vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

  const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
  render(<AgentSyncPanel onDone={() => {}} />)

  expect(await screen.findByText('本机 Agent 同步')).toBeInTheDocument()
  expect(screen.getByText('把散落在各 Agent 里的 Skills 收进中心库')).toBeInTheDocument()
  expect(screen.getByText('发现 1 个可接管 Skill，1 个同名冲突')).toBeInTheDocument()
  expect(screen.getByText('待处理收纳箱')).toBeInTheDocument()
  expect(screen.getByText('alpha')).toBeInTheDocument()
  expect(screen.getByText('bird')).toBeInTheDocument()
  expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()
  expect(screen.getByText('3 已管理，默认隐藏')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "shows a task-focused pending inbox"
```

Expected: FAIL because the current UI does not render `本机 Agent 同步`, `待处理收纳箱`, or hide managed rows by default.

- [ ] **Step 3: Add a test for advanced managed visibility**

Add this test in the same describe block:

```tsx
it('can reveal managed skills from advanced controls', async () => {
  const { skillApiV2 } = await import('../services/skillApiV2')
  const inventory: AgentSkillInventoryAgent[] = [
    {
      agentId: 'claude-code',
      displayName: 'Claude Code',
      iconKey: 'claude-code',
      skillsDir: '/Users/me/.claude/skills',
      installed: true,
      managedCount: 1,
      unmanagedCount: 1,
      importableCount: 1,
      items: [
        {
          id: 'managed-alpha',
          agentId: 'claude-code',
          skillId: 'managed-alpha',
          name: 'managed-alpha',
          path: '/Users/me/.claude/skills/managed-alpha',
          managed: true,
          canImport: false,
          status: 'managed',
          statusLabel: '已管理',
          reason: null,
          targetId: 'target-managed-alpha',
          actualMode: 'link',
          hash: 'hash-managed-alpha',
        },
        {
          id: 'local-alpha',
          agentId: 'claude-code',
          skillId: 'alpha',
          name: 'alpha',
          path: '/Users/me/.claude/skills/alpha',
          managed: false,
          canImport: true,
          status: 'unmanaged',
          statusLabel: '未管理',
          reason: null,
          targetId: null,
          actualMode: null,
          hash: 'hash-alpha',
        },
      ],
    },
  ]
  vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

  const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
  render(<AgentSyncPanel onDone={() => {}} />)

  expect(await screen.findByText('alpha')).toBeInTheDocument()
  expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()

  fireEvent.click(screen.getByRole('button', { name: '高级查看' }))
  fireEvent.click(screen.getByLabelText('显示已管理 Skills'))

  expect(screen.getByText('managed-alpha')).toBeInTheDocument()
})
```

- [ ] **Step 4: Run the focused advanced test and verify it fails**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "can reveal managed skills"
```

Expected: FAIL because `高级查看` and `显示已管理 Skills` do not exist yet.

- [ ] **Step 5: Commit only the failing tests if committing between red/green phases**

Use this only if the team wants red-phase commits:

```bash
git add src/test/skillManagerV2View.test.tsx
git commit -m "test: cover agent sync pending inbox redesign"
```

Expected: commit includes only test changes.

### Task 2: Derive Pending And Advanced Row Sets

**Files:**
- Modify: `src/components/skills-v2/InstallView.tsx`
- Test: `src/test/skillManagerV2View.test.tsx`

- [ ] **Step 1: Add state for advanced controls**

Inside `AgentSyncPanel`, replace the existing default view mode state and add advanced state near the existing state declarations:

```tsx
const [viewMode, setViewMode] = useState<AgentSyncViewMode>('list')
const [advancedOpen, setAdvancedOpen] = useState(false)
const [showManaged, setShowManaged] = useState(false)
```

Keep `selectedAgent`, `statusFilter`, `query`, and the existing import/adopt state.

- [ ] **Step 2: Replace row derivation with pending-aware datasets**

Replace the current `visibleAgents`, `rows`, `importableRows`, `oneClickItems`, `oneClickImportable`, and `oneClickConflicts` block with:

```tsx
const visibleAgents = useMemo(
  () => selectedAgent === 'all' ? agents : agents.filter((agent) => agent.agentId === selectedAgent),
  [agents, selectedAgent],
)

const allRows = useMemo(
  () => visibleAgents.flatMap((agent) => agent.items.map((item) => ({ agent, item }))),
  [visibleAgents],
)

const q = query.trim().toLowerCase()
const matchesQuery = ({ item }: AgentSyncRow) => {
  if (!q) return true
  return [item.name, item.skillId, item.path, item.statusLabel, item.reason || '']
    .join(' ')
    .toLowerCase()
    .includes(q)
}

const pendingRows = useMemo(
  () => allRows.filter(({ item }) => !item.managed || item.status === 'conflict'),
  [allRows],
)
const managedRows = useMemo(
  () => allRows.filter(({ item }) => item.managed),
  [allRows],
)

const baseRows = showManaged ? allRows : pendingRows
const rows = useMemo(() => {
  return baseRows
    .filter(({ item }) => {
      if (statusFilter === 'managed' && !item.managed) return false
      if (statusFilter === 'importable' && !item.canImport) return false
      if (statusFilter === 'unmanaged' && item.managed) return false
      if (statusFilter === 'conflict' && item.status !== 'conflict') return false
      return true
    })
    .filter(matchesQuery)
}, [baseRows, statusFilter, q])

const importableRows = rows.filter(({ item }) => item.canImport)
const oneClickItems = pendingRows.map(({ item }) => item)
const oneClickImportable = oneClickItems.filter((item) => item.canImport)
const oneClickConflicts = oneClickItems.filter((item) => !item.managed && item.status === 'conflict')
```

Do not change `toggle`, `selectAllVisible`, `openAdoptPreview`, `adoptItems`, or `executeOneClickOrganize` in this task.

- [ ] **Step 3: Use the new datasets for totals**

Keep the existing totals and add pending counts:

```tsx
const totalManaged = agents.reduce((sum, agent) => sum + agent.managedCount, 0)
const totalUnmanaged = agents.reduce((sum, agent) => sum + agent.unmanagedCount, 0)
const totalImportable = agents.reduce((sum, agent) => sum + agent.importableCount, 0)
const totalConflicts = agents.reduce(
  (sum, agent) => sum + agent.items.filter((item) => !item.managed && item.status === 'conflict').length,
  0,
)
const pendingCount = pendingRows.length
const managedCount = managedRows.length
```

- [ ] **Step 4: Run the pending-only test**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "shows a task-focused pending inbox"
```

Expected: still FAIL because the layout labels are not implemented yet, but managed rows should now be absent from the queried DOM once JSX is updated in the next task.

- [ ] **Step 5: Commit the data derivation if tests are at the expected intermediate state**

```bash
git add src/components/skills-v2/InstallView.tsx
git commit -m "refactor: derive pending agent sync rows"
```

Expected: commit contains only `InstallView.tsx`.

### Task 3: Build The Task Header, Summary Panel, And Agent Strip

**Files:**
- Modify: `src/components/skills-v2/InstallView.tsx`
- Modify: `src/components/skills-v2/SkillManagerV2.css`
- Test: `src/test/skillManagerV2View.test.tsx`

- [ ] **Step 1: Add small label helpers**

Add these helper functions above `export function AgentSyncPanel`:

```tsx
function pluralSkill(count: number) {
  return `${count} 个`
}

function agentAttentionLabel(agent: AgentSkillInventoryAgent) {
  const conflicts = agent.items.filter((item) => !item.managed && item.status === 'conflict').length
  if (agent.importableCount > 0 && conflicts > 0) return `${agent.importableCount} 可接管 · ${conflicts} 冲突`
  if (agent.importableCount > 0) return `${agent.importableCount} 可接管`
  if (conflicts > 0) return `${conflicts} 冲突`
  return '健康'
}

function agentAttentionTone(agent: AgentSkillInventoryAgent) {
  if (agent.items.some((item) => !item.managed && item.status === 'conflict')) return 'conflict'
  if (agent.importableCount > 0 || agent.unmanagedCount > 0) return 'attention'
  return 'ok'
}
```

- [ ] **Step 2: Replace the top toolbar JSX**

Inside `return`, replace the current first `sm2__market-boardbar`, statline, note, and action bar with:

```tsx
<div className="sm2__agent-sync-header">
  <div>
    <span className="sm2__agent-sync-eyebrow">本机 Agent 同步</span>
    <h3>把散落在各 Agent 里的 Skills 收进中心库</h3>
    <p>优先处理未管理和冲突项；已管理 Skills 默认隐藏。</p>
  </div>
  <button className="sm2__btn" onClick={scan} disabled={scanning || importing}>
    {scanning ? '扫描中…' : '重新扫描'}
  </button>
</div>

<div className="sm2__agent-sync-summary">
  <div className="sm2__agent-sync-summary-main">
    <span className="sm2__agent-sync-summary-mark" aria-hidden="true">!</span>
    <div>
      <strong>
        {totalImportable > 0
          ? `发现 ${pluralSkill(totalImportable)}可接管 Skill，${totalConflicts} 个同名冲突`
          : totalConflicts > 0
            ? `发现 ${totalConflicts} 个同名冲突`
            : '本机 Agent Skills 已整理完成'}
      </strong>
      <p>
        {totalImportable > 0
          ? '建议先一键整理可接管项，再逐个处理冲突。'
          : totalConflicts > 0
            ? '这些 Skill 需要逐个确认来源和导入方式。'
            : '没有需要接管的 Skill，可以随时重新扫描。'}
      </p>
    </div>
  </div>
  <div className="sm2__agent-sync-summary-chips">
    <span>{agents.length} Agent</span>
    <span>{totalManaged} 已管理，默认隐藏</span>
    <span>{totalImportable > 0 ? '软连接推荐' : `${pendingCount} 待处理`}</span>
  </div>
  <div className="sm2__agent-sync-summary-actions">
    <button className="sm2__btn" onClick={() => setOneClickOpen(true)} disabled={importing || totalImportable === 0}>
      查看整理方式
    </button>
    <button className="sm2__btn sm2__btn--featured" onClick={openOneClickOrganize} disabled={(oneClickImportable.length === 0 && oneClickConflicts.length === 0) || importing || scanning}>
      {importing ? '整理中…' : totalImportable > 0 ? `一键整理 ${totalImportable} 个` : totalConflicts > 0 ? '处理冲突' : '已完成'}
    </button>
  </div>
</div>

<div className="sm2__agent-sync-agent-strip" aria-label="Agent 同步摘要">
  <button
    className={`sm2__agent-sync-agent-card${selectedAgent === 'all' ? ' sm2__agent-sync-agent-card--active' : ''}`}
    onClick={() => setSelectedAgent('all')}
  >
    <strong>全部 Agent</strong>
    <span>{totalImportable} 可接管 · {totalConflicts} 冲突</span>
    <em>{totalManaged} 已管理</em>
  </button>
  {agents.map((agent) => (
    <button
      key={agent.agentId}
      className={`sm2__agent-sync-agent-card sm2__agent-sync-agent-card--${agentAttentionTone(agent)}${selectedAgent === agent.agentId ? ' sm2__agent-sync-agent-card--active' : ''}`}
      onClick={() => setSelectedAgent(agent.agentId)}
    >
      <span className="sm2__agent-sync-agent-name">
        <AgentIconBadge iconKey={agent.iconKey} size={24} title={agent.displayName} />
        <strong>{agent.displayName}</strong>
      </span>
      <span>{agentAttentionLabel(agent)}</span>
      <em>{agent.managedCount} 已管理</em>
    </button>
  ))}
</div>
```

- [ ] **Step 3: Add header and strip CSS**

Append to the Agent sync CSS section in `SkillManagerV2.css`:

```css
.sm2__agent-sync-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 4px 2px 0;
}

.sm2__agent-sync-eyebrow {
  display: block;
  margin-bottom: 5px;
  color: var(--text-secondary, #64748b);
  font-size: 12px;
  font-weight: 800;
}

.sm2__agent-sync-header h3 {
  margin: 0;
  color: var(--text-primary, #0f172a);
  font-size: 20px;
  line-height: 1.25;
}

.sm2__agent-sync-header p {
  margin: 6px 0 0;
  color: var(--text-secondary, #64748b);
  font-size: 13px;
}

.sm2__agent-sync-summary {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 14px 18px;
  padding: 16px;
  border: 1px solid var(--border, rgba(15, 23, 42, 0.1));
  border-radius: 12px;
  background: rgba(248, 250, 252, 0.9);
}

.sm2__agent-sync-summary-main {
  display: flex;
  gap: 12px;
  min-width: 0;
}

.sm2__agent-sync-summary-mark {
  display: inline-grid;
  place-items: center;
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  border-radius: 50%;
  background: #0f172a;
  color: #fff;
  font-weight: 900;
}

.sm2__agent-sync-summary-main strong {
  display: block;
  color: var(--text-primary, #0f172a);
  font-size: 17px;
  line-height: 1.35;
}

.sm2__agent-sync-summary-main p {
  margin: 4px 0 0;
  color: var(--text-secondary, #64748b);
  font-size: 13px;
}

.sm2__agent-sync-summary-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  grid-column: 1 / -1;
}

.sm2__agent-sync-summary-chips span {
  padding: 5px 9px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.78);
  color: var(--text-secondary, #64748b);
  font-size: 12px;
  font-weight: 750;
}

.sm2__agent-sync-summary-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.sm2__agent-sync-agent-strip {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  gap: 10px;
}

.sm2__agent-sync-agent-card {
  display: grid;
  gap: 5px;
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--border, rgba(15, 23, 42, 0.1));
  border-radius: 10px;
  background: var(--card-bg, rgba(255, 255, 255, 0.78));
  color: var(--text-primary, #0f172a);
  text-align: left;
  cursor: pointer;
}

.sm2__agent-sync-agent-card--active {
  border-color: rgba(10, 132, 255, 0.45);
  box-shadow: 0 0 0 1px rgba(10, 132, 255, 0.18);
}

.sm2__agent-sync-agent-name {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.sm2__agent-sync-agent-card span,
.sm2__agent-sync-agent-card em {
  color: var(--text-secondary, #64748b);
  font-size: 12px;
  font-style: normal;
  font-weight: 650;
}
```

- [ ] **Step 4: Run the focused hierarchy test**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "shows a task-focused pending inbox"
```

Expected: FAIL only on `待处理收纳箱` if the inbox title has not been implemented yet.

- [ ] **Step 5: Commit the header and summary**

```bash
git add src/components/skills-v2/InstallView.tsx src/components/skills-v2/SkillManagerV2.css
git commit -m "feat: add agent sync task summary"
```

Expected: commit includes the task header, summary panel, Agent strip, and CSS.

### Task 4: Rebuild The Pending Inbox With List/Card Parity

**Files:**
- Modify: `src/components/skills-v2/InstallView.tsx`
- Modify: `src/components/skills-v2/SkillManagerV2.css`
- Modify: `src/test/skillManagerV2View.test.tsx`

- [ ] **Step 1: Replace the old action bar with an inbox toolbar**

Replace the current `sm2__agent-sync-actions` block with:

```tsx
<section className="sm2__agent-sync-inbox" aria-labelledby="agent-sync-inbox-title">
  <div className="sm2__agent-sync-inbox-head">
    <div>
      <h3 id="agent-sync-inbox-title">待处理收纳箱</h3>
      <p>只显示需要用户决策的 Skill。已选择 {selectedIds.size} 个。</p>
    </div>
    <div className="sm2__agent-sync-inbox-tools">
      <div className="sm2__search-wrapper">
        <span className="sm2__search-icon">⌕</span>
        <input
          className="sm2__search sm2__search--with-icon"
          placeholder="搜索待处理 Skill…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <label className="sm2__agent-select sm2__agent-select--compact">
        <span>选择 Agent</span>
        <select
          aria-label="选择 Agent"
          value={selectedAgent}
          onChange={(e) => setSelectedAgent(e.target.value)}
          disabled={scanning}
        >
          <option value="all">全部 Agent</option>
          {agents.map((agent) => (
            <option key={agent.agentId} value={agent.agentId}>
              {agent.displayName} · {agent.importableCount} 可接管
            </option>
          ))}
        </select>
      </label>
      <div className="sm2__view-toggle sm2__view-toggle--soft" aria-label="切换待处理视图">
        <button aria-pressed={viewMode === 'list'} className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
        <button aria-pressed={viewMode === 'cards'} className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
      </div>
    </div>
  </div>
  <div className="sm2__agent-sync-actions">
    <button className="sm2__btn" onClick={selectAllVisible} disabled={importableRows.length === 0 || importing}>选择当前可接管</button>
    <button className="sm2__btn" onClick={() => setSelectedIds(new Set())} disabled={selectedIds.size === 0 || importing}>清空</button>
    <button className="sm2__btn sm2__btn--primary" onClick={importSelected} disabled={selectedIds.size === 0 || importing}>
      {importing ? '接管中…' : '接管到中心库'}
    </button>
  </div>
```

Move the existing loading/empty/list/card rendering inside this `<section>` and close the section after the list/card block.

- [ ] **Step 2: Update empty text**

Change the empty state inside the section to:

```tsx
) : rows.length === 0 ? (
  <div className="sm2__empty">{showManaged ? '没有匹配的本地 Skill。' : '没有需要接管的 Skill。可以在高级查看中显示已管理 Skills。'}</div>
) : viewMode === 'cards' ? (
```

- [ ] **Step 3: Add a list/card parity test**

Add this test inside the same describe block:

```tsx
it('keeps list and card views on the same pending dataset', async () => {
  const { skillApiV2 } = await import('../services/skillApiV2')
  const inventory: AgentSkillInventoryAgent[] = [
    {
      agentId: 'claude-code',
      displayName: 'Claude Code',
      iconKey: 'claude-code',
      skillsDir: '/Users/me/.claude/skills',
      installed: true,
      managedCount: 1,
      unmanagedCount: 1,
      importableCount: 1,
      items: [
        {
          id: 'managed-alpha',
          agentId: 'claude-code',
          skillId: 'managed-alpha',
          name: 'managed-alpha',
          path: '/Users/me/.claude/skills/managed-alpha',
          managed: true,
          canImport: false,
          status: 'managed',
          statusLabel: '已管理',
          reason: null,
          targetId: 'target-managed-alpha',
          actualMode: 'link',
          hash: 'hash-managed-alpha',
        },
        {
          id: 'local-alpha',
          agentId: 'claude-code',
          skillId: 'alpha',
          name: 'alpha',
          path: '/Users/me/.claude/skills/alpha',
          managed: false,
          canImport: true,
          status: 'unmanaged',
          statusLabel: '未管理',
          reason: null,
          targetId: null,
          actualMode: null,
          hash: 'hash-alpha',
        },
      ],
    },
  ]
  vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

  const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
  const { container } = render(<AgentSyncPanel onDone={() => {}} />)

  expect(await screen.findByText('alpha')).toBeInTheDocument()
  expect(container.querySelector('.sm2__agent-sync-listview')).not.toBeNull()
  expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()

  fireEvent.click(screen.getByRole('button', { name: '卡片' }))

  expect(container.querySelector('.sm2__install-grid')).not.toBeNull()
  expect(screen.getByText('alpha')).toBeInTheDocument()
  expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()
})
```

- [ ] **Step 4: Add inbox CSS**

Append:

```css
.sm2__agent-sync-inbox {
  display: grid;
  gap: 12px;
}

.sm2__agent-sync-inbox-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.sm2__agent-sync-inbox-head h3 {
  margin: 0;
  color: var(--text-primary, #0f172a);
  font-size: 16px;
}

.sm2__agent-sync-inbox-head p {
  margin: 4px 0 0;
  color: var(--text-secondary, #64748b);
  font-size: 12px;
}

.sm2__agent-sync-inbox-tools {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  flex-wrap: wrap;
}

.sm2__agent-select--compact {
  flex-basis: 190px;
}
```

- [ ] **Step 5: Run the inbox tests**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "pending inbox|list and card views"
```

Expected: PASS for the two new default/list-card tests.

- [ ] **Step 6: Commit the inbox layout**

```bash
git add src/components/skills-v2/InstallView.tsx src/components/skills-v2/SkillManagerV2.css src/test/skillManagerV2View.test.tsx
git commit -m "feat: focus agent sync on pending inbox"
```

Expected: commit includes tests, inbox JSX, and inbox CSS.

### Task 5: Add Advanced Viewing Without Restoring Default Clutter

**Files:**
- Modify: `src/components/skills-v2/InstallView.tsx`
- Modify: `src/components/skills-v2/SkillManagerV2.css`
- Test: `src/test/skillManagerV2View.test.tsx`

- [ ] **Step 1: Add advanced controls under the inbox toolbar**

Insert this block before rendering progress/notice/error:

```tsx
<div className="sm2__agent-sync-advanced">
  <button
    className="sm2__btn sm2__btn--ghost"
    type="button"
    aria-expanded={advancedOpen}
    onClick={() => setAdvancedOpen((open) => !open)}
  >
    高级查看
  </button>
  {advancedOpen && (
    <div className="sm2__agent-sync-advanced-panel">
      <label className="sm2__agent-sync-managed-toggle">
        <input
          type="checkbox"
          checked={showManaged}
          onChange={(e) => setShowManaged(e.target.checked)}
        />
        <span>显示已管理 Skills</span>
      </label>
      <div className="sm2__view-toggle sm2__market-boardtabs">
        {AGENT_STATUS_TABS.map((tab) => (
          <button
            key={tab.id}
            className={statusFilter === tab.id ? 'active' : ''}
            onClick={() => setStatusFilter(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
    </div>
  )}
</div>
```

Remove the old always-visible status tab toolbar if any remains.

- [ ] **Step 2: Prevent invalid managed filter state**

Add this effect below the row derivation:

```tsx
useEffect(() => {
  if (!showManaged && statusFilter === 'managed') {
    setStatusFilter('all')
  }
}, [showManaged, statusFilter])
```

- [ ] **Step 3: Add advanced CSS**

Append:

```css
.sm2__agent-sync-advanced {
  display: grid;
  justify-items: start;
  gap: 10px;
}

.sm2__agent-sync-advanced-panel {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
  width: 100%;
  padding: 10px;
  border: 1px solid var(--border, rgba(15, 23, 42, 0.08));
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.62);
}

.sm2__agent-sync-managed-toggle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--text-secondary, #64748b);
  font-size: 12px;
  font-weight: 750;
}

.sm2__agent-sync-managed-toggle input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent, #007aff);
}
```

- [ ] **Step 4: Run the advanced visibility test**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "can reveal managed skills"
```

Expected: PASS.

- [ ] **Step 5: Run all Agent sync tests**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "Agent sync"
```

Expected: PASS. If existing tests fail because labels moved from always-visible toolbar to advanced controls, update assertions to open `高级查看` before interacting with status tabs.

- [ ] **Step 6: Commit advanced controls**

```bash
git add src/components/skills-v2/InstallView.tsx src/components/skills-v2/SkillManagerV2.css src/test/skillManagerV2View.test.tsx
git commit -m "feat: add advanced agent sync filters"
```

Expected: commit includes advanced disclosure and test updates.

### Task 6: Responsive Polish And Final Verification

**Files:**
- Modify: `src/components/skills-v2/SkillManagerV2.css`
- Optionally modify: `src/components/skills-v2/InstallView.tsx`

- [ ] **Step 1: Add responsive rules for narrow settings windows**

Append:

```css
@media (max-width: 760px) {
  .sm2__agent-sync-header,
  .sm2__agent-sync-inbox-head {
    flex-direction: column;
    align-items: stretch;
  }

  .sm2__agent-sync-summary {
    grid-template-columns: 1fr;
  }

  .sm2__agent-sync-summary-actions,
  .sm2__agent-sync-inbox-tools {
    justify-content: flex-start;
  }

  .sm2__agent-sync-summary-actions .sm2__btn,
  .sm2__agent-sync-inbox-tools .sm2__search-wrapper,
  .sm2__agent-sync-inbox-tools .sm2__agent-select,
  .sm2__agent-sync-inbox-tools .sm2__view-toggle {
    width: 100%;
  }
}
```

- [ ] **Step 2: Run focused tests**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx -- -t "Agent sync"
```

Expected: PASS.

- [ ] **Step 3: Run the full frontend test file**

Run:

```bash
pnpm test:run src/test/skillManagerV2View.test.tsx
```

Expected: PASS.

- [ ] **Step 4: Run lint**

Run:

```bash
pnpm lint
```

Expected: PASS.

- [ ] **Step 5: Run the production build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit verification polish**

```bash
git add src/components/skills-v2/SkillManagerV2.css src/components/skills-v2/InstallView.tsx src/test/skillManagerV2View.test.tsx
git commit -m "fix: polish agent sync responsive layout"
```

Expected: commit includes only final polish and any small accessibility fixes discovered during verification.

### Task 7: Manual QA Checklist

**Files:**
- No required file edits unless QA reveals a bug.

- [ ] **Step 1: Start the browser UI**

Run:

```bash
pnpm dev
```

Expected: Vite serves the app at `http://localhost:1423`.

- [ ] **Step 2: Open the Skills install Agent sync tab**

In the browser UI, navigate to Skills management, open `安装 Skills`, then `Agent 同步`.

Expected:

- The first visible block is `本机 Agent 同步`.
- The primary action is `一键整理 N 个` when importable skills exist.
- The main repeated area is `待处理收纳箱`.
- Managed skills are not visible in the default list.

- [ ] **Step 3: Check list and card views**

Click `列表`, then `卡片`.

Expected:

- Both views show the same pending skills.
- Selection state survives the view switch.
- Managed skills remain hidden until advanced controls enable them.

- [ ] **Step 4: Check advanced visibility**

Open `高级查看`, enable `显示已管理 Skills`, then choose `已管理`.

Expected:

- Managed skills become visible.
- Turning off `显示已管理 Skills` hides them again and returns the status filter to `全部`.

- [ ] **Step 5: Stop the dev server**

Stop the Vite process with `Ctrl-C`.

Expected: no long-running dev server remains unless the user asked to keep it running.
