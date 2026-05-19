use tauri::State;

use super::app_type::SwitchAppType;
use super::health::{self, ProviderHealth, SpeedTestResult};
use super::live_writer;
use super::migration;
use super::presets::{self, ProviderPreset};
use super::pricing::{self, ModelPricing};
use super::prompts::{self, SwitchPrompt};
use super::providers::{self, SwitchProvider};
use super::usage::{self, DailyCost, ModelUsage, ProviderUsage, UsageSummary};
use crate::commands::AppState;

#[tauri::command]
pub async fn switch_list_providers(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<Vec<SwitchProvider>, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    providers::list_providers(&state.switch_db, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_create_provider(
    state: State<'_, AppState>,
    provider: SwitchProvider,
) -> Result<(), String> {
    providers::create_provider(&state.switch_db, &provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_update_provider(
    state: State<'_, AppState>,
    provider: SwitchProvider,
) -> Result<(), String> {
    providers::update_provider(&state.switch_db, &provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_delete_provider(
    state: State<'_, AppState>,
    app_type: String,
    id: String,
) -> Result<(), String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    providers::delete_provider(&state.switch_db, &at, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_set_current(
    state: State<'_, AppState>,
    app_type: String,
    id: String,
) -> Result<(), String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    providers::set_current(&state.switch_db, &at, &id).map_err(|e| e.to_string())?;
    if let Some(provider) =
        providers::get_provider(&state.switch_db, &at, &id).map_err(|e| e.to_string())?
    {
        live_writer::write_provider_to_agent_config(&provider).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn switch_get_current(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<Option<SwitchProvider>, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    providers::get_current(&state.switch_db, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_duplicate_provider(
    state: State<'_, AppState>,
    app_type: String,
    id: String,
) -> Result<SwitchProvider, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    providers::duplicate_provider(&state.switch_db, &at, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_detect_cc_switch() -> Result<bool, String> {
    Ok(migration::detect_cc_switch())
}

#[tauri::command]
pub async fn switch_import_cc_switch_preview() -> Result<migration::ImportPreview, String> {
    migration::import_preview().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_import_cc_switch(
    state: State<'_, AppState>,
) -> Result<migration::ImportResult, String> {
    migration::import_from_cc_switch(&state.switch_db).map_err(|e| e.to_string())
}

// --- Debug commands ---

#[tauri::command]
pub async fn switch_clear_all_data(state: State<'_, AppState>) -> Result<(), String> {
    state.switch_db.clear_all_data().map_err(|e| e.to_string())
}

// --- Prompt commands ---

#[tauri::command]
pub async fn switch_list_prompts(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<Vec<SwitchPrompt>, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    prompts::list_prompts(&state.switch_db, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_create_prompt(
    state: State<'_, AppState>,
    prompt: SwitchPrompt,
) -> Result<(), String> {
    prompts::create_prompt(&state.switch_db, &prompt).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_update_prompt(
    state: State<'_, AppState>,
    prompt: SwitchPrompt,
) -> Result<(), String> {
    prompts::update_prompt(&state.switch_db, &prompt).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_delete_prompt(
    state: State<'_, AppState>,
    id: String,
    app_type: String,
) -> Result<(), String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    prompts::delete_prompt(&state.switch_db, &id, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_toggle_prompt(
    state: State<'_, AppState>,
    id: String,
    app_type: String,
) -> Result<(), String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    prompts::toggle_prompt(&state.switch_db, &id, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_apply_prompts(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<(), String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    prompts::apply_prompts(&state.switch_db, &at).map_err(|e| e.to_string())
}

// --- Preset commands ---

#[tauri::command]
pub async fn switch_list_presets() -> Result<Vec<ProviderPreset>, String> {
    Ok(presets::list_presets())
}

// --- Usage commands ---

#[tauri::command]
pub async fn switch_get_usage_summary(
    state: State<'_, AppState>,
    app_type: String,
    days: u32,
) -> Result<UsageSummary, String> {
    usage::get_usage_summary(&state.switch_db, &app_type, days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_get_usage_by_provider(
    state: State<'_, AppState>,
    app_type: String,
    days: u32,
) -> Result<Vec<ProviderUsage>, String> {
    usage::get_usage_by_provider(&state.switch_db, &app_type, days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_get_usage_by_model(
    state: State<'_, AppState>,
    app_type: String,
    days: u32,
) -> Result<Vec<ModelUsage>, String> {
    usage::get_usage_by_model(&state.switch_db, &app_type, days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_get_daily_cost(
    state: State<'_, AppState>,
    app_type: String,
    days: u32,
) -> Result<Vec<DailyCost>, String> {
    usage::get_daily_cost(&state.switch_db, &app_type, days).map_err(|e| e.to_string())
}

// --- Pricing commands ---

#[tauri::command]
pub async fn switch_list_model_pricing() -> Result<Vec<ModelPricing>, String> {
    Ok(pricing::default_pricing())
}

// --- Health commands ---

#[tauri::command]
pub async fn switch_get_provider_health(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<Vec<ProviderHealth>, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    health::get_provider_health(&state.switch_db, &at).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_speed_test(
    state: State<'_, AppState>,
    app_type: String,
    provider_id: String,
) -> Result<SpeedTestResult, String> {
    let at: SwitchAppType = app_type.parse().map_err(|e: String| e)?;
    health::speed_test(&state.switch_db, &at, &provider_id)
        .await
        .map_err(|e| e.to_string())
}
