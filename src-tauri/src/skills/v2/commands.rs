//! Tauri command wrappers for Skill Manager v2.

#![allow(dead_code)]
// `Ok(svc()?.method()?)` reads clearly at the command boundary; the wrapper
// shape is intentional and uniform across every command here.
#![allow(clippy::needless_question_mark)]

use crate::skills::v2::models::*;
use crate::skills::v2::service::{ClaimOrigin, Service, UpsertPackInput};
use crate::skills::v2::{diagnosis, snapshot};
use std::sync::Arc;

fn svc() -> Result<Arc<Service>, String> {
    crate::skills::v2::service()
}

#[tauri::command]
pub fn skill_manager_bootstrap() -> Result<(), String> {
    let svc = svc()?;
    svc.bootstrap()
}

#[tauri::command]
pub fn skill_manager_init() -> Result<(), String> {
    let svc = svc()?;
    svc.init()
}

#[tauri::command]
pub fn skill_manager_overview() -> Result<SkillManagerOverview, String> {
    Ok(svc()?.overview()?)
}

#[tauri::command]
pub fn skill_manager_refresh() -> Result<(), String> {
    svc()?.refresh()
}

#[tauri::command]
pub fn skill_manager_settings() -> Result<SkillManagerSettings, String> {
    Ok(svc()?.settings()?)
}

#[tauri::command]
pub fn skill_manager_update_settings(
    update: SettingsUpdate,
) -> Result<SkillManagerSettings, String> {
    svc()?.update_settings(update)
}

#[tauri::command]
pub fn list_center_skills_v2() -> Result<Vec<SkillSummary>, String> {
    Ok(svc()?.list_center_skills()?)
}

#[tauri::command]
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

#[tauri::command]
pub fn preview_delete_center_skill(skill_id: String) -> Result<DeleteCenterSkillPreview, String> {
    Ok(svc()?.preview_delete_center_skill(&skill_id)?)
}

#[tauri::command]
pub fn execute_delete_center_skill(skill_id: String, remove_linked: bool) -> Result<(), String> {
    svc()?.execute_delete_center_skill(&skill_id, remove_linked)
}

#[tauri::command]
pub fn preview_distribute_skill(
    skill_ids: Vec<String>,
    target_agents: Vec<String>,
    requested_mode: String,
) -> Result<DistributionPreview, String> {
    Ok(svc()?.preview_distribute_skill(skill_ids, target_agents, requested_mode)?)
}

#[tauri::command]
pub fn execute_distribute_skill(
    preview: DistributionPreview,
) -> Result<DistributionPreview, String> {
    svc()?.execute_distribute_skill(preview, ClaimOrigin::Direct)
}

#[tauri::command]
pub fn scan_agent_inventory(agent_id: String) -> Result<serde_json::Value, String> {
    let svc = svc()?;
    let result = svc.scan_one_agent_into_db(&agent_id)?;
    Ok(serde_json::json!({
        "agentId": agent_id,
        "managed": result.managed,
        "unmanaged": result.unmanaged,
    }))
}

#[tauri::command]
pub fn preview_adopt_agent_skill(
    agent_id: String,
    unmanaged_id: String,
) -> Result<crate::skills::v2::service::AdoptPreview, String> {
    Ok(svc()?.preview_adopt_agent_skill(&agent_id, &unmanaged_id)?)
}

#[tauri::command]
pub fn execute_adopt_agent_skill(
    agent_id: String,
    unmanaged_id: String,
    option: String,
    renamed_id: Option<String>,
) -> Result<String, String> {
    svc()?.execute_adopt_agent_skill(&agent_id, &unmanaged_id, &option, renamed_id)
}

#[tauri::command]
pub fn preview_sync_copy_target(
    target_id: String,
) -> Result<crate::skills::v2::service::CopySyncPreview, String> {
    Ok(svc()?.preview_sync_copy_target(&target_id)?)
}

#[tauri::command]
pub fn execute_sync_copy_target(
    target_id: String,
    action: String,
) -> Result<crate::skills::v2::service::CopySyncPreview, String> {
    svc()?.execute_sync_copy_target(&target_id, &action)
}

#[tauri::command]
pub fn list_skill_packs_v2() -> Result<Vec<SkillPackSummary>, String> {
    Ok(svc()?.list_skill_packs()?)
}

