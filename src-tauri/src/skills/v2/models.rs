//! Skill Manager v2 — data transfer objects, enums and DB row models.
//!
//! All structs use `#[serde(rename_all = "camelCase")]` so the DTO shapes that
//! cross the Tauri boundary match the TypeScript types in `skillApi.ts`.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManagerSettings {
    #[serde(default = "default_center_path")]
    pub center_path: String,
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
    /// "link" | "copy"
    #[serde(default = "default_mode")]
    pub default_distribute_mode: String,
    /// "ask" | "copy" — behaviour when a symlink cannot be created
    #[serde(default = "default_link_fail")]
    pub link_fail_policy: String,
    #[serde(default = "default_true")]
    pub startup_scan: bool,
    #[serde(default = "default_true")]
    pub show_unmanaged: bool,
}

fn default_center_path() -> String {
    crate::skills::v2::fsutil::default_center_path()
        .to_string_lossy()
        .to_string()
}
fn default_sqlite_path() -> String {
    crate::skills::v2::fsutil::default_sqlite_path()
        .to_string_lossy()
        .to_string()
}
fn default_mode() -> String {
    "link".to_string()
}
fn default_link_fail() -> String {
    "ask".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for SkillManagerSettings {
    fn default() -> Self {
        Self {
            center_path: default_center_path(),
            sqlite_path: default_sqlite_path(),
            default_distribute_mode: default_mode(),
            link_fail_policy: default_link_fail(),
            startup_scan: true,
            show_unmanaged: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManagerMetrics {
    pub center_skill_count: usize,
    pub target_count: usize,
    pub unmanaged_count: usize,
    pub issue_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManagerOverview {
    pub metrics: SkillManagerMetrics,
    pub skills: Vec<SkillSummary>,
    pub agents: Vec<AgentSummary>,
    pub packs: Vec<SkillPackSummary>,
    pub issues: Vec<DiagnosisIssue>,
    pub settings: SkillManagerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skill_type: String,
    pub source_type: String,
    pub source_uri: Option<String>,
    pub center_path: String,
    pub current_hash: String,
    pub status: String,
    pub installed_agents: Vec<InstalledAgentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAgentRef {
    pub agent_id: String,
    pub display_name: String,
    pub icon_key: String,
    pub mode: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    #[serde(flatten)]
    pub summary: SkillSummary,
    pub center_resolved_path: Option<String>,
    pub frontmatter: std::collections::BTreeMap<String, String>,
    pub files: Option<FileTreeNode>,
    pub targets: Vec<SkillTargetDetail>,
    pub source: Option<SkillSourceDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceDetail {
    pub source_type: String,
    pub source_uri: Option<String>,
    pub source_ref: Option<String>,
    pub imported_from_agent: Option<String>,
    pub imported_from_path: Option<String>,
    pub installed_via: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetDetail {
    pub id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub target_path: String,
    pub resolved_target_path: Option<String>,
    pub install_mode: String,
    pub actual_mode: String,
    pub source_hash: String,
    pub current_hash: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub claims: Vec<TargetClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetClaim {
    pub id: String,
    pub claim_type: String,
    pub pack_id: Option<String>,
    pub pack_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub id: String,
    pub display_name: String,
    pub icon_key: String,
    pub enabled: bool,
    pub skills_dir: Option<String>,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub installed: bool,
    pub managed_skill_count: usize,
    pub unmanaged_skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetail {
    pub id: String,
    pub display_name: String,
    pub icon_key: String,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub skills_dir: Option<String>,
    pub config_path: Option<String>,
    pub mcp_config_path: Option<String>,
    pub plugin_dir: Option<String>,
    pub skills: Vec<SkillTargetDetail>,
    pub applied_packs: Vec<AppliedPackSummary>,
    pub available_packs: Vec<SkillPackSummary>,
    pub mcp_servers: Vec<McpServerStatus>,
    pub plugins: Vec<PluginStatus>,
    pub health: Vec<AgentHealthIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedPackSummary {
    pub pack_id: String,
    pub pack_name: String,
    pub member_count: usize,
    pub agent_id: Option<String>,
    pub display_name: Option<String>,
    pub icon_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub member_count: usize,
    pub applied_agent_count: usize,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub members: Vec<PackMember>,
    pub applied_agents: Vec<AppliedPackSummary>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMember {
    pub skill_id: String,
    pub skill_name: String,
    pub required: bool,
    pub sort_order: i64,
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub valid: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub enabled: bool,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealthIssue {
    pub kind: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: String,
    pub path: String,
    pub children: Option<Vec<FileTreeNode>>,
}

// ── Preview / execution DTOs ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictBlocker {
    pub skill_id: String,
    pub agent_id: String,
    pub reason: String,
    pub existing_path: Option<String>,
    pub existing_path_kind: Option<String>,
    pub resolved_existing_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionBlockerDecision {
    pub skill_id: String,
    pub agent_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionChange {
    pub skill_id: String,
    pub agent_id: String,
    pub action: String,
    pub actual_mode: Option<String>,
    pub reason: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionPreview {
    pub skill_ids: Vec<String>,
    pub target_agents: Vec<String>,
    pub requested_mode: String,
    pub changes: Vec<DistributionChange>,
    pub blockers: Vec<ConflictBlocker>,
    #[serde(default)]
    pub blocker_decisions: Vec<DistributionBlockerDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCenterSkillInput {
    pub source_path: String,
    /// "local_folder" | "archive" | "agent_import" | "manual_center" | ...
    pub source_type: String,
    pub source_uri: Option<String>,
    pub imported_from_agent: Option<String>,
    pub imported_from_path: Option<String>,
    /// when importing a directory holding many skills
    pub multi: Option<bool>,
    /// "copy" | "link"; defaults to "copy" when omitted.
    #[serde(default)]
    pub import_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCenterSkillCandidate {
    pub skill_id: String,
    pub proposed_skill_id: String,
    pub name: String,
    pub description: String,
    pub source_dir: String,
    pub hash: String,
    pub action: String, // "create" | "update" | "blocked_same_name_diff_source"
    pub existing_source_type: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCenterSkillPreview {
    pub candidates: Vec<AddCenterSkillCandidate>,
    pub blockers: Vec<AddCenterSkillCandidate>,
    pub center_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCenterSkillDecision {
    pub skill_id: String,
    pub proposed_skill_id: Option<String>,
    /// "create" | "update" | "skip"
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCenterSkillResult {
    pub skill_ids: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCenterSkillPreview {
    pub skill_id: String,
    pub affected_targets: Vec<AffectedTarget>,
    pub removable: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedTarget {
    pub target_id: String,
    pub agent_id: String,
    pub display_name: String,
    pub target_path: String,
    pub mode: String,
    pub claim_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillPackPreview {
    pub pack_id: String,
    pub pack_name: String,
    pub applied_agents: Vec<String>,
    pub affected_targets: Vec<AffectedTarget>,
    pub removable: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovePackFromAgentPreview {
    pub pack_id: String,
    pub pack_name: String,
    pub agent_id: String,
    pub display_name: String,
    pub affected_targets: Vec<AffectedTarget>,
    pub will_remove_targets: usize,
    pub will_preserve_targets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSkillFromPackPreview {
    pub pack_id: String,
    pub pack_name: String,
    pub skill_id: String,
    pub skill_name: String,
    pub affected_targets: Vec<AffectedTarget>,
    pub applied_agent_count: usize,
    pub can_keep_standalone: bool,
    pub can_remove_targets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmanagedItemDto {
    pub id: String,
    pub item_type: String,
    pub agent_id: Option<String>,
    pub path: String,
    pub inferred_skill_id: Option<String>,
    pub hash: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillInventoryAgent {
    pub agent_id: String,
    pub display_name: String,
    pub icon_key: String,
    pub skills_dir: Option<String>,
    pub installed: bool,
    pub managed_count: usize,
    pub unmanaged_count: usize,
    pub importable_count: usize,
    pub items: Vec<AgentSkillInventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillInventoryItem {
    pub id: String,
    pub agent_id: String,
    pub skill_id: String,
    pub name: String,
    pub path: String,
    pub managed: bool,
    pub can_import: bool,
    pub status: String,
    pub status_label: String,
    pub reason: Option<String>,
    pub target_id: Option<String>,
    pub actual_mode: Option<String>,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisIssue {
    pub id: String,
    pub issue_type: String,
    pub severity: String,
    pub fix_kind: String,
    pub title: String,
    pub detail: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub actions: Vec<DiagnosisAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisAction {
    pub id: String,
    pub label: String,
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub schema_version: i64,
    pub exported_at: String,
    pub center_path: String,
    pub skills: Vec<serde_json::Value>,
    pub sources: Vec<serde_json::Value>,
    pub agents: Vec<serde_json::Value>,
    pub targets: Vec<serde_json::Value>,
    pub claims: Vec<serde_json::Value>,
    pub packs: Vec<serde_json::Value>,
    pub diagnosis_summary: serde_json::Value,
}

// ── Settings update input ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub center_path: Option<String>,
    pub sqlite_path: Option<String>,
    pub default_distribute_mode: Option<String>,
    pub link_fail_policy: Option<String>,
    pub startup_scan: Option<bool>,
    pub show_unmanaged: Option<bool>,
}
