import { create } from 'zustand'
import type { SwitchAppType, SwitchProvider, SwitchPrompt, ProviderPreset, ImportPreview, ImportResult } from '../services/switchApi'
import { switchApi } from '../services/switchApi'

interface SwitchState {
  activeAppType: SwitchAppType
  providers: SwitchProvider[]
  prompts: SwitchPrompt[]
  presets: ProviderPreset[]
  loading: boolean
  error: string | null
  ccSwitchDetected: boolean | null
  importPreview: ImportPreview | null
  importResult: ImportResult | null
  importing: boolean
}

interface SwitchActions {
  setActiveAppType: (appType: SwitchAppType) => void
  loadProviders: () => Promise<void>
  createProvider: (provider: SwitchProvider) => Promise<void>
  updateProvider: (provider: SwitchProvider) => Promise<void>
  deleteProvider: (id: string) => Promise<void>
  duplicateProvider: (id: string) => Promise<void>
  setCurrent: (id: string) => Promise<void>
  // Prompts
  loadPrompts: () => Promise<void>
  createPrompt: (prompt: SwitchPrompt) => Promise<void>
  updatePrompt: (prompt: SwitchPrompt) => Promise<void>
  deletePrompt: (id: string) => Promise<void>
  togglePrompt: (id: string) => Promise<void>
  applyPrompts: () => Promise<void>
  // Presets
  loadPresets: () => Promise<void>
  // Import
  detectCcSwitch: () => Promise<void>
  previewImport: () => Promise<void>
  runImport: () => Promise<void>
  clearAllData: () => Promise<void>
  clearError: () => void
}

export const useSwitchStore = create<SwitchState & SwitchActions>((set, get) => ({
  activeAppType: 'claude',
  providers: [],
  prompts: [],
  presets: [],
  loading: false,
  error: null,
  ccSwitchDetected: null,
  importPreview: null,
  importResult: null,
  importing: false,

  async clearAllData() {
    try {
      await switchApi.clearAllData()
      set({ providers: [], prompts: [], error: null })
      get().loadProviders()
      get().loadPrompts()
    } catch (e) {
      set({ error: `清除失败: ${e}` })
    }
  },

  clearError() {
    set({ error: null })
  },

  setActiveAppType(appType) {
    set({ activeAppType: appType, error: null })
    get().loadProviders()
    get().loadPrompts()
  },

  // --- Providers ---

  async loadProviders() {
    set({ loading: true, error: null })
    try {
      const providers = await switchApi.listProviders(get().activeAppType)
      set({ providers, loading: false })
    } catch (e) {
      set({ loading: false, error: `Failed to load providers: ${e}` })
    }
  },

  async createProvider(provider) {
    try {
      await switchApi.createProvider(provider)
      get().loadProviders()
    } catch (e) {
      set({ error: `Failed to create provider: ${e}` })
      throw e
    }
  },

  async updateProvider(provider) {
    try {
      await switchApi.updateProvider(provider)
      get().loadProviders()
    } catch (e) {
      set({ error: `Failed to update provider: ${e}` })
      throw e
    }
  },

  async deleteProvider(id) {
    try {
      await switchApi.deleteProvider(get().activeAppType, id)
      get().loadProviders()
    } catch (e) {
      set({ error: `Failed to delete provider: ${e}` })
    }
  },

  async duplicateProvider(id) {
    try {
      await switchApi.duplicateProvider(get().activeAppType, id)
      get().loadProviders()
    } catch (e) {
      set({ error: `Failed to duplicate provider: ${e}` })
    }
  },

  async setCurrent(id) {
    try {
      await switchApi.setCurrent(get().activeAppType, id)
      get().loadProviders()
    } catch (e) {
      set({ error: `Failed to set current provider: ${e}` })
    }
  },

  // --- Prompts ---

  async loadPrompts() {
    try {
      const prompts = await switchApi.listPrompts(get().activeAppType)
      set({ prompts })
    } catch (e) {
      set({ error: `Failed to load prompts: ${e}` })
    }
  },

  async createPrompt(prompt) {
    try {
      await switchApi.createPrompt(prompt)
      get().loadPrompts()
    } catch (e) {
      set({ error: `Failed to create prompt: ${e}` })
      throw e
    }
  },

  async updatePrompt(prompt) {
    try {
      await switchApi.updatePrompt(prompt)
      get().loadPrompts()
    } catch (e) {
      set({ error: `Failed to update prompt: ${e}` })
      throw e
    }
  },

  async deletePrompt(id) {
    try {
      await switchApi.deletePrompt(id, get().activeAppType)
      get().loadPrompts()
    } catch (e) {
      set({ error: `Failed to delete prompt: ${e}` })
    }
  },

  async togglePrompt(id) {
    try {
      await switchApi.togglePrompt(id, get().activeAppType)
      get().loadPrompts()
    } catch (e) {
      set({ error: `Failed to toggle prompt: ${e}` })
    }
  },

  async applyPrompts() {
    try {
      await switchApi.applyPrompts(get().activeAppType)
    } catch (e) {
      set({ error: `Failed to apply prompts: ${e}` })
    }
  },

  // --- Presets ---

  async loadPresets() {
    try {
      const presets = await switchApi.listPresets()
      set({ presets })
    } catch (e) {
      set({ error: `Failed to load presets: ${e}` })
    }
  },

  // --- Import ---

  async detectCcSwitch() {
    try {
      const detected = await switchApi.detectCcSwitch()
      set({ ccSwitchDetected: detected })
    } catch {
      set({ ccSwitchDetected: false })
    }
  },

  async previewImport() {
    try {
      const preview = await switchApi.importCcSwitchPreview()
      set({ importPreview: preview })
    } catch (e) {
      set({ error: `Failed to preview import: ${e}` })
    }
  },

  async runImport() {
    set({ importing: true, error: null })
    try {
      const result = await switchApi.importCcSwitch()
      set({ importResult: result, importing: false })
      get().loadProviders()
    } catch (e) {
      set({ importing: false, error: `Import failed: ${e}` })
    }
  },
}))
