import { useMemo, useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi } from '../../services/skillApi'
import { SkillCard } from './SkillCard'
import { InstallDialog } from './InstallDialog'

const AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']

export function SkillListView() {
  const { t } = useTranslation()
  const {
    skills, scanning, searchQuery, typeFilter, agentFilter,
    setSearchQuery, setTypeFilter, setAgentFilter,
    loadAll, batchMode, toggleBatchMode, batchSelected, clearBatch,
  } = useSkillStore()
  const [installOpen, setInstallOpen] = useState(false)

  const filtered = useMemo(() => {
    let list = skills
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      list = list.filter(s => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q))
    }
    if (typeFilter !== 'all') {
      list = list.filter(s => s.skillType === typeFilter)
    }
    if (agentFilter !== 'all') {
      list = list.filter(s => s.agents.some(a => a.agent === agentFilter))
    }
    return list
  }, [skills, searchQuery, typeFilter, agentFilter])

  const installed = useMemo(() => filtered.filter(s => s.source === 'island'), [filtered])
  const discovered = useMemo(() => filtered.filter(s => s.source === 'local'), [filtered])

  const handleBatchToggle = useCallback(async (enabled: boolean) => {
    for (const id of batchSelected) {
      const skill = skills.find(s => s.id === id)
      if (skill) {
        for (const agent of skill.agents) {
          await skillApi.toggle(id, agent.agent, enabled)
        }
      }
    }
    clearBatch()
    loadAll()
  }, [batchSelected, skills, clearBatch, loadAll])

  if (scanning) {
    return (
      <div className="skills-scanning">
        <div className="skills-spinner" />
        {t('skills.scanning')}
      </div>
    )
  }

  return (
    <div>
      <div className="skills-toolbar">
        <input
          className="skills-search"
          placeholder={t('skills.searchPlaceholder')}
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
        />
        <button className="skills-btn skills-btn--small" onClick={toggleBatchMode}>
          {batchMode ? t('skills.cancelBatch') : t('skills.batchMode')}
        </button>
        <button className="skills-btn skills-btn--primary skills-btn--small" onClick={() => setInstallOpen(true)}>
          + {t('skills.install')}
        </button>
      </div>

      <div className="skills-filter-chips" style={{ marginTop: 8 }}>
        {(['all', 'skill', 'mcp'] as const).map(f => (
          <button
            key={f}
            className={`skills-chip ${typeFilter === f ? 'skills-chip--active' : ''}`}
            onClick={() => setTypeFilter(f)}
          >
            {f === 'all' ? t('skills.all') : f.toUpperCase()}
          </button>
        ))}
        <span style={{ width: 8 }} />
        <button
          className={`skills-chip ${agentFilter === 'all' ? 'skills-chip--active' : ''}`}
          onClick={() => setAgentFilter('all')}
        >
          {t('skills.allAgents')}
        </button>
        {AGENTS.map(a => (
          <button
            key={a}
            className={`skills-chip ${agentFilter === a ? 'skills-chip--active' : ''}`}
            onClick={() => setAgentFilter(a)}
          >
            {a}
          </button>
        ))}
      </div>

      {batchMode && batchSelected.size > 0 && (
        <div className="skills-batch-bar" style={{ marginTop: 8 }}>
          <span className="skills-batch-bar__count">
            {t('skills.selectedCount', { count: batchSelected.size })}
          </span>
          <button className="skills-btn skills-btn--small" onClick={() => handleBatchToggle(true)}>
            {t('skills.enableAll')}
          </button>
          <button className="skills-btn skills-btn--small" onClick={() => handleBatchToggle(false)}>
            {t('skills.disableAll')}
          </button>
        </div>
      )}

      {filtered.length === 0 ? (
        <div className="skills-empty">
          <div className="skills-empty__icon">📦</div>
          <div className="skills-empty__text">{t('skills.noSkills')}</div>
          <div className="skills-empty__hint">{t('skills.noSkillsHint')}</div>
        </div>
      ) : (
        <>
          {installed.length > 0 && (
            <>
              <div className="skills-group-label">{t('skills.installedViaIsland')}</div>
              <div className="skills-list">
                {installed.map(s => <SkillCard key={s.id} skill={s} onRefresh={loadAll} />)}
              </div>
            </>
          )}
          {discovered.length > 0 && (
            <>
              <div className="skills-group-label">{t('skills.localDiscovered')}</div>
              <div className="skills-list">
                {discovered.map(s => <SkillCard key={s.id} skill={s} onRefresh={loadAll} />)}
              </div>
            </>
          )}
        </>
      )}

      {installOpen && (
        <InstallDialog onClose={() => { setInstallOpen(false); loadAll() }} />
      )}
    </div>
  )
}
