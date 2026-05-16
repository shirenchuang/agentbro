import { create } from 'zustand'
import type { FileTreeNode, ObsidianVault, ScanRoot, ScannedSkill, SkillCollection, SkillPack, SyncConfig } from '../services/skillApi'
import { skillApi } from '../services/skillApi'

interface SkillRegistryMetadata {
  sources: Record<string, { origin: string }>
  packs: SkillPack[]
  collections?: SkillCollection[]
  scanRoots?: ScanRoot[]
  sync: SyncConfig | null
}

interface SkillState {
  skills: ScannedSkill[]
  packs: SkillPack[]
  collections: SkillCollection[]
  scanRoots: ScanRoot[]
  obsidianVaults: ObsidianVault[]
  syncConfig: SyncConfig | null
  loading: boolean
  scanning: boolean
  activeTab: 'skills' | 'central' | 'plugins' | 'collections' | 'packs' | 'discover' | 'obsidian' | 'market' | 'sync'
  selectedSkillId: string | null
  detailOpen: boolean
  fileTree: FileTreeNode | null
  searchQuery: string
  typeFilter: 'all' | 'skill' | 'plugin' | 'mcp'
  agentFilter: string
  batchMode: boolean
  batchSelected: Set<string>
}

interface SkillActions {
  loadAll: () => Promise<void>
  loadCollections: () => Promise<void>
  loadScanRoots: () => Promise<void>
  loadObsidianVaults: () => Promise<void>
  setTab: (tab: SkillState['activeTab']) => void
  selectSkill: (id: string) => void
  closeDetail: () => void
  loadFileTree: (skillPath: string) => Promise<void>
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
  collections: [],
  scanRoots: [],
  obsidianVaults: [],
  syncConfig: null,
  loading: false,
  scanning: false,
  activeTab: 'skills',
  selectedSkillId: null,
  detailOpen: false,
  fileTree: null,
  searchQuery: '',
  typeFilter: 'all',
  agentFilter: 'all',
  batchMode: false,
  batchSelected: new Set(),

  loadAll: async () => {
    set({ scanning: true })
    try {
      const [scanResult, meta]: [Record<string, ScannedSkill[]>, SkillRegistryMetadata] = await Promise.all([
        skillApi.scanAll(),
        skillApi.getMetadata(),
      ])

      const merged = new Map<string, ScannedSkill>()
      for (const [, agentSkills] of Object.entries(scanResult)) {
        for (const skill of agentSkills) {
          if (merged.has(skill.id)) {
            const existing = merged.get(skill.id)!
            const seenAgents = new Set(existing.agents.map(agentKey))
            for (const agent of skill.agents) {
              const key = agentKey(agent)
              if (!seenAgents.has(key)) {
                existing.agents.push(agent)
                seenAgents.add(key)
              }
            }
          } else {
            const source = meta.sources[skill.id] ? 'island' as const : skill.source
            const originUrl = meta.sources[skill.id]?.origin ?? null
            merged.set(skill.id, {
              ...skill,
              agents: uniqueAgents(skill.agents),
              source,
              originUrl,
              hasUpdate: skill.hasUpdate || Boolean(originUrl),
            })
          }
        }
      }

      for (const [id, entry] of Object.entries(meta.sources)) {
        if (!merged.has(id)) {
          merged.set(id, {
            id,
            name: id,
            description: '',
            skillType: 'skill',
            icon: null,
            source: 'island',
            originUrl: entry.origin ?? null,
            hasUpdate: false,
            filePath: '',
            fileSize: 0,
            modifiedAt: 0,
            agents: [],
            frontmatter: {},
          })
        }
      }

      set({
        skills: Array.from(merged.values()),
        packs: meta.packs,
        collections: meta.collections ?? [],
        scanRoots: meta.scanRoots ?? [],
        syncConfig: meta.sync,
        scanning: false,
      })
    } catch (e) {
      console.error('Failed to load skills:', e)
      set({ scanning: false })
    }
  },

  loadCollections: async () => {
    try {
      const collections = await skillApi.listCollections()
      set({ collections })
    } catch (e) {
      console.error('Failed to load collections:', e)
    }
  },

  loadScanRoots: async () => {
    try {
      const scanRoots = await skillApi.getScanRoots()
      set({ scanRoots })
    } catch (e) {
      console.error('Failed to load scan roots:', e)
    }
  },

  loadObsidianVaults: async () => {
    try {
      const obsidianVaults = await skillApi.getObsidianVaults()
      set({ obsidianVaults })
    } catch (e) {
      console.error('Failed to load Obsidian vaults:', e)
    }
  },

  setTab: (tab) => set({ activeTab: tab }),

  selectSkill: (id) => {
    set({ selectedSkillId: id, detailOpen: true, fileTree: null })
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

function agentKey(agent: ScannedSkill['agents'][number]) {
  return `${agent.agent}:${agent.installPath}:${agent.linkTarget ?? ''}`
}

function uniqueAgents(agents: ScannedSkill['agents']) {
  const seen = new Set<string>()
  return agents.filter((agent) => {
    const key = agentKey(agent)
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}