#[tauri::command]
pub fn get_skill_pack_detail(pack_id: String) -> Result<SkillPackDetail, String> {
    Ok(svc()?.get_skill_pack_detail(&pack_id)?)
}

#[tauri::command]
pub fn execute_upsert_skill_pack(pack: UpsertPackInput) -> Result<SkillPackDetail, String> {
    svc()?.upsert_skill_pack(pack)
}

#[tauri::command]
pub fn preview_delete_skill_pack(pack_id: String) -> Result<DeleteSkillPackPreview, String> {
    svc()?.preview_delete_skill_pack(&pack_id)
}

#[tauri::command]
pub fn execute_delete_skill_pack(pack_id: String) -> Result<(), String> {
    svc()?.delete_skill_pack(&pack_id)
}

#[tauri::command]
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

#[tauri::command]
pub fn execute_apply_skill_pack(
    pack_id: String,
    target_agents: Vec<String>,
    requested_mode: String,
) -> Result<DistributionPreview, String> {
    svc()?.apply_skill_pack(&pack_id, target_agents, requested_mode)
}

#[tauri::command]
pub fn preview_remove_skill_pack_from_agent(
    pack_id: String,
    agent_id: String,
) -> Result<RemovePackFromAgentPreview, String> {
    svc()?.preview_remove_pack_from_agent(&pack_id, &agent_id)
}

#[tauri::command]
pub fn execute_remove_skill_pack_from_agent(
    pack_id: String,
    agent_id: String,
) -> Result<crate::skills::v2::service::RevokeResult, String> {
    svc()?.remove_skill_pack_from_agent(&pack_id, &agent_id)
}

#[tauri::command]
pub fn preview_remove_skill_from_pack(
    pack_id: String,
    skill_id: String,
) -> Result<RemoveSkillFromPackPreview, String> {
    svc()?.preview_remove_skill_from_pack(&pack_id, &skill_id)
}

#[tauri::command]
pub fn execute_remove_skill_from_pack(
    pack_id: String,
    skill_id: String,
    also_remove_targets: bool,
) -> Result<(), String> {
    svc()?.remove_skill_from_pack(&pack_id, &skill_id, also_remove_targets)
}

#[tauri::command]
pub fn list_managed_agents_v2() -> Result<Vec<AgentSummary>, String> {
    Ok(svc()?.list_managed_agents()?)
}

#[tauri::command]
pub fn get_agent_detail_v2(agent_id: String) -> Result<AgentDetail, String> {
    Ok(svc()?.get_agent_detail(&agent_id)?)
}

#[tauri::command]
pub fn list_unmanaged_v2() -> Result<Vec<UnmanagedItemDto>, String> {
    Ok(svc()?.list_unmanaged()?)
}

#[tauri::command]
pub fn list_agent_skill_inventory_v2() -> Result<Vec<AgentSkillInventoryAgent>, String> {
    Ok(svc()?.list_agent_skill_inventory()?)
}

#[tauri::command]
pub fn run_skill_manager_diagnosis() -> Result<Vec<DiagnosisIssue>, String> {
    let svc = svc()?;
    svc.refresh()?;
    diagnosis::run(&svc)
}

#[tauri::command]
pub fn list_diagnosis_issues() -> Result<Vec<DiagnosisIssue>, String> {
    svc()?.list_current_diagnosis_issues()
}

#[tauri::command]
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

#[tauri::command]
pub fn execute_fix_diagnosis_issue(issue_type: String, entity_id: String) -> Result<(), String> {
    let svc = svc()?;
    diagnosis::execute_fix(&svc, &issue_type, &entity_id)
}

#[tauri::command]
pub fn execute_safe_fixes() -> Result<usize, String> {
    let svc = svc()?;
    diagnosis::execute_safe_fixes(&svc)
}

#[tauri::command]
pub fn skill_manager_export_snapshot() -> Result<String, String> {
    let svc = svc()?;
    snapshot::export_to_file(&svc)
}

#[tauri::command]
pub fn skill_manager_get_snapshot() -> Result<Snapshot, String> {
    let svc = svc()?;
    snapshot::export(&svc)
}

#[tauri::command]
pub fn open_skill_path(path: String) -> Result<(), String> {
    crate::skills::v2::fsutil::open_path(&path)
}

#[tauri::command]
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
