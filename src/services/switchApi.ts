import { invoke } from '@tauri-apps/api/core'

export type SwitchAppType = 'claude' | 'codex' | 'gemini' | 'opencode' | 'hermes'

export interface SwitchProvider {
  id: string
  app_type: string
  name: string
  settings_config: Record<string, unknown>
  website_url?: string | null
  category?: string | null
  icon?: string | null
  icon_color?: string | null
  meta: Record<string, unknown>
  is_current: boolean
  in_failover_queue: boolean
  created_at?: number | null
  sort_index?: number | null
  notes?: string | null
}

export interface SwitchPrompt {
  id: string
  app_type: string
  name: string
  content: string
  description?: string | null
  enabled: boolean
  created_at?: number | null
  updated_at?: number | null
  sort_index?: number | null
}

export interface ProviderPreset {
  id: string
  name: string
  category: string
  icon: string
  icon_color: string
  website_url: string
  settings_template: Record<string, unknown>
  description: string
  supported_apps: string[]
}

export interface UsageSummary {
  total_requests: number
  total_input_tokens: number
  total_output_tokens: number
  total_cost_usd: number
}

export interface ProviderUsage {
  provider_id: string
  request_count: number
  input_tokens: number
  output_tokens: number
  cost_usd: number
}

export interface ModelUsage {
  model_id: string
  request_count: number
  input_tokens: number
  output_tokens: number
  cost_usd: number
}

export interface DailyCost {
  date: string
  cost_usd: number
  request_count: number
}

export interface ModelPricing {
  model_id: string
  display_name: string
  provider: string
  input_per_mtok: number
  output_per_mtok: number
  cache_read_per_mtok?: number | null
  cache_write_per_mtok?: number | null
}

export interface SpeedTestResult {
  provider_id: string
  latency_ms: number
  success: boolean
  error?: string | null
  status_code?: number | null
}

export interface ProviderHealthInfo {
  provider_id: string
  provider_name: string
  is_current: boolean
  has_api_key: boolean
  base_url: string
  last_test?: SpeedTestResult | null
}

export interface ImportPreview {
  providers: number
  provider_endpoints: number
  mcp_servers: number
  prompts: number
  skills: number
}

export interface ImportResult {
  providers_imported: number
  provider_endpoints_imported: number
  mcp_servers_imported: number
  prompts_imported: number
  skills_imported: number
}

export const switchApi = {
  // --- Providers ---
  listProviders(appType: SwitchAppType): Promise<SwitchProvider[]> {
    return invoke('switch_list_providers', { appType })
  },

  createProvider(provider: SwitchProvider): Promise<void> {
    return invoke('switch_create_provider', { provider })
  },

  updateProvider(provider: SwitchProvider): Promise<void> {
    return invoke('switch_update_provider', { provider })
  },

  deleteProvider(appType: SwitchAppType, id: string): Promise<void> {
    return invoke('switch_delete_provider', { appType, id })
  },

  duplicateProvider(appType: SwitchAppType, id: string): Promise<SwitchProvider> {
    return invoke('switch_duplicate_provider', { appType, id })
  },

  setCurrent(appType: SwitchAppType, id: string): Promise<void> {
    return invoke('switch_set_current', { appType, id })
  },

  getCurrent(appType: SwitchAppType): Promise<SwitchProvider | null> {
    return invoke('switch_get_current', { appType })
  },

  // --- Prompts ---
  listPrompts(appType: SwitchAppType): Promise<SwitchPrompt[]> {
    return invoke('switch_list_prompts', { appType })
  },

  createPrompt(prompt: SwitchPrompt): Promise<void> {
    return invoke('switch_create_prompt', { prompt })
  },

  updatePrompt(prompt: SwitchPrompt): Promise<void> {
    return invoke('switch_update_prompt', { prompt })
  },

  deletePrompt(id: string, appType: SwitchAppType): Promise<void> {
    return invoke('switch_delete_prompt', { id, appType })
  },

  togglePrompt(id: string, appType: SwitchAppType): Promise<void> {
    return invoke('switch_toggle_prompt', { id, appType })
  },

  applyPrompts(appType: SwitchAppType): Promise<void> {
    return invoke('switch_apply_prompts', { appType })
  },

  // --- Presets ---
  listPresets(): Promise<ProviderPreset[]> {
    return invoke('switch_list_presets')
  },

  // --- Import ---
  detectCcSwitch(): Promise<boolean> {
    return invoke('switch_detect_cc_switch')
  },

  importCcSwitchPreview(): Promise<ImportPreview> {
    return invoke('switch_import_cc_switch_preview')
  },

  importCcSwitch(): Promise<ImportResult> {
    return invoke('switch_import_cc_switch')
  },

  // --- Debug ---
  clearAllData(): Promise<void> {
    return invoke('switch_clear_all_data')
  },

  // --- Usage ---
  getUsageSummary(appType: SwitchAppType, days: number): Promise<UsageSummary> {
    return invoke('switch_get_usage_summary', { appType, days })
  },

  getUsageByProvider(appType: SwitchAppType, days: number): Promise<ProviderUsage[]> {
    return invoke('switch_get_usage_by_provider', { appType, days })
  },

  getUsageByModel(appType: SwitchAppType, days: number): Promise<ModelUsage[]> {
    return invoke('switch_get_usage_by_model', { appType, days })
  },

  getDailyCost(appType: SwitchAppType, days: number): Promise<DailyCost[]> {
    return invoke('switch_get_daily_cost', { appType, days })
  },

  // --- Pricing ---
  listModelPricing(): Promise<ModelPricing[]> {
    return invoke('switch_list_model_pricing')
  },

  // --- Health ---
  getProviderHealth(appType: SwitchAppType): Promise<ProviderHealthInfo[]> {
    return invoke('switch_get_provider_health', { appType })
  },

  speedTest(appType: SwitchAppType, providerId: string): Promise<SpeedTestResult> {
    return invoke('switch_speed_test', { appType, providerId })
  },
}
