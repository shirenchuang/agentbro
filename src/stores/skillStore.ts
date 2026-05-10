import { create } from 'zustand'
import type { ScannedSkill, SkillPack, SyncConfig, FileTreeNode } from '../services/skillApi'
import { skillApi } from '../services/skillApi'

interface SkillState {
  skills: ScannedSkill[]
  packs: SkillPack[]
  syncConfig: SyncConfig | null
  loading: boolean
  scanning: boolean
  activeTab: 'skills' | 'packs' | 'sync'
  selectedSkillId: string | null
  detailOpen: boolean
  fileTree: FileTreeNode | null
  fileContent: string
  selectedFilePath: string
  searchQuery: string
  typeFilter: 'all' | 'skill' | 'mcp'
  agentFilter: string
  batchMode: boolean
  batchSelected: Set<string>
}

interface SkillActions {
  loadAll: () => Promise<void>
  setTab: (tab: SkillState['activeTab']) => void
  selectSkill: (id: string) => void
  closeDetail: () => void
  loadFileTree: (skillPath: string) => Promise<void>
  loadFileContent: (filePath: string) => Promise<void>
  setSearchQuery: (q: string) => void
  setTypeFilter: (f: SkillState['typeFilter']) => void
  setAgentFilter: (a: string) => void
  toggleBatchMode: () => void
  toggleBatchItem: (id: string) => void
  clearBatch: () => void
}

export const useSkillStore = create<SkillState & SkillActions>()((set, get) => ({
  skills: [],
  packs: [],
  syncConfig: null,
  loading: false,
  scanning: false,
  activeTab: 'skills',
  selectedSkillId: null,
  detailOpen: false,
  fileTree: null,
  fileContent: '',
  selectedFilePath: '',
  searchQuery: '',
  typeFilter: 'all',
  agentFilter: 'all',
  batchMode: false,
  batchSelected: new Set(),

  loadAll: async () => {
    set({ scanning: true })
    try {
      const [scanResult, meta] = await Promise.all([
        skillApi.scanAll(),
        skillApi.getMetadata(),
      ])

      const merged = new Map<string, ScannedSkill>()
      for (const [, agentSkills] of Object.entries(scanResult)) {
        for (const skill of agentSkills) {
          if (merged.has(skill.id)) {
            const existing = merged.get(skill.id)!
            existing.agents.push(...skill.agents)
          } else {
            const source = meta.sources[skill.id] ? 'island' as const : skill.source
            merged.set(skill.id, { ...skill, source, originUrl: meta.sources[skill.id]?.origin ?? null })
          }
        }
      }

      set({
        skills: Array.from(merged.values()),
        packs: meta.packs,
        syncConfig: meta.sync,
        scanning: false,
      })
    } catch (e) {
      console.error('Failed to load skills:', e)
      set({ scanning: false })
    }
  },

  setTab: (tab) => set({ activeTab: tab }),

  selectSkill: (id) => {
    set({ selectedSkillId: id, detailOpen: true, fileTree: null, fileContent: '', selectedFilePath: '' })
    const skill = get().skills.find(s => s.id === id)
    if (skill) {
      get().loadFileTree(skill.filePath)
    }
  },

  closeDetail: () => set({ detailOpen: false, selectedSkillId: null }),

  loadFileTree: async (skillPath) => {
    try {
      const tree = await skillApi.readFileTree(skillPath)
      set({ fileTree: tree })
    } catch { /* ignore */ }
  },

  loadFileContent: async (filePath) => {
    try {
      const content = await skillApi.readFileContent(filePath)
      set({ fileContent: content, selectedFilePath: filePath })
    } catch { /* ignore */ }
  },

  setSearchQuery: (q) => set({ searchQuery: q }),
  setTypeFilter: (f) => set({ typeFilter: f }),
  setAgentFilter: (a) => set({ agentFilter: a }),
  toggleBatchMode: () => set(s => ({ batchMode: !s.batchMode, batchSelected: new Set() })),
  toggleBatchItem: (id) => set(s => {
    const next = new Set(s.batchSelected)
    if (next.has(id)) next.delete(id); else next.add(id)
    return { batchSelected: next }
  }),
  clearBatch: () => set({ batchSelected: new Set() }),
}))
