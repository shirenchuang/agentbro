//! Tauri command wrappers for Skill Manager v2.

#![allow(dead_code)]
// `Ok(svc()?.method()?)` reads clearly at the command boundary; the wrapper
// shape is intentional and uniform across every command here.
#![allow(clippy::needless_question_mark)]

use crate::skills::v2::models::*;
use crate::skills::v2::service::{
    AdoptBatchItem, AdoptBatchResult, ClaimOrigin, DeleteSkillTargetDistributionsResult,
    DeleteUnmanagedAgentSkillsResult, Service, UpsertPackInput,
};
use crate::skills::v2::{diagnosis, snapshot};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

static MARKETPLACE_BATCH_CANCEL: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();

fn marketplace_batch_cancel_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    MARKETPLACE_BATCH_CANCEL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn svc() -> Result<Arc<Service>, String> {
    crate::skills::v2::service()
}

#[tauri::command(async)]
pub fn skill_manager_bootstrap() -> Result<(), String> {
    let svc = svc()?;
    svc.bootstrap()
}

#[tauri::command(async)]
pub fn skill_manager_init() -> Result<(), String> {
    let svc = svc()?;
    svc.init()
}

#[tauri::command(async)]
pub fn skill_manager_overview() -> Result<SkillManagerOverview, String> {
    Ok(svc()?.overview()?)
}

#[tauri::command(async)]
pub fn skill_pack_picker_data() -> Result<SkillPackPickerData, String> {
    svc()?.skill_pack_picker_data()
}

#[tauri::command(async)]
pub fn skill_manager_refresh() -> Result<(), String> {
    svc()?.refresh()
}

#[tauri::command(async)]
pub fn skill_manager_refresh_overview() -> Result<SkillManagerOverview, String> {
    Ok(svc()?.refresh_overview()?)
}

#[tauri::command(async)]
pub fn skill_manager_settings() -> Result<SkillManagerSettings, String> {
    Ok(svc()?.settings()?)
}

#[tauri::command(async)]
pub fn skill_manager_update_settings(
    update: SettingsUpdate,
) -> Result<SkillManagerSettings, String> {
    svc()?.update_settings(update)
}

#[tauri::command(async)]
pub fn list_center_skills_v2() -> Result<Vec<SkillSummary>, String> {
    Ok(svc()?.list_center_skills()?)
}

#[tauri::command(async)]
pub fn get_skill_detail_v2(skill_id: String) -> Result<SkillDetail, String> {
    Ok(svc()?.get_skill_detail(&skill_id)?)
}

// `(async)` runs the blocking body on a worker thread instead of the main
// thread, so a network skill download never freezes the webview and multiple
// installs can run concurrently.
#[tauri::command(async)]
pub fn preview_add_center_skill(
    input: AddCenterSkillInput,
) -> Result<AddCenterSkillPreview, String> {
    Ok(svc()?.preview_add_center_skill(input)?)
}

#[tauri::command(async)]
pub fn execute_add_center_skill(
    input: AddCenterSkillInput,
    decisions: Vec<AddCenterSkillDecision>,
) -> Result<AddCenterSkillResult, String> {
    Ok(svc()?.execute_add_center_skill(input, decisions)?)
}

#[tauri::command(async)]
pub fn execute_marketplace_skill_batch(
    app: AppHandle,
    job_id: String,
    repo_source: String,
    skills: Vec<MarketplaceBatchSkillInput>,
) -> Result<MarketplaceBatchInstallResult, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    marketplace_batch_cancel_registry()
        .lock()
        .map_err(|_| "Marketplace batch cancellation registry is unavailable".to_string())?
        .insert(job_id.clone(), cancel.clone());

    let result = run_marketplace_skill_batch(&app, &job_id, &repo_source, skills, &cancel);
    if let Ok(mut registry) = marketplace_batch_cancel_registry().lock() {
        registry.remove(&job_id);
    }
    result
}

fn run_marketplace_skill_batch(
    app: &AppHandle,
    job_id: &str,
    repo_source: &str,
    skills: Vec<MarketplaceBatchSkillInput>,
    cancel: &AtomicBool,
) -> Result<MarketplaceBatchInstallResult, String> {
    let total = skills.len();
    let emit_progress =
        |phase: &str, item_id: Option<String>, completed: usize, message: Option<String>| {
            let _ = app.emit(
                "marketplace-skill-batch-progress",
                MarketplaceBatchProgress {
                    job_id: job_id.to_string(),
                    phase: phase.to_string(),
                    item_id,
                    completed,
                    total,
                    message,
                },
            );
        };

    emit_progress("preparing", None, 0, None);
    let skill_ids = skills
        .iter()
        .map(|skill| skill.skill_id.clone())
        .collect::<Vec<_>>();
    let (repo_root, temp_root) =
        match crate::skills::installer::resolve_github_repo_skills_with_cancel(
            repo_source,
            &skill_ids,
            None,
            cancel,
        ) {
            Ok(resolved) => resolved,
            Err(_error) if cancel.load(Ordering::Relaxed) => {
                emit_progress("cancelled", None, 0, None);
                return Ok(MarketplaceBatchInstallResult {
                    items: Vec::new(),
                    cancelled: true,
                });
            }
            Err(error) => {
                emit_progress("source_failed", None, 0, Some(error.clone()));
                return Err(error);
            }
        };

    let install_result = (|| {
        let service = svc()?;
        let mut items = Vec::with_capacity(total);
        for skill in skills {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            emit_progress("installing", Some(skill.item_id.clone()), items.len(), None);
            let result =
                crate::skills::installer::locate_skillssh_skill_dir(&repo_root, &skill.skill_id)
                    .and_then(|source_dir| {
                        service.execute_add_center_skill(
                            AddCenterSkillInput {
                                source_path: source_dir.display().to_string(),
                                source_type: "skillssh".to_string(),
                                source_uri: Some(skill.source_uri.clone()),
                                imported_from_agent: None,
                                imported_from_path: None,
                                multi: Some(false),
                                import_mode: Some("copy".to_string()),
                            },
                            Vec::new(),
                        )
                    });
            match result {
                Ok(installed) => {
                    let installed_id = installed
                        .skill_ids
                        .first()
                        .or_else(|| installed.updated.first())
                        .cloned()
                        .unwrap_or_else(|| skill.skill_id.clone());
                    items.push(MarketplaceBatchItemResult {
                        item_id: skill.item_id.clone(),
                        skill_id: installed_id,
                        success: true,
                        error: None,
                    });
                    emit_progress("success", Some(skill.item_id), items.len(), None);
                }
                Err(error) => {
                    items.push(MarketplaceBatchItemResult {
                        item_id: skill.item_id.clone(),
                        skill_id: skill.skill_id,
                        success: false,
                        error: Some(error.clone()),
                    });
                    emit_progress("failed", Some(skill.item_id), items.len(), Some(error));
                }
            }
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        emit_progress(
            if cancelled { "cancelled" } else { "completed" },
            None,
            items.len(),
            None,
        );
        Ok::<MarketplaceBatchInstallResult, String>(MarketplaceBatchInstallResult {
            items,
            cancelled,
        })
    })();

    if let Some(root) = temp_root {
        let _ = std::fs::remove_dir_all(root);
    }
    install_result
}

#[tauri::command(async)]
pub fn cancel_marketplace_skill_batch(job_id: String) -> Result<bool, String> {
    let registry = marketplace_batch_cancel_registry()
        .lock()
        .map_err(|_| "Marketplace batch cancellation registry is unavailable".to_string())?;
    let Some(cancel) = registry.get(&job_id) else {
        return Ok(false);
    };
    cancel.store(true, Ordering::Relaxed);
    Ok(true)
}

#[tauri::command(async)]
pub fn preview_delete_center_skill(skill_id: String) -> Result<DeleteCenterSkillPreview, String> {
    Ok(svc()?.preview_delete_center_skill(&skill_id)?)
}

#[tauri::command(async)]
pub fn execute_delete_center_skill(skill_id: String, remove_linked: bool) -> Result<(), String> {
    svc()?.execute_delete_center_skill(&skill_id, remove_linked)
}

#[tauri::command(async)]
pub fn preview_delete_center_skills(
    skill_ids: Vec<String>,
) -> Result<DeleteCenterSkillPreview, String> {
    Ok(svc()?.preview_delete_center_skills(skill_ids)?)
}

#[tauri::command(async)]
pub fn execute_delete_center_skills(
    skill_ids: Vec<String>,
    remove_linked: bool,
) -> Result<(), String> {
    svc()?.execute_delete_center_skills(skill_ids, remove_linked)
}

#[tauri::command(async)]
pub fn preview_distribute_skill(
    skill_ids: Vec<String>,
    target_agents: Vec<String>,
    requested_mode: String,
) -> Result<DistributionPreview, String> {
    Ok(svc()?.preview_distribute_skill(skill_ids, target_agents, requested_mode)?)
}

#[tauri::command(async)]
pub fn execute_distribute_skill(
    preview: DistributionPreview,
) -> Result<DistributionPreview, String> {
    svc()?.execute_distribute_skill(preview, ClaimOrigin::Direct)
}

#[tauri::command(async)]
pub fn scan_agent_inventory(agent_id: String) -> Result<serde_json::Value, String> {
    let svc = svc()?;
    let result = svc.scan_agent_inventory_into_db(&agent_id)?;
    Ok(serde_json::json!({
        "agentId": agent_id,
        "managed": result.managed,
        "unmanaged": result.unmanaged,
        "readOnly": result.read_only,
        "includedShared": result.included_shared,
    }))
}

#[tauri::command(async)]
pub fn preview_adopt_agent_skill(
    agent_id: String,
    unmanaged_id: String,
) -> Result<crate::skills::v2::service::AdoptPreview, String> {
    Ok(svc()?.preview_adopt_agent_skill(&agent_id, &unmanaged_id)?)
}

#[tauri::command(async)]
pub fn execute_adopt_agent_skill(
    agent_id: String,
    unmanaged_id: String,
    option: String,
    renamed_id: Option<String>,
) -> Result<String, String> {
    svc()?.execute_adopt_agent_skill(&agent_id, &unmanaged_id, &option, renamed_id)
}

#[tauri::command(async)]
pub fn execute_adopt_agent_skills(items: Vec<AdoptBatchItem>) -> Result<AdoptBatchResult, String> {
    svc()?.execute_adopt_agent_skills(items)
}

#[tauri::command(async)]
pub fn takeover_center_agent_skills(
    agent_id: String,
    unmanaged_ids: Vec<String>,
) -> Result<AdoptBatchResult, String> {
    svc()?.takeover_center_agent_skills(&agent_id, unmanaged_ids)
}

#[tauri::command(async)]
pub fn delete_unmanaged_agent_skill(agent_id: String, unmanaged_id: String) -> Result<(), String> {
    svc()?.delete_unmanaged_agent_skill(&agent_id, &unmanaged_id)
}

#[tauri::command(async)]
pub fn delete_unmanaged_agent_skills(
    agent_id: String,
    unmanaged_ids: Vec<String>,
) -> Result<DeleteUnmanagedAgentSkillsResult, String> {
    svc()?.delete_unmanaged_agent_skills(&agent_id, unmanaged_ids)
}

#[tauri::command(async)]
pub fn preview_sync_copy_target(
    target_id: String,
) -> Result<crate::skills::v2::service::CopySyncPreview, String> {
    Ok(svc()?.preview_sync_copy_target(&target_id)?)
}

#[tauri::command(async)]
pub fn preview_copy_target_diff(
    target_id: String,
) -> Result<crate::skills::v2::service::CopyTargetDiffPreview, String> {
    Ok(svc()?.preview_copy_target_diff(&target_id)?)
}

#[tauri::command(async)]
pub fn execute_sync_copy_target(
    target_id: String,
    action: String,
) -> Result<crate::skills::v2::service::CopySyncPreview, String> {
    svc()?.execute_sync_copy_target(&target_id, &action)
}

#[tauri::command(async)]
pub fn delete_skill_target_distribution(target_id: String) -> Result<(), String> {
    svc()?.delete_skill_target_distribution(&target_id)
}

#[tauri::command(async)]
pub fn delete_skill_target_distributions(
    target_ids: Vec<String>,
) -> Result<DeleteSkillTargetDistributionsResult, String> {
    svc()?.delete_skill_target_distributions(target_ids)
}

#[tauri::command(async)]
pub fn list_skill_packs_v2() -> Result<Vec<SkillPackSummary>, String> {
    Ok(svc()?.list_skill_packs()?)
}

#[tauri::command(async)]
pub fn get_skill_pack_detail(pack_id: String) -> Result<SkillPackDetail, String> {
    Ok(svc()?.get_skill_pack_detail(&pack_id)?)
}

#[tauri::command(async)]
pub fn execute_upsert_skill_pack(
    pack: UpsertPackInput,
    defer_sync: Option<bool>,
) -> Result<SkillPackDetail, String> {
    if defer_sync.unwrap_or(false) {
        svc()?.upsert_skill_pack_deferred(pack)
    } else {
        svc()?.upsert_skill_pack(pack)
    }
}

#[tauri::command(async)]
pub fn preview_delete_skill_pack(pack_id: String) -> Result<DeleteSkillPackPreview, String> {
    svc()?.preview_delete_skill_pack(&pack_id)
}

#[tauri::command(async)]
pub fn execute_delete_skill_pack(pack_id: String) -> Result<(), String> {
    svc()?.delete_skill_pack(&pack_id)
}

#[tauri::command(async)]
pub fn preview_apply_skill_pack(
    pack_id: String,
    target_agents: Vec<String>,
    requested_mode: String,
) -> Result<DistributionPreview, String> {
    let svc = svc()?;
    let preview =
        svc.preview_distribute_skill(pack_members(&svc, &pack_id)?, target_agents, requested_mode)?;
    Ok(preview)
}

#[tauri::command(async)]
pub fn execute_apply_skill_pack(
    pack_id: String,
    target_agents: Vec<String>,
    requested_mode: String,
    blocker_decisions: Option<Vec<DistributionBlockerDecision>>,
) -> Result<DistributionPreview, String> {
    svc()?.apply_skill_pack_with_decisions(
        &pack_id,
        target_agents,
        requested_mode,
        blocker_decisions.unwrap_or_default(),
    )
}

#[tauri::command(async)]
pub fn execute_sync_skill_pack_to_agents(
    pack_id: String,
    target_agents: Option<Vec<String>>,
) -> Result<SkillPackSyncResult, String> {
    svc()?.sync_skill_pack_to_agents(&pack_id, target_agents.unwrap_or_default())
}

#[tauri::command(async)]
pub fn preview_remove_skill_pack_from_agent(
    pack_id: String,
    agent_id: String,
) -> Result<RemovePackFromAgentPreview, String> {
    svc()?.preview_remove_pack_from_agent(&pack_id, &agent_id)
}

#[tauri::command(async)]
pub fn execute_remove_skill_pack_from_agent(
    pack_id: String,
    agent_id: String,
) -> Result<crate::skills::v2::service::RevokeResult, String> {
    svc()?.remove_skill_pack_from_agent(&pack_id, &agent_id)
}

#[tauri::command(async)]
pub fn preview_remove_skill_from_pack(
    pack_id: String,
    skill_id: String,
) -> Result<RemoveSkillFromPackPreview, String> {
    svc()?.preview_remove_skill_from_pack(&pack_id, &skill_id)
}

#[tauri::command(async)]
pub fn execute_remove_skill_from_pack(
    pack_id: String,
    skill_id: String,
    also_remove_targets: bool,
) -> Result<(), String> {
    svc()?.remove_skill_from_pack(&pack_id, &skill_id, also_remove_targets)
}

#[tauri::command(async)]
pub fn preview_move_direct_skill_to_pack(
    target_id: String,
    pack_id: String,
) -> Result<MoveDirectSkillToPackPreview, String> {
    svc()?.preview_move_direct_skill_to_pack(&target_id, &pack_id)
}

#[tauri::command(async)]
pub fn execute_move_direct_skill_to_pack(
    target_id: String,
    pack_id: String,
    blocker_decisions: Option<Vec<DistributionBlockerDecision>>,
) -> Result<MoveDirectSkillToPackPreview, String> {
    svc()?.move_direct_skill_to_pack(&target_id, &pack_id, blocker_decisions.unwrap_or_default())
}

#[tauri::command(async)]
pub fn list_managed_agents_v2() -> Result<Vec<AgentSummary>, String> {
    Ok(svc()?.list_managed_agents()?)
}

#[tauri::command(async)]
pub fn get_agent_detail_v2(agent_id: String) -> Result<AgentDetail, String> {
    Ok(svc()?.get_agent_detail(&agent_id)?)
}

#[tauri::command(async)]
pub fn read_agent_config_file_v2(
    agent_id: String,
    path: String,
) -> Result<crate::skills::config_file_editor::AgentConfigDocument, String> {
    let service = svc()?;
    crate::skills::config_file_editor::read_agent_config_file(&service, &agent_id, &path)
}

#[tauri::command(async)]
pub fn write_agent_config_file_v2(
    agent_id: String,
    path: String,
    content: String,
    expected_revision: String,
) -> Result<crate::skills::config_file_editor::AgentConfigDocument, String> {
    let service = svc()?;
    crate::skills::config_file_editor::write_agent_config_file(
        &service,
        &agent_id,
        &path,
        &content,
        &expected_revision,
    )
}

#[tauri::command(async)]
pub fn list_plugin_inventory_v2(
    agent_id: String,
) -> Result<crate::skills::plugin_management::PluginInventory, String> {
    let svc = svc()?;
    crate::skills::plugin_management::list_plugins(&svc, &agent_id)
}

#[tauri::command(async)]
pub fn get_plugin_detail_v2(
    agent_id: String,
    plugin_id: String,
) -> Result<crate::skills::plugin_management::PluginDetail, String> {
    let svc = svc()?;
    crate::skills::plugin_management::get_plugin_detail(&svc, &agent_id, &plugin_id)
}

#[tauri::command(async)]
pub fn read_plugin_file_v2(
    agent_id: String,
    plugin_id: String,
    relative_path: String,
) -> Result<crate::skills::plugin_management::PluginFileContent, String> {
    let svc = svc()?;
    crate::skills::plugin_management::read_plugin_file(&svc, &agent_id, &plugin_id, &relative_path)
}

#[tauri::command(async)]
pub fn set_plugin_enabled_v2(
    agent_id: String,
    plugin_id: String,
    revision: String,
    enabled: bool,
) -> Result<crate::skills::plugin_management::PluginInventory, String> {
    let svc = svc()?;
    crate::skills::plugin_management::set_plugin_enabled(
        &svc, &agent_id, &plugin_id, &revision, enabled,
    )
}

#[tauri::command(async)]
pub fn list_unmanaged_v2() -> Result<Vec<UnmanagedItemDto>, String> {
    Ok(svc()?.list_unmanaged()?)
}

#[tauri::command(async)]
pub fn list_agent_skill_inventory_v2() -> Result<Vec<AgentSkillInventoryAgent>, String> {
    Ok(svc()?.list_agent_skill_inventory()?)
}

#[tauri::command(async)]
pub fn list_skill_projects_v2() -> Result<Vec<ProjectSummary>, String> {
    svc()?.list_projects()
}

#[tauri::command(async)]
pub fn add_skill_project_v2(root_path: String) -> Result<ProjectDetail, String> {
    svc()?.add_project(root_path)
}

#[tauri::command(async)]
pub fn remove_skill_project_v2(project_id: String) -> Result<(), String> {
    svc()?.remove_project(&project_id)
}

#[tauri::command(async)]
pub fn get_skill_project_detail_v2(project_id: String) -> Result<ProjectDetail, String> {
    svc()?.get_project_detail(&project_id)
}

#[tauri::command(async)]
pub fn scan_skill_project_v2(project_id: String) -> Result<ProjectDetail, String> {
    svc()?.scan_project(&project_id)
}

#[tauri::command(async)]
pub fn install_center_skills_to_project_v2(
    project_id: String,
    agent_id: String,
    skill_ids: Vec<String>,
    requested_mode: String,
) -> Result<ProjectDetail, String> {
    svc()?.install_center_skills_to_project(&project_id, &agent_id, skill_ids, requested_mode)
}

#[tauri::command(async)]
pub fn install_skill_pack_to_project_v2(
    project_id: String,
    agent_id: String,
    pack_id: String,
    requested_mode: String,
) -> Result<ProjectDetail, String> {
    svc()?.install_skill_pack_to_project(&project_id, &agent_id, &pack_id, requested_mode)
}

#[tauri::command(async)]
pub fn run_skill_manager_diagnosis() -> Result<Vec<DiagnosisIssue>, String> {
    let svc = svc()?;
    svc.refresh()?;
    diagnosis::run(&svc)
}

#[tauri::command(async)]
pub fn list_diagnosis_issues() -> Result<Vec<DiagnosisIssue>, String> {
    svc()?.list_current_diagnosis_issues()
}

#[tauri::command(async)]
pub fn preview_fix_diagnosis_issue(
    issue_type: String,
    entity_id: String,
) -> Result<serde_json::Value, String> {
    let svc = svc()?;
    let issues = diagnosis::run(&svc)?;
    let issue = issues
        .into_iter()
        .find(|i| i.issue_type == issue_type && i.entity_id.as_deref() == Some(&entity_id));
    Ok(serde_json::json!({
        "issue": issue,
        "destructive": issue.as_ref().map(|i| i.fix_kind == "confirm").unwrap_or(false),
    }))
}

#[tauri::command(async)]
pub fn execute_fix_diagnosis_issue(issue_type: String, entity_id: String) -> Result<(), String> {
    let svc = svc()?;
    diagnosis::execute_fix(&svc, &issue_type, &entity_id)
}

#[tauri::command(async)]
pub fn execute_safe_fixes() -> Result<usize, String> {
    let svc = svc()?;
    diagnosis::execute_safe_fixes(&svc)
}

#[tauri::command(async)]
pub fn skill_manager_export_snapshot() -> Result<String, String> {
    let svc = svc()?;
    snapshot::export_to_file(&svc)
}

#[tauri::command(async)]
pub fn skill_manager_get_snapshot() -> Result<Snapshot, String> {
    let svc = svc()?;
    snapshot::export(&svc)
}

#[tauri::command(async)]
pub fn open_skill_path(path: String) -> Result<(), String> {
    crate::skills::v2::fsutil::open_path(&path)
}

#[tauri::command(async)]
pub fn reveal_skill_path(path: String) -> Result<(), String> {
    crate::skills::v2::fsutil::reveal_path(&path)
}

fn pack_members(svc: &Service, pack_id: &str) -> Result<Vec<String>, String> {
    Ok(svc
        .get_skill_pack_detail(pack_id)?
        .members
        .into_iter()
        .map(|m| m.skill_id)
        .collect())
}
