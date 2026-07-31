//! Skill Manager v2 core service — center library, scanning, distribution,
//! target/claim rules, skill packs, copy sync and deletion.

// Internal DB row builders and state helpers intentionally carry several
// fields/arguments; the `Ok(x?)` shape reads clearly at service boundaries.
#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_question_mark)]

use crate::skills::v2::agent_meta;
use crate::skills::v2::db::{self, Db};
use crate::skills::v2::fsutil::{self, inspect_path, PathKind};
use crate::skills::v2::models::*;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SHARED_SKILLS_AGENT_ID: &str = "agents";
const DEFAULT_SKILL_PACK_ID: &str = "default";
const DEFAULT_SKILL_PACK_NAME: &str = "全量技能包";
const DEFAULT_SKILL_PACK_DESCRIPTION: &str =
    "中心库全部 Skills。无需维护成员，应用时按当前中心库全量分发。";

pub struct Service {
    pub db: Arc<Db>,
    pub home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillTargetDistributionFailure {
    pub target_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillTargetDistributionsResult {
    pub deleted: usize,
    pub failures: Vec<DeleteSkillTargetDistributionFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdoptBatchItem {
    pub agent_id: String,
    pub unmanaged_id: String,
    pub option: String,
    pub renamed_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteUnmanagedAgentSkillFailure {
    pub unmanaged_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdoptBatchItemResult {
    pub unmanaged_id: String,
    pub skill_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdoptBatchResult {
    pub items: Vec<AdoptBatchItemResult>,
    pub finalization_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteUnmanagedAgentSkillsResult {
    pub deleted: usize,
    pub failures: Vec<DeleteUnmanagedAgentSkillFailure>,
}

impl Service {
    pub fn new(sqlite_path: &Path, home: PathBuf) -> Result<Self, String> {
        let db = Arc::new(Db::open(sqlite_path)?);
        let service = Service { db, home };
        service.recover_interrupted_pack_syncs()?;
        Ok(service)
    }

    fn recover_interrupted_pack_syncs(&self) -> Result<(), String> {
        let now = db::now_iso();
        self.db.transaction(|tx| {
            tx.execute(
                "UPDATE skill_packs
                 SET last_sync_status = 'pending', last_sync_error = NULL
                 WHERE id IN (
                   SELECT DISTINCT pack_id FROM skill_pack_agent_syncs WHERE status = 'syncing'
                 )",
                [],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE skill_pack_agent_syncs
                 SET status = 'pending', error = NULL, updated_at = ?1
                 WHERE status = 'syncing'",
                params![now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn center_path(&self) -> Result<PathBuf, String> {
        self.settings()?;
        Ok(fixed_center_path(&self.home))
    }

    pub fn settings(&self) -> Result<SkillManagerSettings, String> {
        let (settings, previous_center) = self.db.with_conn(|c| {
            let v = db::load_settings_json(c);
            if v.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                let mut def = SkillManagerSettings::default();
                normalize_fixed_center_path(&self.home, &mut def);
                // persist defaults so the file mirror + DB agree
                let val = serde_json::to_value(&def).map_err(|e| e.to_string())?;
                db::save_settings_json(c, &val)?;
                Ok((def, None))
            } else {
                let mut settings: SkillManagerSettings =
                    serde_json::from_value(v).map_err(|e| e.to_string())?;
                let previous_center = normalize_fixed_center_path(&self.home, &mut settings);
                if previous_center.is_some() {
                    let val = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
                    db::save_settings_json(c, &val)?;
                    let _ = std::fs::write(
                        fsutil::settings_path(),
                        serde_json::to_string_pretty(&val).unwrap_or_default(),
                    );
                }
                Ok((settings, previous_center))
            }
        })?;
        if let Some(source) = previous_center {
            migrate_center_skills_best_effort(&source, &fixed_center_path(&self.home));
        }
        Ok(settings)
    }

    pub fn update_settings(&self, update: SettingsUpdate) -> Result<SkillManagerSettings, String> {
        let (next, previous_center) = self.db.with_conn(|c| {
            // Read directly from this connection — calling self.settings() here
            // would re-lock the Mutex and self-deadlock.
            let raw = db::load_settings_json(c);
            let mut current: SkillManagerSettings =
                if raw.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    SkillManagerSettings::default()
                } else {
                    serde_json::from_value(raw).map_err(|e| e.to_string())?
                };
            if let Some(v) = update.sqlite_path {
                current.sqlite_path = v;
            }
            if let Some(v) = update.default_distribute_mode {
                current.default_distribute_mode = v;
            }
            if let Some(v) = update.link_fail_policy {
                current.link_fail_policy = v;
            }
            if let Some(v) = update.startup_scan {
                current.startup_scan = v;
            }
            if let Some(v) = update.show_unmanaged {
                current.show_unmanaged = v;
            }
            if let Some(v) = update.auto_sync_skill_packs {
                current.auto_sync_skill_packs = v;
            }
            let previous_center = normalize_fixed_center_path(&self.home, &mut current);
            let val = serde_json::to_value(&current).map_err(|e| e.to_string())?;
            db::save_settings_json(c, &val)?;
            // mirror to file for human inspection
            let _ = std::fs::write(
                fsutil::settings_path(),
                serde_json::to_string_pretty(&val).unwrap_or_default(),
            );
            Ok((current, previous_center))
        })?;
        let center = fixed_center_path(&self.home);
        if let Some(source) = previous_center {
            migrate_center_skills_best_effort(&source, &center);
        }
        std::fs::create_dir_all(&center).map_err(|e| format!("center mkdir: {}", e))?;
        self.refresh_snapshot_best_effort();
        Ok(next)
    }

    /// Ensure the DB-backed manager can be read without doing a filesystem scan.
    /// Full center/agent scanning is intentionally kept behind refresh/init so
    /// opening the Skill library can render from cached SQLite state immediately.
    pub fn bootstrap(&self) -> Result<(), String> {
        let center = self.center_path()?;
        std::fs::create_dir_all(&center).map_err(|e| format!("center mkdir: {}", e))?;
        let applied = self.db.applied_version()?;
        if applied == SCHEMA_VERSION && self.is_empty_state()? {
            self.migrate_legacy_metadata()?;
        }
        Ok(())
    }

    /// Ensure center dir exists; migrate legacy metadata if DB is fresh.
    pub fn init(&self) -> Result<(), String> {
        self.bootstrap()?;
        self.scan_center_into_db()?;
        self.scan_all_agents_into_db()?;
        Ok(())
    }

    fn refresh_snapshot_best_effort(&self) {
        if let Err(e) = crate::skills::v2::snapshot::export_to_file(self) {
            log::warn!("Skill Manager v2 snapshot refresh failed: {}", e);
        }
    }

    fn is_empty_state(&self) -> Result<bool, String> {
        self.db.with_conn(|c| {
            let n: i64 = c
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            Ok(n == 0)
        })
    }

    /// Migrate legacy `~/.agentbro/metadata.json` (sources, packs) into the DB.
    /// Never deletes the legacy file.
    pub fn migrate_legacy_metadata(&self) -> Result<(), String> {
        let path = db::legacy_metadata_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Ok(());
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            return Ok(());
        };

        // Resolve paths BEFORE entering the transaction — `center_path()` reads
        // settings via the DB connection, and re-locking the Mutex inside the
        // transaction would self-deadlock (std Mutex is not reentrant).
        let center_root = self.center_path()?;

        self.db.transaction(|tx| {
            // sources: { skillId: { origin } }
            if let Some(sources) = v.get("sources").and_then(|s| s.as_object()) {
                for (skill_id, info) in sources {
                    let origin = info.get("origin").and_then(|o| o.as_str()).unwrap_or("");
                    if skill_exists(tx, skill_id)? {
                        continue;
                    }
                    let dir = center_root.join(skill_id);
                    let (hash, name, desc) = if dir.is_dir() {
                        let fm = fsutil::read_frontmatter(&dir);
                        (
                            fsutil::hash_dir(&dir),
                            fm.name().unwrap_or(skill_id).to_string(),
                            fm.description().to_string(),
                        )
                    } else {
                        (String::new(), skill_id.clone(), String::new())
                    };
                    upsert_skill(
                        tx,
                        skill_id,
                        &name,
                        &desc,
                        "skill",
                        &dir.display().to_string(),
                        &hash,
                        &serde_json::json!({}),
                    )?;
                    upsert_source(
                        tx,
                        skill_id,
                        "manual_center",
                        Some(origin),
                        None,
                        None,
                        None,
                        "migration",
                    )?;
                }
            }
            // packs
            if let Some(packs) = v.get("packs").and_then(|p| p.as_array()) {
                for pack in packs {
                    let id = pack.get("id").and_then(|i| i.as_str()).unwrap_or("pack").to_string();
                    let name = pack.get("name").and_then(|i| i.as_str()).unwrap_or(&id).to_string();
                    let description = pack.get("description").and_then(|i| i.as_str()).unwrap_or("").to_string();
                    let now = db::now_iso();
                    tx.execute(
                        "INSERT OR IGNORE INTO skill_packs(id, name, description, tags_json, created_at, updated_at)
                         VALUES (?1, ?2, ?3, '[]', ?4, ?4)",
                        params![id, name, description, now],
                    )
                    .map_err(|e| e.to_string())?;
                    if let Some(skills) = pack.get("skills").and_then(|s| s.as_array()) {
                        for (idx, sid) in skills.iter().enumerate() {
                            if let Some(sid) = sid.as_str() {
                                tx.execute(
                                    "INSERT OR IGNORE INTO skill_pack_members(pack_id, skill_id, sort_order, required)
                                     VALUES (?1, ?2, ?3, 1)",
                                    params![id, sid, idx as i64],
                                )
                                .map_err(|e| e.to_string())?;
                            }
                        }
                    }
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    // ── Center library scanning ───────────────────────────────────

    /// Scan the configured center root and upsert skill rows. Returns ids found.
    pub fn scan_center_into_db(&self) -> Result<Vec<String>, String> {
        let mut found = Vec::new();
        let now = db::now_iso();
        let center = self.center_path()?;
        if center.is_dir() {
            self.scan_one_center_root(&center, &now, &mut found)?;
        }
        Ok(found)
    }

    fn scan_one_center_root(
        &self,
        center: &std::path::Path,
        now: &str,
        found: &mut Vec<String>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(center).map_err(|e| format!("read center: {}", e))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if fsutil::is_ignored_entry(&name) || name.starts_with('.') {
                continue;
            }
            if name == "agentbro-skills.snapshot.json" {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_id = fsutil::infer_skill_id(&path);
            let fm = fsutil::read_frontmatter(&path);
            let hash = fsutil::hash_dir(&path);
            let name = fm
                .name()
                .map(String::from)
                .unwrap_or_else(|| skill_id.clone());
            let desc = fm.description().to_string();
            self.db.transaction(|tx| {
                upsert_skill_full(
                    tx,
                    &skill_id,
                    &name,
                    &desc,
                    "skill",
                    &path.display().to_string(),
                    &hash,
                    &serde_json::to_value(&fm.map).map_err(|e| e.to_string())?,
                    now,
                )?;
                // If no source recorded yet, mark as manual_center.
                let has_source: bool = tx
                    .query_row(
                        "SELECT 1 FROM skill_sources WHERE skill_id = ?1",
                        params![skill_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                    .is_some();
                if !has_source {
                    upsert_source(
                        tx,
                        &skill_id,
                        "manual_center",
                        None,
                        None,
                        None,
                        None,
                        "scan",
                    )?;
                }
                Ok(())
            })?;
            found.push(skill_id.clone());
        }
        Ok(())
    }

    /// Scan all managed agents; record unmanaged items + update target statuses.
    pub fn scan_all_agents_into_db(&self) -> Result<(), String> {
        for agent_id in agent_meta::managed_agent_ids() {
            self.scan_one_agent_into_db(&agent_id)?;
        }
        Ok(())
    }

    pub fn scan_one_agent_into_db(&self, agent_id: &str) -> Result<AgentScanResult, String> {
        self.ensure_agent_row(agent_id)?;
        let skill_dirs = agent_meta::agent_skill_dirs(&self.home, agent_id);
        let primary_skills_dir = agent_meta::agent_skills_dir(&self.home, agent_id);
        if skill_dirs.is_empty() {
            return Ok(AgentScanResult {
                managed: 0,
                unmanaged: 0,
                read_only: 0,
                included_shared: false,
                shared_managed: 0,
                shared_unmanaged: 0,
                shared_read_only: 0,
            });
        }
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE agents SET skills_dir = ?1, last_scanned_at = ?2 WHERE id = ?3",
                params![
                    primary_skills_dir
                        .as_ref()
                        .map(|path| path.display().to_string()),
                    now,
                    agent_id
                ],
            )
            .map_err(|e| e.to_string())
        })?;

        // Wipe stale unmanaged rows for this agent, then re-detect.
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM unmanaged_items WHERE agent_id = ?1",
                params![agent_id],
            )
            .map_err(|e| e.to_string())
        })?;

        let mut managed = 0usize;
        let mut unmanaged = 0usize;
        let mut read_only = 0usize;
        let mut seen_skill_ids = BTreeSet::new();
        let shared_skills_dir = self.home.join(".agents").join("skills");
        for skills_dir in skill_dirs {
            if agent_meta::inherits_shared_agents_skills(agent_id)
                && skills_dir == shared_skills_dir
            {
                continue;
            }
            if !skills_dir.is_dir() {
                continue;
            }
            let skill_paths = discover_agent_skill_paths(
                &skills_dir,
                matches!(agent_id, "openclaw" | SHARED_SKILLS_AGENT_ID),
                agent_id == SHARED_SKILLS_AGENT_ID,
            )?;
            for path in skill_paths {
                let inferred = fsutil::infer_skill_id(&path);
                if !seen_skill_ids.insert(inferred.clone()) {
                    continue;
                }
                // is there a managed target for this agent+path?
                let target = self.find_target_by_path(agent_id, &path)?;
                match target {
                    Some((target_id, skill_id, _)) => {
                        managed += 1;
                        self.refresh_target_status(&target_id, &skill_id, &path)?;
                    }
                    None => {
                        let center_known = self.skill_id_known(&inferred)?;
                        let path_is_read_only =
                            agent_meta::is_read_only_agent_skill_path(&self.home, agent_id, &path);
                        let reason = if path_is_read_only {
                            "agent_builtin_read_only"
                        } else if is_shared_agents_skill_path(&self.home, &path) {
                            "shared_agents_directory"
                        } else if center_known {
                            "same_name_as_center_skill"
                        } else {
                            "not_in_center_library"
                        };
                        if path_is_read_only {
                            read_only += 1;
                        } else {
                            unmanaged += 1;
                        }
                        self.record_unmanaged(
                            agent_id,
                            &path,
                            &inferred,
                            Some(fsutil::hash_dir(&path)),
                            reason,
                        )?;
                    }
                }
            }
        }
        Ok(AgentScanResult {
            managed,
            unmanaged,
            read_only,
            included_shared: false,
            shared_managed: 0,
            shared_unmanaged: 0,
            shared_read_only: 0,
        })
    }

    pub fn scan_agent_inventory_into_db(&self, agent_id: &str) -> Result<AgentScanResult, String> {
        let mut result = self.scan_one_agent_into_db(agent_id)?;
        if agent_meta::inherits_shared_agents_skills(agent_id) {
            let shared = self.scan_one_agent_into_db(SHARED_SKILLS_AGENT_ID)?;
            result.included_shared = true;
            result.shared_managed = shared.managed;
            result.shared_unmanaged = shared.unmanaged;
            result.shared_read_only = shared.read_only;
        }
        Ok(result)
    }

    fn ensure_agent_row(&self, agent_id: &str) -> Result<bool, String> {
        let display = agent_meta::display_name(agent_id);
        let installed = agent_meta::agent_installed(&self.home, agent_id);
        let skills_dir =
            agent_meta::agent_skills_dir(&self.home, agent_id).map(|p| p.display().to_string());
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO agents(id, display_name, skills_dir, enabled) VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, skills_dir = COALESCE(excluded.skills_dir, agents.skills_dir)",
                params![agent_id, display, skills_dir],
            )
            .map_err(|e| e.to_string())?;
            // mark enabled based on install presence
            c.execute(
                "UPDATE agents SET enabled = ?1 WHERE id = ?2",
                params![installed as i64, agent_id],
            )
            .map_err(|e| e.to_string())
        })?;
        if agent_id == "workbuddy" {
            self.cleanup_workbuddy_marketplace_unmanaged()?;
        }
        Ok(installed)
    }

    fn cleanup_workbuddy_marketplace_unmanaged(&self) -> Result<(), String> {
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM unmanaged_items
                 WHERE agent_id = 'workbuddy'
                   AND path LIKE '%/.workbuddy/skills-marketplace/%'",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
    }

    fn find_target_by_path(
        &self,
        agent_id: &str,
        path: &Path,
    ) -> Result<Option<(String, String, String)>, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, skill_id, target_path FROM skill_targets WHERE agent_id = ?1 AND target_path = ?2",
                params![agent_id, path.display().to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    fn skill_id_known(&self, skill_id: &str) -> Result<bool, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT 1 FROM skills WHERE id = ?1",
                params![skill_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())
            .map(|o| o.is_some())
        })
    }

    fn record_unmanaged(
        &self,
        agent_id: &str,
        path: &Path,
        inferred: &str,
        hash: Option<String>,
        reason: &str,
    ) -> Result<(), String> {
        let now = db::now_iso();
        let id = format!(
            "unm-{agent_id}-{}",
            sanitize_for_id(&path.display().to_string())
        );
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO unmanaged_items(id, item_type, agent_id, path, inferred_skill_id, hash, reason, first_seen_at, last_seen_at)
                 VALUES (?1, 'agent_skill', ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(id) DO UPDATE SET hash = excluded.hash, reason = excluded.reason, last_seen_at = excluded.last_seen_at, inferred_skill_id = excluded.inferred_skill_id",
                params![id, agent_id, path.display().to_string(), inferred, hash, reason, now],
            )
            .map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    /// Recompute a target's status + current_hash by inspecting the filesystem.
    fn refresh_target_status(
        &self,
        target_id: &str,
        skill_id: &str,
        path: &Path,
    ) -> Result<(), String> {
        let (stored_mode, source_hash) = self.target_actual_mode_and_source(target_id)?;
        let actual_mode = filesystem_target_mode(path).unwrap_or(stored_mode);
        let current_hash = if actual_mode == "copy" {
            if path.is_dir() {
                Some(fsutil::hash_dir(path))
            } else {
                None
            }
        } else {
            None
        };
        let status = self.compute_target_status(
            skill_id,
            actual_mode.as_str(),
            &source_hash,
            current_hash.as_deref(),
            path,
        )?;
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE skill_targets SET actual_mode = ?1, current_hash = ?2, status = ?3, updated_at = ?4 WHERE id = ?5",
                params![actual_mode, current_hash, status, now, target_id],
            )
            .map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    fn target_actual_mode_and_source(&self, target_id: &str) -> Result<(String, String), String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT actual_mode, source_hash FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .map_err(|e| e.to_string())
        })
    }

    /// Compute status string for a target based on filesystem reality.
    pub fn compute_target_status(
        &self,
        skill_id: &str,
        actual_mode: &str,
        source_hash: &str,
        current_hash: Option<&str>,
        path: &Path,
    ) -> Result<String, String> {
        match inspect_path(path) {
            PathKind::Missing => Ok("missing".to_string()),
            PathKind::BrokenSymlink => Ok("broken_link".to_string()),
            PathKind::Symlink(_) => Ok("ok".to_string()),
            PathKind::File => Ok("ok".to_string()),
            PathKind::Dir => {
                if actual_mode == "copy" {
                    let current = current_hash
                        .map(String::from)
                        .unwrap_or_else(|| fsutil::hash_dir(path));
                    let center_hash = self
                        .live_center_hash(skill_id)?
                        .unwrap_or_else(|| source_hash.to_string());
                    let center_changed = center_hash != source_hash;
                    let copy_changed = current != source_hash;
                    if center_changed && copy_changed {
                        Ok("copy_diverged".to_string())
                    } else if center_changed {
                        Ok("copy_outdated".to_string())
                    } else if copy_changed {
                        Ok("copy_modified".to_string())
                    } else {
                        Ok("ok".to_string())
                    }
                } else {
                    Ok("ok".to_string())
                }
            }
        }
    }

    fn live_center_hash(&self, skill_id: &str) -> Result<Option<String>, String> {
        let Some(row) = self.skill_row(skill_id)? else {
            return Ok(None);
        };
        let path = Path::new(&row.center_path);
        if path.is_dir() {
            Ok(Some(fsutil::hash_dir(path)))
        } else {
            Ok(Some(row.current_hash))
        }
    }

    // ── Overview & reads ──────────────────────────────────────────

    pub fn overview(&self) -> Result<SkillManagerOverview, String> {
        self.overview_with_target_refresh(false)
    }

    pub fn skill_pack_picker_data(&self) -> Result<SkillPackPickerData, String> {
        self.init_if_needed()?;
        let agents = self
            .list_managed_agents()?
            .into_iter()
            .filter(|agent| agent.installed && agent.enabled && agent.skills_dir.is_some())
            .collect::<Vec<_>>();
        let mut applied_by_agent = agents
            .iter()
            .map(|agent| (agent.id.clone(), Vec::new()))
            .collect::<HashMap<_, _>>();
        let applied = self.db.with_conn(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT DISTINCT t.agent_id, c.pack_id
                     FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id
                     WHERE c.pack_id IS NOT NULL
                     ORDER BY t.agent_id, c.pack_id",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| error.to_string())?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row.map_err(|error| error.to_string())?);
            }
            Ok(values)
        })?;
        for (agent_id, pack_id) in applied {
            if let Some(pack_ids) = applied_by_agent.get_mut(&agent_id) {
                pack_ids.push(pack_id);
            }
        }
        Ok(SkillPackPickerData {
            agents,
            packs: self.list_skill_packs()?,
            applied_by_agent,
            default_distribute_mode: self.settings()?.default_distribute_mode,
        })
    }

    fn overview_with_target_refresh(
        &self,
        refresh_targets: bool,
    ) -> Result<SkillManagerOverview, String> {
        self.init_if_needed()?;
        let skills = self.list_center_skills_with_target_refresh(refresh_targets)?;
        let agents = self.list_managed_agents()?;
        let packs = self.list_skill_packs()?;
        let issues = self.list_current_diagnosis_issues()?;
        let target_count = self.count_targets()?;
        let unmanaged_count = self.count_unmanaged()?;
        Ok(SkillManagerOverview {
            metrics: SkillManagerMetrics {
                center_skill_count: skills.len(),
                target_count,
                unmanaged_count,
                issue_count: issues.len(),
            },
            skills,
            agents,
            packs,
            issues,
            settings: self.settings()?,
        })
    }

    pub fn list_current_diagnosis_issues(&self) -> Result<Vec<DiagnosisIssue>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, issue_type, severity, entity_type, entity_id, title, detail, fix_kind
                     FROM diagnosis_issues WHERE resolved_at IS NULL ORDER BY created_at DESC, id",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    let issue_type: String = r.get(1)?;
                    let fix_kind: String = r.get(7)?;
                    let actions = if fix_kind == "info" {
                        Vec::new()
                    } else {
                        vec![DiagnosisAction {
                            id: format!("fix:{issue_type}"),
                            label: "修复".to_string(),
                            destructive: fix_kind == "confirm",
                        }]
                    };
                    Ok(DiagnosisIssue {
                        id: r.get(0)?,
                        issue_type,
                        severity: r.get(2)?,
                        entity_type: r.get(3)?,
                        entity_id: r.get(4)?,
                        title: r.get(5)?,
                        detail: r.get(6)?,
                        fix_kind,
                        actions,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut issues = Vec::new();
            for row in rows {
                issues.push(row.map_err(|e| e.to_string())?);
            }
            Ok(issues)
        })
    }

    fn init_if_needed(&self) -> Result<(), String> {
        // cheap: only ensure center dir + rescan if center dir changed
        let center = self.center_path()?;
        if !center.is_dir() {
            std::fs::create_dir_all(&center).map_err(|e| format!("center mkdir: {}", e))?;
        }
        Ok(())
    }

    pub fn refresh(&self) -> Result<(), String> {
        self.scan_center_into_db()?;
        self.scan_all_agents_into_db()?;
        Ok(())
    }

    pub fn refresh_overview(&self) -> Result<SkillManagerOverview, String> {
        self.refresh()?;
        self.overview_with_target_refresh(false)
    }

    fn count_targets(&self) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM skill_targets", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }
    fn count_unmanaged(&self) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM unmanaged_items WHERE reason <> 'agent_builtin_read_only'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }

    pub fn list_center_skills(&self) -> Result<Vec<SkillSummary>, String> {
        self.list_center_skills_with_target_refresh(true)
    }

    fn list_center_skills_with_target_refresh(
        &self,
        refresh_targets: bool,
    ) -> Result<Vec<SkillSummary>, String> {
        let rows = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT s.id, s.name, s.description, s.skill_type, s.current_hash, s.center_path,
                            src.source_type, src.source_uri
                     FROM skills s LEFT JOIN skill_sources src ON src.skill_id = s.id
                     ORDER BY s.name COLLATE NOCASE",
                )
                .map_err(|e| e.to_string())?;
            let out = stmt
                .query_map([], |r| {
                    Ok(SkillRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        skill_type: r.get(3)?,
                        current_hash: r.get(4)?,
                        center_path: r.get(5)?,
                        source_type: r.get(6)?,
                        source_uri: r.get(7)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in out {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;

        if refresh_targets {
            for row in &rows {
                self.refresh_targets_for_skill(&row.id)?;
            }
        }

        let mut targets_by_skill = self.targets_by_skill()?;
        let mut summaries = Vec::new();
        for row in rows {
            let targets = targets_by_skill.remove(&row.id).unwrap_or_default();
            let installed_agents = self.installed_agent_refs(&targets);
            let status = self.aggregate_skill_status(&row, &targets);
            summaries.push(SkillSummary {
                id: row.id,
                name: row.name,
                description: row.description,
                skill_type: row.skill_type,
                source_type: row
                    .source_type
                    .unwrap_or_else(|| "manual_center".to_string()),
                source_uri: row.source_uri,
                center_path: row.center_path,
                current_hash: row.current_hash,
                status,
                installed_agents,
            });
        }
        Ok(summaries)
    }

    fn targets_by_skill(&self) -> Result<HashMap<String, Vec<TargetRow>>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, skill_id, agent_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at
                     FROM skill_targets ORDER BY skill_id, agent_id",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(TargetRow {
                        id: r.get(0)?,
                        skill_id: r.get(1)?,
                        agent_id: r.get(2)?,
                        target_path: r.get(3)?,
                        install_mode: r.get(4)?,
                        actual_mode: r.get(5)?,
                        source_hash: r.get(6)?,
                        current_hash: r.get(7)?,
                        status: r.get(8)?,
                        created_at: r.get(9)?,
                        updated_at: r.get(10)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut by_skill: HashMap<String, Vec<TargetRow>> = HashMap::new();
            for row in rows {
                let target = row.map_err(|e| e.to_string())?;
                by_skill
                    .entry(target.skill_id.clone())
                    .or_default()
                    .push(target);
            }
            Ok(by_skill)
        })
    }

    fn aggregate_skill_status(&self, row: &SkillRow, targets: &[TargetRow]) -> String {
        let _ = row;
        for t in targets {
            if matches!(t.status.as_str(), "broken_link" | "missing") {
                return "conflict".to_string();
            }
            if matches!(t.status.as_str(), "copy_modified" | "copy_diverged") {
                return "copyDiverged".to_string();
            }
            if t.status == "copy_outdated" {
                return "updateAvailable".to_string();
            }
        }
        "ok".to_string()
    }

    fn installed_agent_refs(&self, targets: &[TargetRow]) -> Vec<InstalledAgentRef> {
        targets
            .iter()
            .map(|t| InstalledAgentRef {
                agent_id: t.agent_id.clone(),
                display_name: agent_meta::display_name(&t.agent_id),
                icon_key: agent_meta::icon_key(&t.agent_id),
                mode: t.actual_mode.clone(),
                status: t.status.clone(),
            })
            .collect()
    }

    pub fn get_skill_detail(&self, skill_id: &str) -> Result<SkillDetail, String> {
        self.refresh_targets_for_skill(skill_id)?;
        let row = self
            .db
            .with_conn(|c| {
                c.query_row(
                    "SELECT id, name, description, skill_type, current_hash, center_path
                 FROM skills WHERE id = ?1",
                    params![skill_id],
                    |r| {
                        Ok(SkillRow {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            description: r.get(2)?,
                            skill_type: r.get(3)?,
                            current_hash: r.get(4)?,
                            center_path: r.get(5)?,
                            source_type: None,
                            source_uri: None,
                        })
                    },
                )
                .optional()
                .map_err(|e| e.to_string())
            })?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;

        let source = self.source_for_skill(skill_id)?;
        let targets = self.targets_for_skill(skill_id)?;
        let installed_agents = self.installed_agent_refs(&targets);
        let status = self.aggregate_skill_status(&row, &targets);
        let frontmatter = self.frontmatter_for_skill(skill_id)?;
        let files = fsutil::build_file_tree(Path::new(&row.center_path), 4);

        let summary = SkillSummary {
            id: row.id.clone(),
            name: row.name.clone(),
            description: row.description.clone(),
            skill_type: row.skill_type.clone(),
            source_type: source
                .as_ref()
                .map(|s| s.source_type.clone())
                .unwrap_or_else(|| "manual_center".to_string()),
            source_uri: source.as_ref().and_then(|s| s.source_uri.clone()),
            center_path: row.center_path.clone(),
            current_hash: row.current_hash.clone(),
            status,
            installed_agents,
        };

        let target_details: Vec<SkillTargetDetail> = targets
            .into_iter()
            .map(|t| {
                let claims = self.claims_for_target(&t.id).unwrap_or_default();
                SkillTargetDetail {
                    id: t.id,
                    skill_id: t.skill_id,
                    agent_id: t.agent_id,
                    resolved_target_path: resolved_target_path(&t.target_path),
                    target_path: t.target_path,
                    install_mode: t.install_mode,
                    actual_mode: t.actual_mode,
                    source_hash: t.source_hash,
                    current_hash: t.current_hash,
                    status: t.status,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                    claims,
                }
            })
            .collect();

        Ok(SkillDetail {
            summary,
            center_resolved_path: resolved_target_path(&row.center_path),
            frontmatter,
            files,
            targets: target_details,
            source,
        })
    }

    fn frontmatter_for_skill(&self, skill_id: &str) -> Result<BTreeMap<String, String>, String> {
        self.db.with_conn(|c| {
            let s: Option<String> = c
                .query_row(
                    "SELECT frontmatter_json FROM skills WHERE id = ?1",
                    params![skill_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match s {
                Some(json) => serde_json::from_str(&json).map_err(|e| e.to_string()),
                None => Ok(BTreeMap::new()),
            }
        })
    }

    fn source_for_skill(&self, skill_id: &str) -> Result<Option<SkillSourceDetail>, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT source_type, source_uri, source_ref, imported_from_agent, imported_from_path, installed_via, created_at, updated_at
                 FROM skill_sources WHERE skill_id = ?1",
                params![skill_id],
                |r| {
                    Ok(SkillSourceDetail {
                        source_type: r.get(0)?,
                        source_uri: r.get(1)?,
                        source_ref: r.get(2)?,
                        imported_from_agent: r.get(3)?,
                        imported_from_path: r.get(4)?,
                        installed_via: r.get(5)?,
                        created_at: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    fn targets_for_skill(&self, skill_id: &str) -> Result<Vec<TargetRow>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, agent_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at
                     FROM skill_targets WHERE skill_id = ?1 ORDER BY agent_id",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([skill_id], |r| {
                    Ok(TargetRow {
                        id: r.get(0)?,
                        skill_id: skill_id.to_string(),
                        agent_id: r.get(1)?,
                        target_path: r.get(2)?,
                        install_mode: r.get(3)?,
                        actual_mode: r.get(4)?,
                        source_hash: r.get(5)?,
                        current_hash: r.get(6)?,
                        status: r.get(7)?,
                        created_at: r.get(8)?,
                        updated_at: r.get(9)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })
    }

    fn refresh_targets_for_skill(&self, skill_id: &str) -> Result<(), String> {
        let targets = self.targets_for_skill(skill_id)?;
        for t in targets {
            self.refresh_target_status(&t.id, skill_id, Path::new(&t.target_path))?;
        }
        Ok(())
    }

    fn claims_for_target(&self, target_id: &str) -> Result<Vec<TargetClaim>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT c.id, c.claim_type, c.pack_id, p.name, c.created_at
                     FROM skill_target_claims c LEFT JOIN skill_packs p ON p.id = c.pack_id
                     WHERE c.target_id = ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([target_id], |r| {
                    Ok(TargetClaim {
                        id: r.get(0)?,
                        claim_type: r.get(1)?,
                        pack_id: r.get(2)?,
                        pack_name: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })
    }

    pub fn list_managed_agents(&self) -> Result<Vec<AgentSummary>, String> {
        self.agent_summaries_for_ids(agent_meta::visible_agent_ids())
    }

    fn list_inventory_agents(&self) -> Result<Vec<AgentSummary>, String> {
        self.agent_summaries_for_ids(agent_meta::managed_agent_ids())
    }

    fn agent_summaries_for_ids(&self, agent_ids: Vec<String>) -> Result<Vec<AgentSummary>, String> {
        let mut out = Vec::new();
        for id in &agent_ids {
            let installed = self.ensure_agent_row(id)?;
            let (enabled, skills_dir, version, latest) = self.db.with_conn(|c| {
                c.query_row(
                    "SELECT enabled, skills_dir, version, latest_version FROM agents WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)? != 0,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())
            })?;
            let managed_skill_count = self.count_agent_targets(id)?;
            let unmanaged_skill_count = self.count_agent_unmanaged(id)?;
            let read_only_skill_count = self.count_agent_read_only(id)?;
            out.push(AgentSummary {
                id: id.clone(),
                display_name: agent_meta::display_name(id),
                icon_key: agent_meta::icon_key(id),
                enabled,
                skills_dir,
                version,
                latest_version: latest,
                installed,
                managed_skill_count,
                unmanaged_skill_count,
                read_only_skill_count,
            });
        }
        Ok(out)
    }

    fn count_agent_targets(&self, agent_id: &str) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM skill_targets WHERE agent_id = ?1",
                params![agent_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }
    fn count_agent_unmanaged(&self, agent_id: &str) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM unmanaged_items
                 WHERE agent_id = ?1 AND reason <> 'agent_builtin_read_only'",
                params![agent_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }
    fn count_agent_read_only(&self, agent_id: &str) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM unmanaged_items
                 WHERE agent_id = ?1 AND reason = 'agent_builtin_read_only'",
                params![agent_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }

    pub fn list_unmanaged(&self) -> Result<Vec<UnmanagedItemDto>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, item_type, agent_id, path, inferred_skill_id, hash, reason
                     FROM unmanaged_items ORDER BY agent_id, path",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(UnmanagedItemDto {
                        id: r.get(0)?,
                        item_type: r.get(1)?,
                        agent_id: r.get(2)?,
                        path: r.get(3)?,
                        inferred_skill_id: r.get(4)?,
                        hash: r.get(5)?,
                        reason: r.get(6)?,
                        read_only: r.get::<_, String>(6)? == "agent_builtin_read_only",
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })
    }

    pub fn list_agent_skill_inventory(&self) -> Result<Vec<AgentSkillInventoryAgent>, String> {
        let agents = self.list_inventory_agents()?;
        let center_hashes = self.center_skill_hashes()?;
        let mut items_by_agent: HashMap<String, Vec<AgentSkillInventoryItem>> = HashMap::new();

        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT t.id, t.agent_id, t.skill_id, COALESCE(s.name, t.skill_id), t.target_path, t.actual_mode, t.status, t.current_hash
                     FROM skill_targets t LEFT JOIN skills s ON s.id = t.skill_id
                     ORDER BY t.agent_id, t.target_path",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    let target_id: String = r.get(0)?;
                    let agent_id: String = r.get(1)?;
                    let skill_id: String = r.get(2)?;
                    let name: String = r.get(3)?;
                    let path: String = r.get(4)?;
                    let actual_mode: String = r.get(5)?;
                    let status: String = r.get(6)?;
                    let hash: Option<String> = r.get(7)?;
                    Ok(AgentSkillInventoryItem {
                        id: target_id.clone(),
                        agent_id,
                        skill_id,
                        name,
                        path,
                        managed: true,
                        read_only: false,
                        can_import: false,
                        status,
                        status_label: "已管理".to_string(),
                        reason: None,
                        target_id: Some(target_id),
                        actual_mode: Some(actual_mode),
                        hash,
                    })
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let item = row.map_err(|e| e.to_string())?;
                items_by_agent
                    .entry(item.agent_id.clone())
                    .or_default()
                    .push(item);
            }
            Ok::<_, String>(())
        })?;

        for item in self.list_unmanaged()? {
            let Some(agent_id) = item.agent_id.clone() else {
                continue;
            };
            if item.item_type != "skill" && item.item_type != "agent_skill" {
                continue;
            }
            let skill_id = item
                .inferred_skill_id
                .clone()
                .filter(|id| !id.trim().is_empty())
                .unwrap_or_else(|| infer_name_from_path(&item.path));
            let center_hash = center_hashes.get(&skill_id);
            let hash_matches = center_hash
                .zip(item.hash.as_ref())
                .map(|(center, local)| center == local)
                .unwrap_or(false);
            let (status, status_label, can_import) = if item.read_only {
                (
                    "builtin_read_only".to_string(),
                    "内置只读".to_string(),
                    false,
                )
            } else if center_hash.is_none() {
                ("unmanaged".to_string(), "未管理".to_string(), true)
            } else if hash_matches {
                (
                    "unmanaged_reusable".to_string(),
                    "未管理 · 中心库同内容".to_string(),
                    true,
                )
            } else {
                (
                    "conflict".to_string(),
                    "未管理 · 同名冲突".to_string(),
                    false,
                )
            };
            items_by_agent
                .entry(agent_id.clone())
                .or_default()
                .push(AgentSkillInventoryItem {
                    id: item.id,
                    agent_id,
                    skill_id: skill_id.clone(),
                    name: skill_id,
                    path: item.path,
                    managed: false,
                    read_only: item.read_only,
                    can_import,
                    status,
                    status_label,
                    reason: Some(item.reason),
                    target_id: None,
                    actual_mode: None,
                    hash: item.hash,
                });
        }

        Ok(agents
            .into_iter()
            .map(|agent| {
                let mut items = items_by_agent.remove(&agent.id).unwrap_or_default();
                items.sort_by(|a, b| {
                    a.managed
                        .cmp(&b.managed)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                let managed_count = items.iter().filter(|item| item.managed).count();
                let unmanaged_count = items
                    .iter()
                    .filter(|item| !item.managed && !item.read_only)
                    .count();
                let read_only_count = items.iter().filter(|item| item.read_only).count();
                let importable_count = items.iter().filter(|item| item.can_import).count();
                AgentSkillInventoryAgent {
                    agent_id: agent.id,
                    display_name: agent.display_name,
                    icon_key: agent.icon_key,
                    skills_dir: agent.skills_dir,
                    installed: agent.installed,
                    managed_count,
                    unmanaged_count,
                    read_only_count,
                    importable_count,
                    items,
                }
            })
            .collect())
    }

    fn center_skill_hashes(&self) -> Result<HashMap<String, String>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT id, current_hash FROM skills")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            let mut out = HashMap::new();
            for row in rows {
                let (id, hash) = row.map_err(|e| e.to_string())?;
                out.insert(id, hash);
            }
            Ok(out)
        })
    }

    // ── Project inventory ────────────────────────────────────────

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>, String> {
        let rows = self.project_rows()?;
        rows.into_iter()
            .map(|row| self.project_summary_for_row(&row))
            .collect()
    }

    pub fn add_project(&self, root_path: String) -> Result<ProjectDetail, String> {
        let root = normalize_project_root(&root_path)?;
        let id = project_id_for_path(&root);
        let name = project_name_for_path(&root);
        let root_path = root.display().to_string();
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO projects(id, name, root_path, created_at, updated_at, last_scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?4, ?4)
                 ON CONFLICT(root_path) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at, last_scanned_at = excluded.last_scanned_at",
                params![id, name, root_path, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        self.get_project_detail(&id)
    }

    pub fn remove_project(&self, project_id: &str) -> Result<(), String> {
        self.db.with_conn(|c| {
            c.execute("DELETE FROM projects WHERE id = ?1", params![project_id])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn get_project_detail(&self, project_id: &str) -> Result<ProjectDetail, String> {
        let row = self
            .project_row(project_id)?
            .ok_or_else(|| format!("Project not found: {project_id}"))?;
        self.project_detail_for_row(&row)
    }

    pub fn scan_project(&self, project_id: &str) -> Result<ProjectDetail, String> {
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE projects SET last_scanned_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![project_id, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        self.get_project_detail(project_id)
    }

    pub fn install_center_skills_to_project(
        &self,
        project_id: &str,
        agent_id: &str,
        skill_ids: Vec<String>,
        requested_mode: String,
    ) -> Result<ProjectDetail, String> {
        let row = self
            .project_row(project_id)?
            .ok_or_else(|| format!("Project not found: {project_id}"))?;
        let root = PathBuf::from(&row.root_path);
        let skills_dir = project_agent_skills_dir(&root, agent_id)?;
        std::fs::create_dir_all(&skills_dir)
            .map_err(|e| format!("mkdir {}: {}", skills_dir.display(), e))?;
        for skill_id in skill_ids {
            self.install_one_center_skill_to_project(&skill_id, &skills_dir, &requested_mode)?;
        }
        self.scan_project(project_id)
    }

    pub fn install_skill_pack_to_project(
        &self,
        project_id: &str,
        agent_id: &str,
        pack_id: &str,
        requested_mode: String,
    ) -> Result<ProjectDetail, String> {
        let skill_ids = self
            .get_skill_pack_detail(pack_id)?
            .members
            .into_iter()
            .filter(|member| !member.missing)
            .map(|member| member.skill_id)
            .collect();
        self.install_center_skills_to_project(project_id, agent_id, skill_ids, requested_mode)
    }

    fn project_rows(&self) -> Result<Vec<ProjectRow>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, name, root_path, created_at, updated_at, last_scanned_at
                     FROM projects ORDER BY pinned DESC, updated_at DESC, name COLLATE NOCASE",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(ProjectRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        root_path: r.get(2)?,
                        created_at: r.get(3)?,
                        updated_at: r.get(4)?,
                        last_scanned_at: r.get(5)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
            Ok(out)
        })
    }

    fn project_row(&self, project_id: &str) -> Result<Option<ProjectRow>, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, name, root_path, created_at, updated_at, last_scanned_at
                 FROM projects WHERE id = ?1",
                params![project_id],
                |r| {
                    Ok(ProjectRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        root_path: r.get(2)?,
                        created_at: r.get(3)?,
                        updated_at: r.get(4)?,
                        last_scanned_at: r.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    fn project_summary_for_row(&self, row: &ProjectRow) -> Result<ProjectSummary, String> {
        let root = PathBuf::from(&row.root_path);
        let agents = self.scan_project_agents(&root)?;
        let instructions = project_instruction_files(&root);
        let mut issue_count = 0;
        let mut skill_count = 0;
        let mut mcp_count = 0;
        let mut plugin_count = 0;
        for agent in &agents {
            issue_count += agent.health.len();
            skill_count += agent.skills.len();
            mcp_count += agent.mcp_servers.len();
            plugin_count += agent.plugins.len();
        }
        if !root.is_dir() {
            issue_count += 1;
        }
        Ok(ProjectSummary {
            id: row.id.clone(),
            name: row.name.clone(),
            root_path: row.root_path.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
            last_scanned_at: row.last_scanned_at.clone(),
            detected_agent_count: agents.len(),
            skill_count,
            mcp_count,
            plugin_count,
            instruction_count: instructions.len(),
            issue_count,
        })
    }

    fn project_detail_for_row(&self, row: &ProjectRow) -> Result<ProjectDetail, String> {
        let root = PathBuf::from(&row.root_path);
        let agents = self.scan_project_agents(&root)?;
        let instructions = project_instruction_files(&root);
        let mut health = Vec::new();
        if !root.is_dir() {
            health.push(ProjectHealthIssue {
                agent_id: None,
                kind: "project_missing".to_string(),
                message: format!("Project path does not exist: {}", root.display()),
                severity: "error".to_string(),
            });
        } else if agents.is_empty() && instructions.is_empty() {
            health.push(ProjectHealthIssue {
                agent_id: None,
                kind: "project_no_agent_config".to_string(),
                message: "No project-level Agent skills, MCP, plugin config, or instruction files were detected.".to_string(),
                severity: "info".to_string(),
            });
        }
        let summary = self.project_summary_for_row(row)?;
        Ok(ProjectDetail {
            summary,
            agents,
            instructions,
            health,
        })
    }

    fn scan_project_agents(&self, root: &Path) -> Result<Vec<ProjectAgentDetail>, String> {
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let center_hashes = self.center_skill_hashes()?;
        let mut agents = Vec::new();
        if let Some(agent) = self.scan_claude_project(root, &center_hashes)? {
            agents.push(agent);
        }
        if let Some(agent) = self.scan_codex_project(root, &center_hashes)? {
            agents.push(agent);
        }
        if let Some(agent) = self.scan_kimi_project(root, &center_hashes)? {
            agents.push(agent);
        }
        Ok(agents)
    }

    fn install_one_center_skill_to_project(
        &self,
        skill_id: &str,
        skills_dir: &Path,
        requested_mode: &str,
    ) -> Result<(), String> {
        let row = self
            .skill_row(skill_id)?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let center = Path::new(&row.center_path);
        let target = skills_dir.join(skill_id);
        let source_hash = fsutil::hash_dir(center);
        if target.exists() || target.is_symlink() {
            let target_hash = if let Some(resolved) = fsutil::resolved_symlink_target(&target) {
                fsutil::hash_dir(&resolved)
            } else if target.is_dir() {
                fsutil::hash_dir(&target)
            } else {
                String::new()
            };
            if target_hash == source_hash {
                return Ok(());
            }
            return Err(format!(
                "Project skill already exists and differs: {}",
                target.display()
            ));
        }
        match requested_mode {
            "link" => {
                let linked = fsutil::try_symlink(center, &target)?;
                if !linked {
                    let policy = self.effective_link_fail_policy()?;
                    if policy == "copy" {
                        fsutil::copy_dir_recursive(center, &target)?;
                    } else {
                        return Err(format!(
                            "Could not create symlink at {}. Set link-fail policy to copy, or choose copy.",
                            target.display()
                        ));
                    }
                }
            }
            "copy" => fsutil::copy_dir_recursive(center, &target)?,
            other => return Err(format!("Unknown mode: {other}")),
        }
        Ok(())
    }

    fn scan_claude_project(
        &self,
        root: &Path,
        center_hashes: &HashMap<String, String>,
    ) -> Result<Option<ProjectAgentDetail>, String> {
        let agent_id = "claude-code";
        let skills_dir = root.join(".claude").join("skills");
        let settings = root.join(".claude").join("settings.json");
        let local_settings = root.join(".claude").join("settings.local.json");
        let mcp_json = root.join(".mcp.json");
        let skills = scan_project_skills(agent_id, &skills_dir, center_hashes)?;
        let mut config_paths = existing_paths(&[settings.clone(), local_settings.clone()]);
        let mcp_config_paths = existing_paths(&[mcp_json.clone(), settings.clone()]);
        let plugin_config_paths = existing_paths(&[settings.clone(), local_settings.clone()]);
        let mut mcp_servers = read_json_mcp_servers_path(&mcp_json);
        mcp_servers.extend(read_json_mcp_servers_path(&settings));
        let mut plugins = read_claude_project_plugins_path(&settings);
        plugins.extend(read_claude_project_plugins_path(&local_settings));
        let mut health = Vec::new();
        if skills_dir.exists() && skills.is_empty() {
            health.push(project_agent_issue(
                agent_id,
                "empty_skills_dir",
                &skills_dir,
            ));
        }
        if settings.exists() && !settings.is_file() {
            health.push(project_agent_issue(
                agent_id,
                "invalid_settings_path",
                &settings,
            ));
            config_paths.retain(|path| path != &settings.display().to_string());
        }
        let detected = !skills.is_empty()
            || !mcp_servers.is_empty()
            || !plugins.is_empty()
            || !config_paths.is_empty()
            || root.join(".claude").join("CLAUDE.md").is_file()
            || root.join("CLAUDE.md").is_file();
        if !detected {
            return Ok(None);
        }
        Ok(Some(ProjectAgentDetail {
            agent_id: agent_id.to_string(),
            display_name: agent_meta::display_name(agent_id),
            icon_key: agent_meta::icon_key(agent_id),
            skills_dirs: existing_paths(&[skills_dir]),
            config_paths,
            mcp_config_paths,
            plugin_config_paths,
            skills,
            mcp_servers,
            plugins,
            health,
        }))
    }

    fn scan_codex_project(
        &self,
        root: &Path,
        center_hashes: &HashMap<String, String>,
    ) -> Result<Option<ProjectAgentDetail>, String> {
        let agent_id = "codex";
        let skills_dir = root.join(".agents").join("skills");
        let config = root.join(".codex").join("config.toml");
        let hooks = root.join(".codex").join("hooks.json");
        let skills = scan_project_skills(agent_id, &skills_dir, center_hashes)?;
        let config_paths = existing_paths(&[config.clone(), hooks]);
        let mcp_config_paths = existing_paths(std::slice::from_ref(&config));
        let plugin_config_paths = existing_paths(std::slice::from_ref(&config));
        let mcp_servers = read_toml_mcp_servers_path(&config);
        let plugins = read_codex_project_plugins_path(&config);
        let mut health = Vec::new();
        if skills_dir.exists() && skills.is_empty() {
            health.push(project_agent_issue(
                agent_id,
                "empty_skills_dir",
                &skills_dir,
            ));
        }
        let detected = !skills.is_empty()
            || !mcp_servers.is_empty()
            || !plugins.is_empty()
            || !config_paths.is_empty()
            || root.join("AGENTS.md").is_file()
            || root.join("AGENTS.override.md").is_file();
        if !detected {
            return Ok(None);
        }
        Ok(Some(ProjectAgentDetail {
            agent_id: agent_id.to_string(),
            display_name: agent_meta::display_name(agent_id),
            icon_key: agent_meta::icon_key(agent_id),
            skills_dirs: existing_paths(&[skills_dir]),
            config_paths,
            mcp_config_paths,
            plugin_config_paths,
            skills,
            mcp_servers,
            plugins,
            health,
        }))
    }

    fn scan_kimi_project(
        &self,
        root: &Path,
        center_hashes: &HashMap<String, String>,
    ) -> Result<Option<ProjectAgentDetail>, String> {
        let agent_id = "kimi";
        let kimi_skills_dir = root.join(".kimi-code").join("skills");
        let mcp_json = root.join(".kimi-code").join("mcp.json");
        let kimi_agents_dir = root.join(".kimi-code").join("agents");
        let shared_agents_dir = root.join(".agents").join("agents");

        let skills = scan_project_skills(agent_id, &kimi_skills_dir, center_hashes)?;
        let config_paths = existing_paths(&[kimi_agents_dir.clone(), shared_agents_dir.clone()]);
        let mcp_config_paths = existing_paths(std::slice::from_ref(&mcp_json));
        let mcp_servers = read_json_mcp_servers_path(&mcp_json);
        let mut health = Vec::new();
        if kimi_skills_dir.exists() && skills.is_empty() {
            health.push(project_agent_issue(
                agent_id,
                "empty_skills_dir",
                &kimi_skills_dir,
            ));
        }
        if mcp_json.exists() && !mcp_json.is_file() {
            health.push(project_agent_issue(
                agent_id,
                "invalid_mcp_config_path",
                &mcp_json,
            ));
        }
        let detected = !skills.is_empty()
            || !mcp_servers.is_empty()
            || !config_paths.is_empty()
            || mcp_json.exists()
            || kimi_skills_dir.exists();
        if !detected {
            return Ok(None);
        }
        Ok(Some(ProjectAgentDetail {
            agent_id: agent_id.to_string(),
            display_name: agent_meta::display_name(agent_id),
            icon_key: agent_meta::icon_key(agent_id),
            skills_dirs: existing_paths(&[kimi_skills_dir]),
            config_paths,
            mcp_config_paths,
            plugin_config_paths: Vec::new(),
            skills,
            mcp_servers,
            plugins: Vec::new(),
            health,
        }))
    }

    // ── Add to center library ─────────────────────────────────────

    pub fn preview_add_center_skill(
        &self,
        input: AddCenterSkillInput,
    ) -> Result<AddCenterSkillPreview, String> {
        let center = self.center_path()?;
        let expanded_src = fsutil::expand_tilde(&input.source_path);
        let (src, _temp_root) = if is_remote_skill_source(&input) {
            crate::skills::installer::resolve_external_skill_source(&input.source_path)?
        } else {
            (expanded_src, None)
        };
        let mut candidates = Vec::new();
        let mut blockers = Vec::new();
        let mut unchanged_count = 0;

        // If the source is an archive, extract it to a temp dir first so the
        // rest of the flow can treat it as a folder. The temp dir persists
        // through execute_add_center_skill (which reads cand.source_dir); it
        // lives in /tmp and is cleaned by the OS.
        let src = if src.is_file()
            && matches!(
                src.extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .as_deref(),
                Some("zip")
            ) {
            cleanup_old_temp_imports();
            let dest = std::env::temp_dir().join(format!("agentbro-skill-import-{}", uuid_short()));
            extract_zip(&src, &dest)?;
            dest
        } else {
            src
        };

        // Determine if source is a single skill or a folder of skills.
        let dirs: Vec<PathBuf> = if input.multi.unwrap_or(false) && src.is_dir() {
            std::fs::read_dir(&src)
                .map_err(|e| format!("read source: {}", e))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && fsutil::is_skill_dir(p))
                .collect()
        } else if src.is_dir() && fsutil::is_skill_dir(&src) {
            vec![src.clone()]
        } else if src.is_dir() {
            // a directory containing skills
            std::fs::read_dir(&src)
                .map_err(|e| format!("read source: {}", e))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && fsutil::is_skill_dir(p))
                .collect()
        } else {
            return Err(format!(
                "Not a valid skill source (directory or .zip containing SKILL.md): {}",
                src.display()
            ));
        };

        if dirs.is_empty() {
            return Err(
                "No valid skill directories found (each must contain SKILL.md)".to_string(),
            );
        }

        for dir in dirs {
            let proposed = fsutil::infer_skill_id(&dir);
            let fm = fsutil::read_frontmatter(&dir);
            let mut hash = None;
            let existing = self.skill_row(&proposed)?;
            let (action, reason, existing_source) = match existing {
                None => ("create".to_string(), None, None),
                Some(row) => {
                    let src_row = self.source_for_skill(&proposed)?;
                    let same_source = match &src_row {
                        Some(s) => sources_match_for_candidate(
                            &input,
                            &dir,
                            &s.source_type,
                            s.source_uri.as_deref(),
                        ),
                        None => {
                            let source_hash = hash.get_or_insert_with(|| fsutil::hash_dir(&dir));
                            row.current_hash == *source_hash
                        }
                    };
                    if same_source {
                        let mode_matches =
                            center_import_mode_matches(&input, &row.center_path, &dir);
                        if mode_matches {
                            let content_matches = if input.import_mode.as_deref() == Some("link") {
                                true
                            } else {
                                let source_hash =
                                    hash.get_or_insert_with(|| fsutil::hash_dir(&dir));
                                skill_directories_match(
                                    &dir,
                                    Path::new(&row.center_path),
                                    source_hash,
                                )
                            };
                            if content_matches {
                                unchanged_count += 1;
                                continue;
                            }
                        }
                        ("update".to_string(), None, src_row.map(|s| s.source_type))
                    } else {
                        (
                            "blocked_same_name_diff_source".to_string(),
                            Some(format!(
                                "A different skill already uses id '{}'. Choose overwrite, rename, or skip.",
                                proposed
                            )),
                            src_row.map(|s| s.source_type),
                        )
                    }
                }
            };
            let hash = hash.unwrap_or_else(|| fsutil::hash_dir(&dir));
            let cand = AddCenterSkillCandidate {
                skill_id: proposed.clone(),
                proposed_skill_id: proposed.clone(),
                name: fm.name().unwrap_or(&proposed).to_string(),
                description: fm.description().to_string(),
                source_dir: dir.display().to_string(),
                hash,
                action: action.clone(),
                existing_source_type: existing_source,
                reason: reason.clone(),
            };
            if action == "blocked_same_name_diff_source" {
                blockers.push(cand);
            } else {
                candidates.push(cand);
            }
        }
        let _ = &center;
        Ok(AddCenterSkillPreview {
            candidates,
            blockers,
            unchanged_count,
            center_path: center.display().to_string(),
        })
    }

    pub fn execute_add_center_skill(
        &self,
        input: AddCenterSkillInput,
        decisions: Vec<AddCenterSkillDecision>,
    ) -> Result<AddCenterSkillResult, String> {
        let preview = self.preview_add_center_skill(input.clone())?;
        let center = self.center_path()?;
        std::fs::create_dir_all(&center).map_err(|e| format!("center mkdir: {}", e))?;
        let mut created = Vec::new();
        let mut updated = Vec::new();
        let mut skipped = Vec::new();

        // decision lookup
        let mut decision_map: BTreeMap<String, AddCenterSkillDecision> = BTreeMap::new();
        for d in &decisions {
            decision_map.insert(d.skill_id.clone(), d.clone());
        }
        // blockers require explicit decision
        for b in &preview.blockers {
            let dec = decision_map.get(&b.skill_id);
            match dec.map(|d| d.resolution.as_str()) {
                Some("skip") => {
                    skipped.push(b.skill_id.clone());
                    continue;
                }
                Some("update") => {
                    // treat as overwrite
                    self.write_skill_to_center(b, &center, input.clone())?;
                    updated.push(b.skill_id.clone());
                }
                Some("create") => {
                    // rename
                    let new_id = decision_map
                        .get(&b.skill_id)
                        .and_then(|d| d.proposed_skill_id.clone())
                        .unwrap_or_else(|| format!("{}-import", b.skill_id));
                    self.write_skill_to_center_renamed(b, &center, &new_id, input.clone())?;
                    created.push(new_id);
                }
                _ => {
                    return Err(format!(
                        "Blocked skill '{}' requires an explicit decision (overwrite/rename/skip).",
                        b.skill_id
                    ));
                }
            }
        }
        // candidates: create/update unless decision says skip
        for c in &preview.candidates {
            let dec = decision_map.get(&c.skill_id).map(|d| d.resolution.as_str());
            if dec == Some("skip") {
                skipped.push(c.skill_id.clone());
                continue;
            }
            if c.action == "update" {
                self.write_skill_to_center(c, &center, input.clone())?;
                updated.push(c.skill_id.clone());
            } else {
                // create — allow rename override
                if let Some(new_id) = decision_map
                    .get(&c.skill_id)
                    .and_then(|d| d.proposed_skill_id.clone())
                    .filter(|s| !s.is_empty())
                {
                    if new_id != c.skill_id {
                        self.write_skill_to_center_renamed(c, &center, &new_id, input.clone())?;
                        created.push(new_id);
                        continue;
                    }
                }
                self.write_skill_to_center(c, &center, input.clone())?;
                created.push(c.skill_id.clone());
            }
        }
        self.scan_center_into_db()?;
        self.refresh_snapshot_best_effort();
        Ok(AddCenterSkillResult {
            skill_ids: created,
            updated,
            skipped,
        })
    }

    fn write_skill_to_center(
        &self,
        cand: &AddCenterSkillCandidate,
        center: &Path,
        input: AddCenterSkillInput,
    ) -> Result<(), String> {
        let dest = center.join(&cand.skill_id);
        let src = Path::new(&cand.source_dir);
        // overwrite if exists
        if dest.exists() || dest.is_symlink() {
            fsutil::remove_path(&dest)?;
        }
        self.write_center_directory(src, &dest, &input)?;
        self.record_source_after_write(&cand.skill_id, &dest, src, input)?;
        Ok(())
    }

    fn write_skill_to_center_renamed(
        &self,
        cand: &AddCenterSkillCandidate,
        center: &Path,
        new_id: &str,
        input: AddCenterSkillInput,
    ) -> Result<(), String> {
        let dest = center.join(new_id);
        let src = Path::new(&cand.source_dir);
        if dest.exists() || dest.is_symlink() {
            fsutil::remove_path(&dest)?;
        }
        self.write_center_directory(src, &dest, &input)?;
        self.record_source_after_write(new_id, &dest, src, input)?;
        Ok(())
    }

    fn write_center_directory(
        &self,
        src: &Path,
        dest: &Path,
        input: &AddCenterSkillInput,
    ) -> Result<(), String> {
        match input.import_mode.as_deref().unwrap_or("copy") {
            "copy" => fsutil::copy_dir_recursive(src, dest),
            "link" => {
                if !is_local_folder_import(input) {
                    return Err(
                        "Link import is only available when importing a local folder.".to_string(),
                    );
                }
                if fsutil::try_symlink(src, dest)? {
                    Ok(())
                } else if self.effective_link_fail_policy()? == "copy" {
                    fsutil::copy_dir_recursive(src, dest)
                } else {
                    Err(format!(
                        "Could not create center symlink at {}.",
                        dest.display()
                    ))
                }
            }
            other => Err(format!("Unknown center import mode: {other}")),
        }
    }

    fn record_source_after_write(
        &self,
        skill_id: &str,
        dest: &Path,
        source_dir: &Path,
        input: AddCenterSkillInput,
    ) -> Result<(), String> {
        let fm = fsutil::read_frontmatter(dest);
        let hash = fsutil::hash_dir(dest);
        let now = db::now_iso();
        let source_uri = source_uri_for_candidate(&input, source_dir);
        self.db.transaction(|tx| {
            upsert_skill_full(
                tx,
                skill_id,
                fm.name().unwrap_or(skill_id),
                fm.description(),
                "skill",
                &dest.display().to_string(),
                &hash,
                &serde_json::to_value(&fm.map).map_err(|e| e.to_string())?,
                &now,
            )?;
            upsert_source(
                tx,
                skill_id,
                &input.source_type,
                source_uri.as_deref(),
                None,
                input.imported_from_agent.as_deref(),
                input.imported_from_path.as_deref(),
                "agentbro",
            )?;
            Ok(())
        })?;
        Ok(())
    }

    fn skill_row(&self, skill_id: &str) -> Result<Option<SkillRow>, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, name, description, skill_type, current_hash, center_path
                 FROM skills WHERE id = ?1",
                params![skill_id],
                |r| {
                    Ok(SkillRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        skill_type: r.get(3)?,
                        current_hash: r.get(4)?,
                        center_path: r.get(5)?,
                        source_type: None,
                        source_uri: None,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    // ── Delete center skill ───────────────────────────────────────

    pub fn preview_delete_center_skill(
        &self,
        skill_id: &str,
    ) -> Result<DeleteCenterSkillPreview, String> {
        self.preview_delete_center_skills(vec![skill_id.to_string()])
    }

    pub fn preview_delete_center_skills(
        &self,
        skill_ids: Vec<String>,
    ) -> Result<DeleteCenterSkillPreview, String> {
        let skill_ids = unique_skill_ids(skill_ids);
        if skill_ids.is_empty() {
            return Err("No skills selected".to_string());
        }
        let mut affected = Vec::new();
        for skill_id in &skill_ids {
            let _row = self
                .skill_row(skill_id)?
                .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
            let targets = self.targets_for_skill(skill_id)?;
            for t in &targets {
                let claim_count = self.count_claims(&t.id)?;
                affected.push(AffectedTarget {
                    target_id: t.id.clone(),
                    agent_id: t.agent_id.clone(),
                    display_name: agent_meta::display_name(&t.agent_id),
                    target_path: t.target_path.clone(),
                    mode: t.actual_mode.clone(),
                    claim_count,
                });
            }
        }
        let removable = affected.is_empty();
        let skill_id = skill_ids.first().cloned().unwrap_or_default();
        Ok(DeleteCenterSkillPreview {
            skill_id,
            skill_ids,
            affected_targets: affected,
            removable,
            warnings: Vec::new(),
        })
    }

    pub fn execute_delete_center_skill(
        &self,
        skill_id: &str,
        remove_linked: bool,
    ) -> Result<(), String> {
        let agents = self.execute_delete_center_skill_inner(skill_id, remove_linked)?;
        if !remove_linked {
            self.scan_agents(agents)?;
        }
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    pub fn execute_delete_center_skills(
        &self,
        skill_ids: Vec<String>,
        remove_linked: bool,
    ) -> Result<(), String> {
        let skill_ids = unique_skill_ids(skill_ids);
        if skill_ids.is_empty() {
            return Err("No skills selected".to_string());
        }
        let mut agents = Vec::new();
        for skill_id in skill_ids {
            agents.extend(self.execute_delete_center_skill_inner(&skill_id, remove_linked)?);
        }
        if !remove_linked {
            self.scan_agents(agents)?;
        }
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    fn execute_delete_center_skill_inner(
        &self,
        skill_id: &str,
        remove_linked: bool,
    ) -> Result<Vec<String>, String> {
        let preview = self.preview_delete_center_skill(skill_id)?;
        let row = self
            .skill_row(skill_id)?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let center_path = Path::new(&row.center_path);
        if remove_linked {
            for t in &preview.affected_targets {
                self.remove_target_completely(&t.target_id)?;
            }
        } else {
            for t in &preview.affected_targets {
                if t.mode == "link" {
                    let target_path = Path::new(&t.target_path);
                    fsutil::remove_path(target_path)?;
                    fsutil::copy_dir_recursive(center_path, target_path)?;
                }
            }
        }
        if center_path.exists() {
            fsutil::remove_path(center_path)?;
        }
        self.db.with_conn(|c| {
            c.execute("DELETE FROM skills WHERE id = ?1", params![skill_id])
                .map_err(|e| e.to_string())
        })?;
        Ok(preview
            .affected_targets
            .iter()
            .map(|t| t.agent_id.clone())
            .collect())
    }

    fn scan_agents(&self, mut agents: Vec<String>) -> Result<(), String> {
        agents.sort();
        agents.dedup();
        for agent in agents {
            self.scan_one_agent_into_db(&agent)?;
        }
        Ok(())
    }

    fn count_claims(&self, target_id: &str) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM skill_target_claims WHERE target_id = ?1",
                params![target_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }

    fn ensure_direct_claim_for_target(&self, target_id: &str) -> Result<(), String> {
        let now = db::now_iso();
        self.db.with_conn(|c| {
            let exists = c
                .query_row(
                    "SELECT 1 FROM skill_target_claims WHERE target_id = ?1 AND claim_type = 'direct'",
                    params![target_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .is_some();
            if exists {
                return Ok(());
            }
            c.execute(
                "INSERT INTO skill_target_claims(id, target_id, claim_type, pack_id, created_at)
                 VALUES (?1, ?2, 'direct', NULL, ?3)",
                params![format!("clm-{}", uuid_short()), target_id, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn delete_skill_target_distribution(&self, target_id: &str) -> Result<(), String> {
        let agent_id = self.db.with_conn(|c| {
            c.query_row(
                "SELECT agent_id FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())
        })?;
        let agent_id = agent_id.ok_or_else(|| format!("Target not found: {target_id}"))?;
        self.remove_target_completely(target_id)?;
        self.scan_one_agent_into_db(&agent_id)?;
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    pub fn delete_skill_target_distributions(
        &self,
        target_ids: Vec<String>,
    ) -> Result<DeleteSkillTargetDistributionsResult, String> {
        let mut deleted = 0usize;
        let mut failures = Vec::new();
        let mut affected_agents = BTreeSet::new();

        for target_id in target_ids {
            let agent_id = self.db.with_conn(|c| {
                c.query_row(
                    "SELECT agent_id FROM skill_targets WHERE id = ?1",
                    params![target_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())
            })?;
            let Some(agent_id) = agent_id else {
                failures.push(DeleteSkillTargetDistributionFailure {
                    target_id,
                    error: "Target not found".to_string(),
                });
                continue;
            };
            match self.remove_target_completely(&target_id) {
                Ok(()) => {
                    deleted += 1;
                    affected_agents.insert(agent_id);
                }
                Err(error) => {
                    failures.push(DeleteSkillTargetDistributionFailure { target_id, error })
                }
            }
        }

        for agent_id in affected_agents {
            self.scan_one_agent_into_db(&agent_id)?;
        }
        self.refresh_snapshot_best_effort();
        Ok(DeleteSkillTargetDistributionsResult { deleted, failures })
    }

    /// Remove a target's file/link AND all its DB rows (claims cascade).
    fn remove_target_completely(&self, target_id: &str) -> Result<(), String> {
        let target_path = self.db.with_conn(|c| {
            c.query_row(
                "SELECT target_path FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())
        })?;
        if let Some(p) = target_path {
            fsutil::remove_path(Path::new(&p))?;
        }
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM skill_targets WHERE id = ?1",
                params![target_id],
            )
            .map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    // ── Distribution (link/copy) ──────────────────────────────────

    pub fn preview_distribute_skill(
        &self,
        skill_ids: Vec<String>,
        target_agents: Vec<String>,
        requested_mode: String,
    ) -> Result<DistributionPreview, String> {
        if target_agents
            .iter()
            .any(|agent| agent == SHARED_SKILLS_AGENT_ID)
        {
            return Err(
                "The shared .agents skills directory cannot be selected as a distribution target."
                    .to_string(),
            );
        }
        let mut changes = Vec::new();
        let mut blockers = Vec::new();
        for skill_id in &skill_ids {
            let row = match self.skill_row(skill_id)? {
                Some(r) => r,
                None => {
                    for agent in &target_agents {
                        blockers.push(ConflictBlocker {
                            skill_id: skill_id.clone(),
                            agent_id: agent.clone(),
                            reason: format!("Skill '{}' is not in the center library.", skill_id),
                            existing_path: None,
                            existing_path_kind: None,
                            resolved_existing_path: None,
                        });
                    }
                    continue;
                }
            };
            for agent in &target_agents {
                let dir = match agent_meta::agent_skills_dir(&self.home, agent) {
                    Some(d) => d,
                    None => {
                        blockers.push(ConflictBlocker {
                            skill_id: skill_id.clone(),
                            agent_id: agent.clone(),
                            reason: format!("Agent '{}' has no known skills directory.", agent),
                            existing_path: None,
                            existing_path_kind: None,
                            resolved_existing_path: None,
                        });
                        continue;
                    }
                };
                let target_path = dir.join(skill_id);
                let existing = self.find_target_by_path(agent, &target_path)?;
                match existing {
                    Some((target_id, _, _)) => {
                        let actual = self.resolve_actual_mode(&requested_mode, agent)?;
                        let current_mode = self.target_actual_mode_for_id(&target_id)?;
                        if current_mode == "copy" && actual == "copy" {
                            let sync = self.preview_sync_copy_target(&target_id)?;
                            if matches!(sync.state.as_str(), "copy_modified" | "copy_diverged") {
                                let (existing_path_kind, resolved_existing_path) =
                                    existing_path_info(&target_path);
                                blockers.push(ConflictBlocker {
                                    skill_id: skill_id.clone(),
                                    agent_id: agent.clone(),
                                    reason: format!(
                                        "Managed copy '{}' has local changes. Choose whether the center library or the agent copy should win before redistributing.",
                                        skill_id
                                    ),
                                    existing_path: Some(target_path.display().to_string()),
                                    existing_path_kind,
                                    resolved_existing_path,
                                });
                                continue;
                            }
                        }
                        if current_mode != actual {
                            if current_mode == "copy" && actual == "link" {
                                let sync = self.preview_sync_copy_target(&target_id)?;
                                if matches!(sync.state.as_str(), "copy_modified" | "copy_diverged")
                                {
                                    let (existing_path_kind, resolved_existing_path) =
                                        existing_path_info(&target_path);
                                    blockers.push(ConflictBlocker {
                                        skill_id: skill_id.clone(),
                                        agent_id: agent.clone(),
                                        reason: format!(
                                            "Managed copy '{}' has local changes. Choose whether the center library or the agent copy should win before converting.",
                                            skill_id
                                        ),
                                        existing_path: Some(target_path.display().to_string()),
                                        existing_path_kind,
                                        resolved_existing_path,
                                    });
                                    continue;
                                }
                            }
                            changes.push(DistributionChange {
                                skill_id: skill_id.clone(),
                                agent_id: agent.clone(),
                                action: "convert".to_string(),
                                actual_mode: Some(actual),
                                reason: Some(format!(
                                    "Already managed as {} — will convert to {}.",
                                    current_mode, requested_mode
                                )),
                                target_path: target_path.display().to_string(),
                            });
                            continue;
                        }
                        changes.push(DistributionChange {
                            skill_id: skill_id.clone(),
                            agent_id: agent.clone(),
                            action: "reinstall".to_string(),
                            actual_mode: Some(current_mode),
                            reason: Some(
                                "Already managed — will refresh target from the center library."
                                    .to_string(),
                            ),
                            target_path: target_path.display().to_string(),
                        });
                    }
                    None => match inspect_path(&target_path) {
                        PathKind::Missing => {
                            let actual = self.resolve_actual_mode(&requested_mode, agent)?;
                            changes.push(DistributionChange {
                                skill_id: skill_id.clone(),
                                agent_id: agent.clone(),
                                action: "create".to_string(),
                                actual_mode: Some(actual),
                                reason: None,
                                target_path: target_path.display().to_string(),
                            });
                        }
                        _ => {
                            let (existing_path_kind, resolved_existing_path) =
                                existing_path_info(&target_path);
                            blockers.push(ConflictBlocker {
                                    skill_id: skill_id.clone(),
                                    agent_id: agent.clone(),
                                    reason: format!(
                                        "An unmanaged '{}' already exists at the target path. Adopt/overwrite/rename it first.",
                                        skill_id
                                    ),
                                    existing_path: Some(target_path.display().to_string()),
                                    existing_path_kind,
                                    resolved_existing_path,
                                });
                        }
                    },
                }
                let _ = &row;
            }
        }
        Ok(DistributionPreview {
            skill_ids,
            target_agents,
            requested_mode,
            changes,
            blockers,
            blocker_decisions: Vec::new(),
        })
    }

    fn target_actual_mode_for_id(&self, target_id: &str) -> Result<String, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT actual_mode FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
    }

    fn resolve_actual_mode(&self, requested: &str, agent: &str) -> Result<String, String> {
        // On non-unix or when link unsupported, fall back per policy.
        let can_link = cfg!(unix) && agent_meta::agent_skills_dir(&self.home, agent).is_some();
        if requested == "link" && !can_link {
            let policy = self.effective_link_fail_policy()?;
            match policy.as_str() {
                "copy" => return Ok("copy".to_string()),
                _ => return Err(
                    "Symlink is not available here; set link-fail policy to copy or choose copy."
                        .to_string(),
                ),
            }
        }
        Ok(requested.to_string())
    }

    fn effective_link_fail_policy(&self) -> Result<String, String> {
        if cfg!(target_os = "windows") {
            Ok("copy".to_string())
        } else {
            Ok(self.settings()?.link_fail_policy)
        }
    }

    pub fn execute_distribute_skill(
        &self,
        preview: DistributionPreview,
        claim_origin: ClaimOrigin,
    ) -> Result<DistributionPreview, String> {
        self.execute_distribute_skill_internal(preview, claim_origin, true)
    }

    fn execute_distribute_skill_internal(
        &self,
        preview: DistributionPreview,
        claim_origin: ClaimOrigin,
        refresh_after: bool,
    ) -> Result<DistributionPreview, String> {
        let mut result = preview.clone();
        if !preview.blockers.is_empty() && preview.blocker_decisions.is_empty() {
            return Err(format!(
                "{} blocker(s) prevent distribution. Resolve them first.",
                preview.blockers.len()
            ));
        }
        if !preview.blockers.is_empty() {
            let decision_by_target: HashMap<(String, String), String> = preview
                .blocker_decisions
                .iter()
                .map(|d| ((d.skill_id.clone(), d.agent_id.clone()), d.action.clone()))
                .collect();
            let mut remaining = Vec::new();
            for blocker in &preview.blockers {
                let key = (blocker.skill_id.clone(), blocker.agent_id.clone());
                let Some(action) = decision_by_target.get(&key) else {
                    remaining.push(blocker.clone());
                    continue;
                };
                match action.as_str() {
                    "skip" => {
                        result.changes.push(DistributionChange {
                            skill_id: blocker.skill_id.clone(),
                            agent_id: blocker.agent_id.clone(),
                            action: "skip".to_string(),
                            actual_mode: None,
                            reason: Some("Skipped by user decision.".to_string()),
                            target_path: blocker.existing_path.clone().unwrap_or_default(),
                        });
                    }
                    "overwrite" | "agent_over_center" => {
                        let existing_path = blocker.existing_path.as_ref().ok_or_else(|| {
                            format!(
                                "Cannot overwrite {}/{} because no target path was reported.",
                                blocker.skill_id, blocker.agent_id
                            )
                        })?;
                        self.skill_row(&blocker.skill_id)?.ok_or_else(|| {
                            format!("Skill '{}' is not in the center library.", blocker.skill_id)
                        })?;
                        let path = Path::new(existing_path);
                        self.ensure_distribution_target_path(
                            &blocker.skill_id,
                            &blocker.agent_id,
                            path,
                        )?;
                        let actual =
                            self.resolve_actual_mode(&preview.requested_mode, &blocker.agent_id)?;
                        if let Some((target_id, existing_skill_id, _)) =
                            self.find_target_by_path(&blocker.agent_id, path)?
                        {
                            if existing_skill_id != blocker.skill_id {
                                return Err(format!(
                                    "Target {} belongs to skill '{}', not '{}'.",
                                    existing_path, existing_skill_id, blocker.skill_id
                                ));
                            }
                            if action == "agent_over_center" {
                                self.execute_sync_copy_target(&target_id, "agent_over_center")?;
                            }
                            self.convert_target(
                                &target_id,
                                &blocker.skill_id,
                                &blocker.agent_id,
                                existing_path,
                                &preview.requested_mode,
                                &actual,
                            )?;
                            self.append_claim(
                                existing_path,
                                &blocker.agent_id,
                                claim_origin.clone(),
                            )?;
                        } else {
                            fsutil::remove_path(path)?;
                            self.create_target(
                                &blocker.skill_id,
                                &blocker.agent_id,
                                existing_path,
                                &preview.requested_mode,
                                &actual,
                                claim_origin.clone(),
                            )?;
                        }
                        result.changes.push(DistributionChange {
                            skill_id: blocker.skill_id.clone(),
                            agent_id: blocker.agent_id.clone(),
                            action: if action == "agent_over_center" {
                                "agent_over_center".to_string()
                            } else {
                                "overwrite".to_string()
                            },
                            actual_mode: Some(actual),
                            reason: Some(if action == "agent_over_center" {
                                "Used the agent copy as source, then converted by user decision."
                                    .to_string()
                            } else {
                                "Used the center library as source by user decision.".to_string()
                            }),
                            target_path: existing_path.clone(),
                        });
                    }
                    other => {
                        return Err(format!(
                            "Unknown distribution blocker decision '{}' for {}/{}.",
                            other, blocker.skill_id, blocker.agent_id
                        ));
                    }
                }
            }
            if !remaining.is_empty() {
                return Err(format!(
                    "{} blocker(s) still require an explicit decision.",
                    remaining.len()
                ));
            }
            result.blockers.clear();
        }
        for change in &preview.changes {
            match change.action.as_str() {
                "create" => {
                    let actual = change
                        .actual_mode
                        .clone()
                        .unwrap_or_else(|| preview.requested_mode.clone());
                    self.create_target(
                        &change.skill_id,
                        &change.agent_id,
                        &change.target_path,
                        &preview.requested_mode,
                        &actual,
                        claim_origin.clone(),
                    )?;
                }
                "reuse" => {
                    self.append_claim(&change.target_path, &change.agent_id, claim_origin.clone())?;
                }
                "convert" | "reinstall" => {
                    let actual = change
                        .actual_mode
                        .clone()
                        .unwrap_or_else(|| preview.requested_mode.clone());
                    let (target_id, existing_skill_id, _) = self
                        .find_target_by_path(&change.agent_id, Path::new(&change.target_path))?
                        .ok_or_else(|| format!("Target not found: {}", change.target_path))?;
                    if existing_skill_id != change.skill_id {
                        return Err(format!(
                            "Target {} belongs to skill '{}', not '{}'.",
                            change.target_path, existing_skill_id, change.skill_id
                        ));
                    }
                    self.convert_target(
                        &target_id,
                        &change.skill_id,
                        &change.agent_id,
                        &change.target_path,
                        &preview.requested_mode,
                        &actual,
                    )?;
                    self.append_claim(&change.target_path, &change.agent_id, claim_origin.clone())?;
                }
                _ => {}
            }
        }
        if refresh_after {
            self.scan_all_agents_into_db()?;
            self.refresh_snapshot_best_effort();
        }
        Ok(result)
    }

    fn create_target(
        &self,
        skill_id: &str,
        agent_id: &str,
        target_path: &str,
        requested_mode: &str,
        actual_mode: &str,
        origin: ClaimOrigin,
    ) -> Result<(), String> {
        let row = self
            .skill_row(skill_id)?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let center = Path::new(&row.center_path);
        let target = Path::new(target_path);
        self.ensure_distribution_target_path(skill_id, agent_id, target)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
        }
        let actual = match actual_mode {
            "link" => {
                let linked = fsutil::try_symlink(center, target)?;
                if linked {
                    "link".to_string()
                } else {
                    let policy = self.effective_link_fail_policy()?;
                    if policy == "copy" {
                        fsutil::copy_dir_recursive(center, target)?;
                        "copy".to_string()
                    } else {
                        return Err(format!(
                            "Could not create symlink at {}. Set link-fail policy to copy, or choose copy.",
                            target.display()
                        ));
                    }
                }
            }
            "copy" => {
                fsutil::copy_dir_recursive(center, target)?;
                "copy".to_string()
            }
            other => return Err(format!("Unknown mode: {other}")),
        };
        let source_hash = fsutil::hash_dir(center);
        let now = db::now_iso();
        let target_id = format!("tgt-{}", uuid_short());
        let current_hash = if actual == "copy" {
            Some(fsutil::hash_dir(target))
        } else {
            None
        };
        self.db.transaction(|tx| {
            tx.execute(
                "INSERT INTO skill_targets(id, skill_id, agent_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'ok', ?9, ?9)",
                params![target_id, skill_id, agent_id, target_path, requested_mode, actual, source_hash, current_hash, now],
            )
            .map_err(|e| e.to_string())?;
            let claim_type = match &origin {
                ClaimOrigin::Direct => "direct",
                ClaimOrigin::Pack(_) => "pack",
            };
            let pack_id = match &origin {
                ClaimOrigin::Pack(id) => Some(id.clone()),
                _ => None,
            };
            tx.execute(
                "INSERT OR IGNORE INTO skill_target_claims(id, target_id, claim_type, pack_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("clm-{}", uuid_short()),
                    target_id,
                    claim_type,
                    pack_id,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(())
    }

    fn convert_target(
        &self,
        target_id: &str,
        skill_id: &str,
        agent_id: &str,
        target_path: &str,
        requested_mode: &str,
        actual_mode: &str,
    ) -> Result<(), String> {
        let row = self
            .skill_row(skill_id)?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let center = Path::new(&row.center_path);
        let target = Path::new(target_path);
        self.ensure_distribution_target_path(skill_id, agent_id, target)?;
        fsutil::remove_path(target)?;
        let actual = match actual_mode {
            "link" => {
                let linked = fsutil::try_symlink(center, target)?;
                if linked {
                    "link".to_string()
                } else {
                    let policy = self.effective_link_fail_policy()?;
                    if policy == "copy" {
                        fsutil::copy_dir_recursive(center, target)?;
                        "copy".to_string()
                    } else {
                        return Err(format!(
                            "Could not create symlink at {}. Set link-fail policy to copy, or choose copy.",
                            target.display()
                        ));
                    }
                }
            }
            "copy" => {
                fsutil::copy_dir_recursive(center, target)?;
                "copy".to_string()
            }
            other => return Err(format!("Unknown mode: {other}")),
        };
        let source_hash = fsutil::hash_dir(center);
        let current_hash = if actual == "copy" {
            Some(fsutil::hash_dir(target))
        } else {
            None
        };
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE skill_targets
                 SET install_mode = ?1, actual_mode = ?2, source_hash = ?3, current_hash = ?4, status = 'ok', updated_at = ?5
                 WHERE id = ?6",
                params![requested_mode, actual, source_hash, current_hash, now, target_id],
            )
            .map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    fn ensure_distribution_target_path(
        &self,
        skill_id: &str,
        agent_id: &str,
        target: &Path,
    ) -> Result<(), String> {
        let skills_dir = agent_meta::agent_skills_dir(&self.home, agent_id)
            .ok_or_else(|| format!("Agent '{}' has no known skills directory.", agent_id))?;
        let parent = target.parent().ok_or_else(|| {
            format!(
                "Target path '{}' has no parent directory.",
                target.display()
            )
        })?;
        if target.file_name().and_then(|name| name.to_str()) != Some(skill_id) {
            return Err(format!(
                "Target path '{}' does not match skill '{}'.",
                target.display(),
                skill_id
            ));
        }
        if fsutil::normalized_path(parent) != fsutil::normalized_path(&skills_dir) {
            return Err(format!(
                "Target path '{}' must be a direct child of {}.",
                target.display(),
                skills_dir.display()
            ));
        }
        Ok(())
    }

    /// Append a claim to an existing target (idempotent).
    fn append_claim(
        &self,
        target_path: &str,
        agent_id: &str,
        origin: ClaimOrigin,
    ) -> Result<(), String> {
        let (target_id, skill_id) = self
            .db
            .with_conn(|c| {
                c.query_row(
                "SELECT id, skill_id FROM skill_targets WHERE target_path = ?1 AND agent_id = ?2",
                params![target_path, agent_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
            })?
            .ok_or_else(|| format!("Target not found: {target_path}"))?;
        let now = db::now_iso();
        let (claim_type, pack_id) = match &origin {
            ClaimOrigin::Direct => ("direct".to_string(), None),
            ClaimOrigin::Pack(id) => ("pack".to_string(), Some(id.clone())),
        };
        self.db.with_conn(|c| {
            // Direct claims are unique per target: SQLite treats NULL pack_id as
            // distinct in the UNIQUE index, so guard explicitly to avoid dupes.
            if claim_type == "direct" {
                let exists: bool = c
                    .query_row(
                        "SELECT 1 FROM skill_target_claims WHERE target_id = ?1 AND claim_type = 'direct'",
                        params![target_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                    .is_some();
                if exists {
                    return Ok(());
                }
            }
            c.execute(
                "INSERT OR IGNORE INTO skill_target_claims(id, target_id, claim_type, pack_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![format!("clm-{}", uuid_short()), target_id, claim_type, pack_id, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        let _ = skill_id;
        Ok(())
    }

    // ── Adopt unmanaged agent skill ───────────────────────────────

    pub fn preview_adopt_agent_skill(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
    ) -> Result<AdoptPreview, String> {
        let item = self.find_unmanaged(unmanaged_id)?;
        if item.read_only {
            return Err(format!(
                "Agent-provided built-in Skill '{}' is read-only and cannot be adopted or replaced.",
                item.inferred_skill_id.as_deref().unwrap_or(unmanaged_id)
            ));
        }
        let _path = Path::new(&item.path);
        let inferred = item.inferred_skill_id.clone().unwrap_or_default();
        let _center = self.center_path()?;
        let center_existing = self.skill_row(&inferred)?;
        let can_quick = match &center_existing {
            Some(row) => row.current_hash == item.hash.clone().unwrap_or_default(),
            None => true,
        };
        let mut quick_options = vec![
            AdoptOption {
                value: "import_keep".into(),
                label: "Import to center, keep agent file as-is".into(),
                destructive: false,
            },
            AdoptOption {
                value: "import_link".into(),
                label: "Import to center and replace agent file with link".into(),
                destructive: true,
            },
            AdoptOption {
                value: "import_copy".into(),
                label: "Import to center and replace agent file with copy".into(),
                destructive: true,
            },
        ];
        if agent_id == SHARED_SKILLS_AGENT_ID {
            quick_options = vec![AdoptOption {
                value: "import_cleanup".into(),
                label: "Import to center and remove the shared .agents copy".into(),
                destructive: true,
            }];
        }
        Ok(AdoptPreview {
            agent_id: agent_id.to_string(),
            unmanaged_id: unmanaged_id.to_string(),
            skill_path: item.path.clone(),
            inferred_skill_id: inferred.clone(),
            hash: item.hash.clone().unwrap_or_default(),
            center_has_same_id: center_existing.is_some(),
            can_quick_adopt: can_quick,
            options: if can_quick {
                quick_options
            } else {
                vec![
                    AdoptOption {
                        value: "center_over_agent".into(),
                        label: "Use center skill and replace agent file with link".into(),
                        destructive: true,
                    },
                    AdoptOption {
                        value: "overwrite_center".into(),
                        label: "Overwrite center skill with this one".into(),
                        destructive: true,
                    },
                    AdoptOption {
                        value: "rename".into(),
                        label: "Import under a new id".into(),
                        destructive: false,
                    },
                    AdoptOption {
                        value: "skip".into(),
                        label: "Keep as unmanaged".into(),
                        destructive: false,
                    },
                ]
            },
        })
    }

    pub fn execute_adopt_agent_skill(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
        option: &str,
        renamed_id: Option<String>,
    ) -> Result<String, String> {
        let skill_id =
            self.execute_adopt_agent_skill_inner(agent_id, unmanaged_id, option, renamed_id)?;
        if option != "skip" {
            self.scan_one_agent_into_db(agent_id)?;
            self.refresh_snapshot_best_effort();
        }
        Ok(skill_id)
    }

    pub fn execute_adopt_agent_skills(
        &self,
        items: Vec<AdoptBatchItem>,
    ) -> Result<AdoptBatchResult, String> {
        let mut affected_agents = BTreeSet::new();
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            if item.option != "skip" {
                affected_agents.insert(item.agent_id.clone());
            }
            match self.execute_adopt_agent_skill_inner(
                &item.agent_id,
                &item.unmanaged_id,
                &item.option,
                item.renamed_id,
            ) {
                Ok(skill_id) => results.push(AdoptBatchItemResult {
                    unmanaged_id: item.unmanaged_id,
                    skill_id: Some(skill_id),
                    error: None,
                }),
                Err(error) => results.push(AdoptBatchItemResult {
                    unmanaged_id: item.unmanaged_id,
                    skill_id: None,
                    error: Some(error),
                }),
            }
        }

        let needs_finalization = !affected_agents.is_empty();
        let mut finalization_errors = Vec::new();
        for agent_id in affected_agents {
            if let Err(error) = self.scan_one_agent_into_db(&agent_id) {
                finalization_errors.push(format!("{agent_id}: {error}"));
            }
        }
        if needs_finalization {
            self.refresh_snapshot_best_effort();
        }

        Ok(AdoptBatchResult {
            items: results,
            finalization_error: (!finalization_errors.is_empty())
                .then(|| finalization_errors.join("\n")),
        })
    }

    pub fn takeover_center_agent_skills(
        &self,
        agent_id: &str,
        unmanaged_ids: Vec<String>,
    ) -> Result<AdoptBatchResult, String> {
        let mut results = Vec::with_capacity(unmanaged_ids.len());
        let mut changed = false;
        for unmanaged_id in unmanaged_ids {
            match self.takeover_center_agent_skill_inner(agent_id, &unmanaged_id) {
                Ok(skill_id) => {
                    changed = true;
                    results.push(AdoptBatchItemResult {
                        unmanaged_id,
                        skill_id: Some(skill_id),
                        error: None,
                    });
                }
                Err(error) => results.push(AdoptBatchItemResult {
                    unmanaged_id,
                    skill_id: None,
                    error: Some(error),
                }),
            }
        }

        let finalization_error = if changed {
            let scan_error = self.scan_one_agent_into_db(agent_id).err();
            self.refresh_snapshot_best_effort();
            scan_error
        } else {
            None
        };

        Ok(AdoptBatchResult {
            items: results,
            finalization_error,
        })
    }

    fn takeover_center_agent_skill_inner(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
    ) -> Result<String, String> {
        let item = self.find_unmanaged(unmanaged_id)?;
        if item.agent_id.as_deref() != Some(agent_id) {
            return Err(format!(
                "Unmanaged item '{}' does not belong to agent '{}'.",
                unmanaged_id, agent_id
            ));
        }
        if item.item_type != "agent_skill" && item.item_type != "skill" {
            return Err("Only unmanaged skills can be taken over here.".to_string());
        }
        if item.reason != "same_name_as_center_skill" {
            return Err("The unmanaged skill is not present in the center library.".to_string());
        }

        let skill_id = item
            .inferred_skill_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "The unmanaged skill has no valid skill id.".to_string())?;
        let center = self
            .skill_row(&skill_id)?
            .ok_or_else(|| format!("Center skill '{}' no longer exists.", skill_id))?;
        let center_path = Path::new(&center.center_path);
        if !fsutil::is_skill_dir(center_path) {
            return Err(format!(
                "Center skill '{}' is unavailable on disk; the agent copy was preserved.",
                skill_id
            ));
        }

        let target_path = Path::new(&item.path);
        let allowed = agent_meta::agent_owned_skill_dirs(&self.home, agent_id)
            .into_iter()
            .any(|root| unmanaged_delete_path_allowed(&root, target_path));
        if !allowed {
            return Err(format!(
                "Refusing to take over unmanaged skill outside '{}' skill roots: {}",
                agent_id,
                target_path.display()
            ));
        }
        if !fsutil::is_skill_dir(target_path) {
            return Err(format!(
                "Agent skill '{}' is unavailable on disk.",
                target_path.display()
            ));
        }
        let normalized_center_path = fsutil::normalized_path(center_path);
        if fsutil::resolved_symlink_target(target_path)
            .is_some_and(|resolved| fsutil::normalized_path(&resolved) == normalized_center_path)
        {
            self.upsert_target_managed(agent_id, &skill_id, target_path, "link", "link")?;
            return Ok(skill_id);
        }
        if fsutil::normalized_path(target_path) == normalized_center_path {
            return Err("The agent path and center skill path are the same.".to_string());
        }

        let parent = target_path
            .parent()
            .ok_or_else(|| format!("Target path '{}' has no parent.", target_path.display()))?;
        let token = uuid_short();
        let pending_link = parent.join(format!(".agentbro-takeover-link-{token}"));
        let backup = parent.join(format!(".agentbro-takeover-backup-{token}"));
        if !fsutil::try_symlink(center_path, &pending_link)? {
            return Err(format!(
                "Could not create a symlink for '{}'; the agent copy was preserved.",
                skill_id
            ));
        }
        if let Err(error) = std::fs::rename(target_path, &backup) {
            let _ = fsutil::remove_path(&pending_link);
            return Err(format!(
                "Could not prepare '{}' for takeover: {}",
                target_path.display(),
                error
            ));
        }
        if let Err(error) = std::fs::rename(&pending_link, target_path) {
            let _ = fsutil::remove_path(&pending_link);
            let _ = std::fs::rename(&backup, target_path);
            return Err(format!(
                "Could not activate the symlink for '{}': {}",
                skill_id, error
            ));
        }
        if let Err(error) =
            self.upsert_target_managed(agent_id, &skill_id, target_path, "link", "link")
        {
            let _ = fsutil::remove_path(target_path);
            let _ = std::fs::rename(&backup, target_path);
            return Err(error);
        }
        let _ = fsutil::remove_path(&backup);
        Ok(skill_id)
    }

    fn execute_adopt_agent_skill_inner(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
        option: &str,
        renamed_id: Option<String>,
    ) -> Result<String, String> {
        let preview = self.preview_adopt_agent_skill(agent_id, unmanaged_id)?;
        if !preview.options.iter().any(|o| o.value == option) {
            return Err(format!(
                "Adopt option '{}' is not allowed for '{}'. Re-run preview and choose one of the suggested actions.",
                option,
                preview.inferred_skill_id
            ));
        }
        if option == "skip" {
            return Ok(preview.inferred_skill_id.clone());
        }
        let src = Path::new(&preview.skill_path);
        let center = self.center_path()?;
        std::fs::create_dir_all(&center).map_err(|e| format!("center mkdir: {}", e))?;
        let target_skill_id = match option {
            "rename" => renamed_id
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("{}-import", preview.inferred_skill_id)),
            _ => preview.inferred_skill_id.clone(),
        };
        let target_skill_id = fsutil::sanitize_id(&target_skill_id);
        if target_skill_id.is_empty() {
            return Err("Renamed skill id cannot be empty.".to_string());
        }

        // 1. import into center (copy), recording agent_import source
        let dest = center.join(&target_skill_id);
        let mut wrote_center = false;
        if dest.exists() {
            if option == "overwrite_center" {
                fsutil::remove_path(&dest)?;
            } else if option == "rename" {
                return Err(format!(
                    "A center skill already exists at '{}'.",
                    dest.display()
                ));
            } else if option == "center_over_agent" {
                // Keep the center copy intact; the target replacement below will
                // make the agent use this existing directory.
            } else if !(preview.center_has_same_id && preview.can_quick_adopt) {
                return Err(format!(
                    "A different center skill already exists at '{}'. Re-run preview and choose overwrite or rename.",
                    dest.display()
                ));
            } else {
                // Same id + same hash: reuse the existing center copy instead of
                // deleting and re-importing it.
            }
        }
        if !dest.exists() || option == "overwrite_center" {
            fsutil::copy_dir_recursive(src, &dest)?;
            wrote_center = true;
        }
        if wrote_center {
            let now = db::now_iso();
            let fm = fsutil::read_frontmatter(&dest);
            let hash = fsutil::hash_dir(&dest);
            let src_path_str = src.display().to_string();
            let dest_path_str = dest.display().to_string();
            self.db.transaction(|tx| {
                upsert_skill_full(
                    tx,
                    &target_skill_id,
                    fm.name().unwrap_or(&target_skill_id),
                    fm.description(),
                    "skill",
                    &dest_path_str,
                    &hash,
                    &serde_json::to_value(&fm.map).map_err(|e| e.to_string())?,
                    &now,
                )?;
                upsert_source(
                    tx,
                    &target_skill_id,
                    "agent_import",
                    None,
                    None,
                    Some(agent_id),
                    Some(src_path_str.as_str()),
                    "agentbro",
                )?;
                Ok(())
            })?;
        }

        // 2. optionally replace agent file
        match option {
            "import_keep" | "overwrite_center" | "rename" => {
                let actual_mode = existing_target_mode(src);
                self.upsert_target_managed(
                    agent_id,
                    &target_skill_id,
                    src,
                    actual_mode,
                    actual_mode,
                )?;
            }
            "import_link" => {
                fsutil::remove_path(src)?;
                let actual_mode = if fsutil::try_symlink(&dest, src)? {
                    "link"
                } else if self.effective_link_fail_policy()? == "copy" {
                    fsutil::copy_dir_recursive(&dest, src)?;
                    "copy"
                } else {
                    return Err(format!(
                        "Could not create symlink at {}. Set link-fail policy to copy, or choose copy.",
                        src.display()
                    ));
                };
                self.upsert_target_managed(agent_id, &target_skill_id, src, "link", actual_mode)?;
            }
            "center_over_agent" => {
                fsutil::remove_path(src)?;
                let actual_mode = if fsutil::try_symlink(&dest, src)? {
                    "link"
                } else if self.effective_link_fail_policy()? == "copy" {
                    fsutil::copy_dir_recursive(&dest, src)?;
                    "copy"
                } else {
                    return Err(format!(
                        "Could not create symlink at {}. Set link-fail policy to copy, or choose copy.",
                        src.display()
                    ));
                };
                self.upsert_target_managed(agent_id, &target_skill_id, src, "link", actual_mode)?;
            }
            "import_copy" => {
                fsutil::remove_path(src)?;
                fsutil::copy_dir_recursive(&dest, src)?;
                self.upsert_target_managed(agent_id, &target_skill_id, src, "copy", "copy")?;
            }
            "import_cleanup" => {
                if agent_id != SHARED_SKILLS_AGENT_ID {
                    return Err(
                        "Cleanup import is only available for shared .agents skills.".into(),
                    );
                }
                fsutil::remove_path(src)?;
            }
            _ => {}
        }

        // 3. clear the unmanaged record
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM unmanaged_items WHERE id = ?1",
                params![unmanaged_id],
            )
            .map_err(|e| e.to_string())
        })?;
        Ok(target_skill_id)
    }

    pub fn delete_unmanaged_agent_skill(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
    ) -> Result<(), String> {
        self.remove_unmanaged_agent_skill(agent_id, unmanaged_id)?;
        self.scan_one_agent_into_db(agent_id)?;
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    pub fn delete_unmanaged_agent_skills(
        &self,
        agent_id: &str,
        unmanaged_ids: Vec<String>,
    ) -> Result<DeleteUnmanagedAgentSkillsResult, String> {
        let mut deleted = 0usize;
        let mut failures = Vec::new();
        for unmanaged_id in unmanaged_ids {
            match self.remove_unmanaged_agent_skill(agent_id, &unmanaged_id) {
                Ok(()) => deleted += 1,
                Err(error) => failures.push(DeleteUnmanagedAgentSkillFailure {
                    unmanaged_id,
                    error,
                }),
            }
        }
        if deleted > 0 {
            self.scan_one_agent_into_db(agent_id)?;
            self.refresh_snapshot_best_effort();
        }
        Ok(DeleteUnmanagedAgentSkillsResult { deleted, failures })
    }

    fn remove_unmanaged_agent_skill(
        &self,
        agent_id: &str,
        unmanaged_id: &str,
    ) -> Result<(), String> {
        let item = self.find_unmanaged(unmanaged_id)?;
        if item.agent_id.as_deref() != Some(agent_id) {
            return Err(format!(
                "Unmanaged item '{}' does not belong to agent '{}'.",
                unmanaged_id, agent_id
            ));
        }
        if item.item_type != "agent_skill" && item.item_type != "skill" {
            return Err("Only unmanaged skills can be deleted here.".to_string());
        }
        if item.read_only {
            return Err(format!(
                "Agent-provided built-in Skill '{}' is read-only and cannot be deleted.",
                item.inferred_skill_id.as_deref().unwrap_or(unmanaged_id)
            ));
        }

        let path = Path::new(&item.path);
        let allowed = agent_meta::agent_owned_skill_dirs(&self.home, agent_id)
            .into_iter()
            .any(|root| unmanaged_delete_path_allowed(&root, path));
        if !allowed {
            return Err(format!(
                "Refusing to delete unmanaged skill outside '{}' skill roots: {}",
                agent_id,
                path.display()
            ));
        }

        fsutil::remove_path(path)?;
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM unmanaged_items WHERE id = ?1",
                params![unmanaged_id],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    fn upsert_target_managed(
        &self,
        agent_id: &str,
        skill_id: &str,
        target_path: &Path,
        install_mode: &str,
        actual_mode: &str,
    ) -> Result<(), String> {
        let center_hash = self
            .skill_row(skill_id)?
            .map(|r| r.current_hash)
            .unwrap_or_default();
        let now = db::now_iso();
        let current_hash = if actual_mode == "copy" {
            Some(fsutil::hash_dir(target_path))
        } else {
            None
        };
        self.db.transaction(|tx| {
            tx.execute(
                "INSERT INTO skill_targets(id, skill_id, agent_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'ok', ?9, ?9)
                 ON CONFLICT(skill_id, agent_id, target_path) DO UPDATE SET install_mode = excluded.install_mode, actual_mode = excluded.actual_mode, source_hash = excluded.source_hash, current_hash = excluded.current_hash, updated_at = excluded.updated_at",
                params![
                    format!("tgt-{}", uuid_short()),
                    skill_id,
                    agent_id,
                    target_path.display().to_string(),
                    install_mode,
                    actual_mode,
                    center_hash,
                    current_hash,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT OR IGNORE INTO skill_target_claims(id, target_id, claim_type, pack_id, created_at)
                 SELECT 'clm-' || ?1, id, 'direct', NULL, ?2 FROM skill_targets
                 WHERE skill_id = ?3 AND agent_id = ?4 AND target_path = ?5
                   AND NOT EXISTS (
                     SELECT 1 FROM skill_target_claims c
                     WHERE c.target_id = skill_targets.id AND c.claim_type = 'direct'
                   )",
                params![uuid_short(), now, skill_id, agent_id, target_path.display().to_string()],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(())
    }

    fn find_unmanaged(&self, unmanaged_id: &str) -> Result<UnmanagedItemDto, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, item_type, agent_id, path, inferred_skill_id, hash, reason
                 FROM unmanaged_items WHERE id = ?1",
                params![unmanaged_id],
                |r| {
                    Ok(UnmanagedItemDto {
                        id: r.get(0)?,
                        item_type: r.get(1)?,
                        agent_id: r.get(2)?,
                        path: r.get(3)?,
                        inferred_skill_id: r.get(4)?,
                        hash: r.get(5)?,
                        reason: r.get(6)?,
                        read_only: r.get::<_, String>(6)? == "agent_builtin_read_only",
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("SKILL_UNMANAGED_STALE:{unmanaged_id}"))
        })
    }

    // ── Copy sync (outdated / modified / diverged) ────────────────

    pub fn preview_sync_copy_target(&self, target_id: &str) -> Result<CopySyncPreview, String> {
        let (skill_id, target_path, source_hash, current_hash) =
            self.target_sync_inputs(target_id)?;
        let center = self.skill_row(&skill_id)?.ok_or("skill missing")?;
        // Compare against the LIVE center content on disk, not the possibly-stale
        // DB row — the center skill may have been edited since distribution.
        let center_hash = if Path::new(&center.center_path).is_dir() {
            fsutil::hash_dir(Path::new(&center.center_path))
        } else {
            center.current_hash.clone()
        };
        // Always read the copy's LIVE content from disk — the DB current_hash
        // may be stale if the copy was edited since the last distribution.
        let copy_hash = {
            let p = Path::new(&target_path);
            if p.is_dir() {
                fsutil::hash_dir(p)
            } else {
                current_hash.clone().unwrap_or_default()
            }
        };

        let center_changed = center_hash != source_hash;
        let copy_changed = copy_hash != source_hash;

        let (state, suggested) = if center_changed && copy_changed {
            ("copy_diverged".to_string(), "manual".to_string())
        } else if center_changed {
            ("copy_outdated".to_string(), "center_over_agent".to_string())
        } else if copy_changed {
            ("copy_modified".to_string(), "agent_over_center".to_string())
        } else {
            ("ok".to_string(), "none".to_string())
        };
        Ok(CopySyncPreview {
            target_id: target_id.to_string(),
            skill_id,
            target_path,
            source_hash,
            center_hash,
            copy_hash,
            state,
            suggested,
        })
    }

    fn target_sync_inputs(
        &self,
        target_id: &str,
    ) -> Result<(String, String, String, Option<String>), String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT skill_id, target_path, source_hash, current_hash FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|e| e.to_string())
        })
    }

    pub fn preview_copy_target_diff(
        &self,
        target_id: &str,
    ) -> Result<CopyTargetDiffPreview, String> {
        let sync = self.preview_sync_copy_target(target_id)?;
        let center = self
            .skill_row(&sync.skill_id)?
            .ok_or_else(|| format!("Skill not found: {}", sync.skill_id))?;
        let center_path = Path::new(&center.center_path);
        let copy_path = Path::new(&sync.target_path);
        if !center_path.is_dir() {
            return Err(format!(
                "Center path is not a directory: {}",
                center_path.display()
            ));
        }
        if !copy_path.is_dir() {
            return Err(format!(
                "Copy target is not a directory: {}",
                copy_path.display()
            ));
        }

        let center_files = collect_relative_files(center_path)?;
        let copy_files = collect_relative_files(copy_path)?;
        let all_files: BTreeSet<String> = center_files.union(&copy_files).cloned().collect();
        let mut files = Vec::new();
        for rel in all_files {
            let center_bytes = read_relative_file(center_path, &rel)?;
            let copy_bytes = read_relative_file(copy_path, &rel)?;
            if center_bytes == copy_bytes {
                continue;
            }
            let change_type = match (&center_bytes, &copy_bytes) {
                (Some(_), Some(_)) => "modified",
                (Some(_), None) => "copy_removed",
                (None, Some(_)) => "copy_added",
                (None, None) => continue,
            }
            .to_string();
            files.push(CopyTargetDiffFile {
                path: rel,
                change_type,
                center_content: center_bytes
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .map(String::from),
                copy_content: copy_bytes
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .map(String::from),
            });
        }

        Ok(CopyTargetDiffPreview {
            target_id: target_id.to_string(),
            skill_id: sync.skill_id,
            target_path: sync.target_path,
            center_path: center.center_path,
            state: sync.state,
            files,
        })
    }

    pub fn execute_sync_copy_target(
        &self,
        target_id: &str,
        action: &str,
    ) -> Result<CopySyncPreview, String> {
        let preview = self.preview_sync_copy_target(target_id)?;
        match action {
            "center_over_agent" => {
                let center = self.skill_row(&preview.skill_id)?;
                let center_path_str = center
                    .as_ref()
                    .map(|c| c.center_path.clone())
                    .unwrap_or_default();
                let center_path = Path::new(&center_path_str);
                let target = Path::new(&preview.target_path);
                fsutil::remove_path(target)?;
                fsutil::copy_dir_recursive(center_path, target)?;
                let now = db::now_iso();
                let new_source = fsutil::hash_dir(center_path);
                self.db.transaction(|tx| {
                    tx.execute(
                        "UPDATE skills SET current_hash = ?1, updated_at = ?2 WHERE id = ?3",
                        params![new_source, now, preview.skill_id],
                    )
                    .map_err(|e| e.to_string())?;
                    tx.execute(
                        "UPDATE skill_targets SET source_hash = ?1, current_hash = ?2, status = 'ok', updated_at = ?3 WHERE id = ?4",
                        params![new_source, fsutil::hash_dir(target), now, target_id],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })?;
            }
            "agent_over_center" => {
                let center = self.skill_row(&preview.skill_id)?;
                let center_path_str = center
                    .as_ref()
                    .map(|c| c.center_path.clone())
                    .unwrap_or_default();
                let center_path = Path::new(&center_path_str);
                let target = Path::new(&preview.target_path);
                fsutil::remove_path(center_path)?;
                fsutil::copy_dir_recursive(target, center_path)?;
                let now = db::now_iso();
                let new_hash = fsutil::hash_dir(center_path);
                self.db.transaction(|tx| {
                    tx.execute(
                        "UPDATE skills SET current_hash = ?1, updated_at = ?2 WHERE id = ?3",
                        params![new_hash, now, preview.skill_id],
                    )
                    .map_err(|e| e.to_string())?;
                    tx.execute(
                        "UPDATE skill_targets SET source_hash = ?1, current_hash = ?2, status = 'ok', updated_at = ?3 WHERE id = ?4",
                        params![new_hash, new_hash, now, target_id],
                    )
                    .map_err(|e| e.to_string())?;
                    tx.execute(
                        "UPDATE skill_sources SET source_type = 'agent_override', updated_at = ?1 WHERE skill_id = ?2",
                        params![now, preview.skill_id],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })?;
            }
            "keep_diverged" | "none" => {
                // mark status as copy_diverged, no file writes
                let now = db::now_iso();
                self.db.with_conn(|c| {
                    c.execute(
                        "UPDATE skill_targets SET status = 'copy_diverged', updated_at = ?1 WHERE id = ?2",
                        params![now, target_id],
                    )
                    .map_err(|e| e.to_string())
                })?;
            }
            other => return Err(format!("Unknown sync action: {other}")),
        }
        let next = self.preview_sync_copy_target(target_id)?;
        self.refresh_snapshot_best_effort();
        Ok(next)
    }

    // ── Skill packs ───────────────────────────────────────────────

    pub fn list_skill_packs(&self) -> Result<Vec<SkillPackSummary>, String> {
        let packs = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT id, name, description, tags_json, revision FROM skill_packs WHERE id <> ?1 ORDER BY name COLLATE NOCASE")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([DEFAULT_SKILL_PACK_ID], |r| {
                    Ok(PackRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        tags_json: r.get(3)?,
                        revision: r.get(4)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;
        let mut out = vec![self.default_skill_pack_summary()?];
        for p in packs {
            let member_count = self.pack_member_count(&p.id)?;
            let applied_agent_count = self.pack_applied_agent_count(&p.id)?;
            let healthy = self.pack_members_healthy(&p.id)?;
            let sync = self.pack_sync_rollup(&p.id, p.revision)?;
            let tags: Vec<String> = serde_json::from_str(&p.tags_json).unwrap_or_default();
            out.push(SkillPackSummary {
                id: p.id,
                name: p.name,
                description: p.description,
                tags,
                member_count,
                applied_agent_count,
                healthy,
                revision: p.revision,
                sync_status: sync.status,
                pending_sync_count: sync.pending_count,
                failed_sync_count: sync.failed_count,
            });
        }
        Ok(out)
    }

    fn default_skill_pack_summary(&self) -> Result<SkillPackSummary, String> {
        Ok(SkillPackSummary {
            id: DEFAULT_SKILL_PACK_ID.to_string(),
            name: DEFAULT_SKILL_PACK_NAME.to_string(),
            description: DEFAULT_SKILL_PACK_DESCRIPTION.to_string(),
            tags: Vec::new(),
            member_count: self.center_skill_count()?,
            applied_agent_count: self.pack_applied_agent_count(DEFAULT_SKILL_PACK_ID)?,
            healthy: true,
            revision: 1,
            sync_status: "synced".to_string(),
            pending_sync_count: 0,
            failed_sync_count: 0,
        })
    }

    fn center_skill_count(&self) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get::<_, i64>(0))
                .map(|n| n as usize)
                .map_err(|e| e.to_string())
        })
    }

    fn pack_member_count(&self, pack_id: &str) -> Result<usize, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return self.center_skill_count();
        }
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM skill_pack_members WHERE pack_id = ?1",
                params![pack_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }
    fn pack_members_healthy(&self, pack_id: &str) -> Result<bool, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Ok(true);
        }
        // healthy if all member skills still exist in center
        self.db.with_conn(|c| {
            let missing: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM skill_pack_members m LEFT JOIN skills s ON s.id = m.skill_id
                     WHERE m.pack_id = ?1 AND s.id IS NULL",
                    params![pack_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            Ok(missing == 0)
        })
    }
    fn pack_applied_agent_count(&self, pack_id: &str) -> Result<usize, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT COUNT(DISTINCT t.agent_id) FROM skill_target_claims c
                 JOIN skill_targets t ON t.id = c.target_id
                 WHERE c.pack_id = ?1",
                params![pack_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }

    pub fn get_skill_pack_detail(&self, pack_id: &str) -> Result<SkillPackDetail, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Ok(SkillPackDetail {
                id: DEFAULT_SKILL_PACK_ID.to_string(),
                name: DEFAULT_SKILL_PACK_NAME.to_string(),
                description: DEFAULT_SKILL_PACK_DESCRIPTION.to_string(),
                tags: Vec::new(),
                members: self.pack_members(DEFAULT_SKILL_PACK_ID)?,
                applied_agents: self.pack_applied_agents(DEFAULT_SKILL_PACK_ID)?,
                revision: 1,
                sync_status: "synced".to_string(),
                pending_sync_count: 0,
                failed_sync_count: 0,
                created_at: String::new(),
                updated_at: String::new(),
            });
        }
        let row = self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, name, description, tags_json, revision, created_at, updated_at FROM skill_packs WHERE id = ?1",
                params![pack_id],
                |r| Ok(PackDetailRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    tags_json: r.get(3)?,
                    revision: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                }),
            )
            .optional()
            .map_err(|e| e.to_string())
        })?
        .ok_or_else(|| format!("Pack not found: {pack_id}"))?;

        let members = self.pack_members(&row.id)?;
        let applied = self.pack_applied_agents(&row.id)?;
        let sync = self.pack_sync_rollup(&row.id, row.revision)?;
        let tags: Vec<String> = serde_json::from_str(&row.tags_json).unwrap_or_default();
        Ok(SkillPackDetail {
            id: row.id,
            name: row.name,
            description: row.description,
            tags,
            members,
            applied_agents: applied,
            revision: row.revision,
            sync_status: sync.status,
            pending_sync_count: sync.pending_count,
            failed_sync_count: sync.failed_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn pack_members(&self, pack_id: &str) -> Result<Vec<PackMember>, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return self.db.with_conn(|c| {
                let mut stmt = c
                    .prepare("SELECT id, name FROM skills ORDER BY name COLLATE NOCASE, id")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for (idx, row) in rows.enumerate() {
                    let (skill_id, skill_name) = row.map_err(|e| e.to_string())?;
                    out.push(PackMember {
                        skill_id,
                        skill_name,
                        required: true,
                        sort_order: idx as i64,
                        missing: false,
                    });
                }
                Ok(out)
            });
        }
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT m.skill_id, m.required, m.sort_order, s.name
                     FROM skill_pack_members m LEFT JOIN skills s ON s.id = m.skill_id
                     WHERE m.pack_id = ?1 ORDER BY m.sort_order",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([pack_id], |r| {
                    let skill_id: String = r.get(0)?;
                    let required: i64 = r.get(1)?;
                    let sort_order: i64 = r.get(2)?;
                    let name: Option<String> = r.get(3)?;
                    let missing = name.is_none();
                    Ok(PackMember {
                        skill_id: skill_id.clone(),
                        skill_name: name.unwrap_or_else(|| skill_id.clone()),
                        required: required != 0,
                        sort_order,
                        missing,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })
    }

    fn pack_applied_agents(&self, pack_id: &str) -> Result<Vec<AppliedPackSummary>, String> {
        let agents: Vec<String> = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT DISTINCT t.agent_id FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id WHERE c.pack_id = ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([pack_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;
        let member_count = self.pack_member_count(pack_id).unwrap_or(0);
        let pack_name = self
            .pack_name(pack_id)
            .unwrap_or_else(|_| pack_id.to_string());
        let pack_revision = self.pack_revision(pack_id).unwrap_or(1);
        Ok(agents
            .into_iter()
            .map(|agent_id| {
                let state = self
                    .pack_agent_sync_state(pack_id, &agent_id, pack_revision)
                    .unwrap_or_else(|_| PackAgentSyncState {
                        synced_revision: pack_revision,
                        status: "synced".to_string(),
                        error: None,
                    });
                AppliedPackSummary {
                    pack_id: pack_id.to_string(),
                    pack_name: pack_name.clone(),
                    member_count,
                    agent_id: Some(agent_id.clone()),
                    display_name: Some(agent_meta::display_name(&agent_id)),
                    icon_key: Some(agent_meta::icon_key(&agent_id)),
                    pack_revision,
                    synced_revision: state.synced_revision,
                    sync_status: state.status,
                    sync_error: state.error,
                }
            })
            .collect())
    }

    fn pack_revision(&self, pack_id: &str) -> Result<i64, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Ok(1);
        }
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT revision FROM skill_packs WHERE id = ?1",
                params![pack_id],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())
        })
    }

    fn pack_sync_rollup(&self, pack_id: &str, revision: i64) -> Result<PackSyncRollup, String> {
        let agents = self.pack_applied_agent_ids(pack_id)?;
        let mut pending_count = 0usize;
        let mut failed_count = 0usize;
        let mut synced_count = 0usize;
        for agent_id in &agents {
            let state = self.pack_agent_sync_state(pack_id, agent_id, revision)?;
            match state.status.as_str() {
                "failed" => failed_count += 1,
                "synced" if state.synced_revision >= revision => synced_count += 1,
                _ => pending_count += 1,
            }
        }
        let status = if agents.is_empty() || synced_count == agents.len() {
            "synced"
        } else if failed_count > 0 && synced_count > 0 {
            "partial"
        } else if failed_count > 0 {
            "failed"
        } else {
            "pending"
        }
        .to_string();
        Ok(PackSyncRollup {
            status,
            pending_count,
            failed_count,
        })
    }

    fn pack_agent_sync_state(
        &self,
        pack_id: &str,
        agent_id: &str,
        revision: i64,
    ) -> Result<PackAgentSyncState, String> {
        let row = self.db.with_conn(|c| {
            c.query_row(
                "SELECT synced_revision, status, error FROM skill_pack_agent_syncs
                 WHERE pack_id = ?1 AND agent_id = ?2",
                params![pack_id, agent_id],
                |r| {
                    Ok(PackAgentSyncState {
                        synced_revision: r.get(0)?,
                        status: r.get(1)?,
                        error: r.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
        })?;
        Ok(row.unwrap_or_else(|| PackAgentSyncState {
            synced_revision: revision,
            status: "synced".to_string(),
            error: None,
        }))
    }

    fn pack_applied_agent_ids(&self, pack_id: &str) -> Result<Vec<String>, String> {
        self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT DISTINCT t.agent_id FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id WHERE c.pack_id = ?1
                     ORDER BY t.agent_id",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([pack_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut agents = Vec::new();
            for row in rows {
                agents.push(row.map_err(|e| e.to_string())?);
            }
            Ok(agents)
        })
    }

    fn pack_name(&self, pack_id: &str) -> Result<String, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Ok(DEFAULT_SKILL_PACK_NAME.to_string());
        }
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT name FROM skill_packs WHERE id = ?1",
                params![pack_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
    }

    fn skill_name(&self, skill_id: &str) -> Result<String, String> {
        self.db.with_conn(|c| {
            c.query_row(
                "SELECT name FROM skills WHERE id = ?1",
                params![skill_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
    }

    fn affected_targets_for_pack(
        &self,
        pack_id: &str,
        agent_id: Option<&str>,
        skill_id: Option<&str>,
    ) -> Result<Vec<AffectedTarget>, String> {
        let mut sql = String::from(
            "SELECT DISTINCT t.id, t.agent_id, t.target_path, t.actual_mode
             FROM skill_target_claims c
             JOIN skill_targets t ON t.id = c.target_id
             WHERE c.pack_id = ?1",
        );
        if agent_id.is_some() {
            sql.push_str(" AND t.agent_id = ?2");
        }
        if skill_id.is_some() {
            sql.push_str(if agent_id.is_some() {
                " AND t.skill_id = ?3"
            } else {
                " AND t.skill_id = ?2"
            });
        }
        sql.push_str(" ORDER BY t.agent_id, t.target_path");

        let rows: Vec<(String, String, String, String)> = self.db.with_conn(|c| {
            let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            match (agent_id, skill_id) {
                (Some(agent), Some(skill)) => {
                    let rows = stmt
                        .query_map(params![pack_id, agent, skill], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })
                        .map_err(|e| e.to_string())?;
                    for r in rows {
                        out.push(r.map_err(|e| e.to_string())?);
                    }
                }
                (Some(agent), None) => {
                    let rows = stmt
                        .query_map(params![pack_id, agent], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })
                        .map_err(|e| e.to_string())?;
                    for r in rows {
                        out.push(r.map_err(|e| e.to_string())?);
                    }
                }
                (None, Some(skill)) => {
                    let rows = stmt
                        .query_map(params![pack_id, skill], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })
                        .map_err(|e| e.to_string())?;
                    for r in rows {
                        out.push(r.map_err(|e| e.to_string())?);
                    }
                }
                (None, None) => {
                    let rows = stmt
                        .query_map(params![pack_id], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })
                        .map_err(|e| e.to_string())?;
                    for r in rows {
                        out.push(r.map_err(|e| e.to_string())?);
                    }
                }
            }
            Ok(out)
        })?;

        rows.into_iter()
            .map(|(target_id, agent_id, target_path, mode)| {
                let claim_count = self.count_claims(&target_id)?;
                Ok(AffectedTarget {
                    target_id,
                    display_name: agent_meta::display_name(&agent_id),
                    agent_id,
                    target_path,
                    mode,
                    claim_count,
                })
            })
            .collect()
    }

    pub fn preview_delete_skill_pack(
        &self,
        pack_id: &str,
    ) -> Result<DeleteSkillPackPreview, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Ok(DeleteSkillPackPreview {
                pack_id: DEFAULT_SKILL_PACK_ID.to_string(),
                pack_name: DEFAULT_SKILL_PACK_NAME.to_string(),
                applied_agents: Vec::new(),
                affected_targets: Vec::new(),
                removable: false,
                warnings: vec!["全量技能包是系统内置入口，不能删除。".to_string()],
            });
        }
        let pack_name = self.pack_name(pack_id)?;
        let affected_targets = self.affected_targets_for_pack(pack_id, None, None)?;
        let mut applied_agents = affected_targets
            .iter()
            .map(|t| t.display_name.clone())
            .collect::<Vec<_>>();
        applied_agents.sort();
        applied_agents.dedup();
        let removable = affected_targets.is_empty();
        let warnings = if removable {
            vec![]
        } else {
            vec![format!(
                "Skill pack '{}' is still applied to {} agent(s). Revoke it before deleting.",
                pack_name,
                applied_agents.len()
            )]
        };
        Ok(DeleteSkillPackPreview {
            pack_id: pack_id.to_string(),
            pack_name,
            applied_agents,
            affected_targets,
            removable,
            warnings,
        })
    }

    pub fn preview_remove_pack_from_agent(
        &self,
        pack_id: &str,
        agent_id: &str,
    ) -> Result<RemovePackFromAgentPreview, String> {
        let pack_name = self.pack_name(pack_id)?;
        let affected_targets = self.affected_targets_for_pack(pack_id, Some(agent_id), None)?;
        let will_remove_targets = affected_targets
            .iter()
            .filter(|t| t.claim_count <= 1)
            .count();
        let will_preserve_targets = affected_targets.len().saturating_sub(will_remove_targets);
        Ok(RemovePackFromAgentPreview {
            pack_id: pack_id.to_string(),
            pack_name,
            agent_id: agent_id.to_string(),
            display_name: agent_meta::display_name(agent_id),
            affected_targets,
            will_remove_targets,
            will_preserve_targets,
        })
    }

    pub fn preview_remove_skill_from_pack(
        &self,
        pack_id: &str,
        skill_id: &str,
    ) -> Result<RemoveSkillFromPackPreview, String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Err("全量技能包始终包含中心库全部 Skills，不能编辑成员。".to_string());
        }
        let pack_name = self.pack_name(pack_id)?;
        let skill_name = self
            .skill_name(skill_id)
            .unwrap_or_else(|_| skill_id.to_string());
        let affected_targets = self.affected_targets_for_pack(pack_id, None, Some(skill_id))?;
        let mut agents = affected_targets
            .iter()
            .map(|t| t.agent_id.clone())
            .collect::<Vec<_>>();
        agents.sort();
        agents.dedup();
        Ok(RemoveSkillFromPackPreview {
            pack_id: pack_id.to_string(),
            pack_name,
            skill_id: skill_id.to_string(),
            skill_name,
            affected_targets,
            applied_agent_count: agents.len(),
            can_keep_standalone: true,
            can_remove_targets: true,
        })
    }

    fn direct_target_for_pack_move(
        &self,
        target_id: &str,
    ) -> Result<(String, String, String), String> {
        let target = self.db.with_conn(|c| {
            c.query_row(
                "SELECT skill_id, agent_id, target_path FROM skill_targets WHERE id = ?1",
                params![target_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
        })?;
        let target = target.ok_or_else(|| format!("Target not found: {target_id}"))?;
        let has_direct_claim = self.db.with_conn(|c| {
            c.query_row(
                "SELECT 1 FROM skill_target_claims
                 WHERE target_id = ?1 AND claim_type = 'direct' LIMIT 1",
                params![target_id],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(|e| e.to_string())
        })?;
        if !has_direct_claim {
            return Err(
                "Only directly distributed Skills can be moved into a skill pack.".to_string(),
            );
        }
        Ok(target)
    }

    pub fn preview_move_direct_skill_to_pack(
        &self,
        target_id: &str,
        pack_id: &str,
    ) -> Result<MoveDirectSkillToPackPreview, String> {
        let (skill_id, agent_id, _) = self.direct_target_for_pack_move(target_id)?;
        let pack = self.get_skill_pack_detail(pack_id)?;
        let already_member = pack
            .members
            .iter()
            .any(|member| member.skill_id == skill_id);
        let will_add_to_pack = pack_id != DEFAULT_SKILL_PACK_ID && !already_member;
        let other_skill_ids = pack
            .members
            .iter()
            .filter(|member| member.skill_id != skill_id)
            .map(|member| member.skill_id.clone())
            .collect::<Vec<_>>();
        let already_applied = self
            .pack_applied_agent_ids(pack_id)?
            .iter()
            .any(|id| id == &agent_id);
        let requested_mode = self.settings()?.default_distribute_mode;
        let distribution =
            self.preview_distribute_skill(Vec::new(), vec![agent_id.clone()], requested_mode)?;

        Ok(MoveDirectSkillToPackPreview {
            target_id: target_id.to_string(),
            skill_id: skill_id.clone(),
            skill_name: self.skill_name(&skill_id).unwrap_or(skill_id),
            agent_id: agent_id.clone(),
            display_name: agent_meta::display_name(&agent_id),
            pack_id: pack.id,
            pack_name: pack.name,
            already_member,
            already_applied,
            will_add_to_pack,
            other_member_count: other_skill_ids.len(),
            distribution,
        })
    }

    pub fn move_direct_skill_to_pack(
        &self,
        target_id: &str,
        pack_id: &str,
        _blocker_decisions: Vec<DistributionBlockerDecision>,
    ) -> Result<MoveDirectSkillToPackPreview, String> {
        let preview = self.preview_move_direct_skill_to_pack(target_id, pack_id)?;
        let (_, agent_id, target_path) = self.direct_target_for_pack_move(target_id)?;
        let previously_applied_agents = self.pack_applied_agent_ids(pack_id)?;
        let old_revision = self.pack_revision(pack_id)?;

        let next_revision = if preview.will_add_to_pack {
            let now = db::now_iso();
            self.db.transaction(|tx| {
                let next_order = tx
                    .query_row(
                        "SELECT COALESCE(MAX(sort_order), -1) + 1
                         FROM skill_pack_members WHERE pack_id = ?1",
                        params![pack_id],
                        |r| r.get::<_, i64>(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO skill_pack_members(pack_id, skill_id, sort_order, required)
                     VALUES (?1, ?2, ?3, 1)",
                    params![pack_id, preview.skill_id, next_order],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE skill_packs
                     SET revision = revision + 1, updated_at = ?1
                     WHERE id = ?2",
                    params![now, pack_id],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?;
            old_revision + 1
        } else {
            old_revision
        };

        self.append_claim(
            &target_path,
            &agent_id,
            ClaimOrigin::Pack(pack_id.to_string()),
        )?;
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM skill_target_claims
                 WHERE target_id = ?1 AND claim_type = 'direct'",
                params![target_id],
            )
            .map_err(|e| e.to_string())
        })?;

        if pack_id != DEFAULT_SKILL_PACK_ID {
            if preview.will_add_to_pack {
                let other_agents = previously_applied_agents
                    .into_iter()
                    .filter(|id| id != &agent_id)
                    .collect::<Vec<_>>();
                self.mark_pack_agents_pending(pack_id, &other_agents, old_revision)?;
                self.mark_pack_agent_status(pack_id, &agent_id, next_revision, "synced", None)?;
                if self.settings()?.auto_sync_skill_packs && !other_agents.is_empty() {
                    let _ = self.sync_skill_pack_to_agents(pack_id, other_agents);
                } else {
                    self.update_pack_rollup_status(pack_id, next_revision)?;
                }
            } else {
                self.mark_pack_agent_status(pack_id, &agent_id, next_revision, "synced", None)?;
                self.update_pack_rollup_status(pack_id, next_revision)?;
            }
        }

        if preview
            .distribution
            .changes
            .iter()
            .any(|change| change.action == "overwrite")
        {
            self.scan_one_agent_into_db(&agent_id)?;
        }
        self.refresh_snapshot_best_effort();
        Ok(preview)
    }

    pub fn upsert_skill_pack(&self, pack: UpsertPackInput) -> Result<SkillPackDetail, String> {
        self.upsert_skill_pack_with_sync(pack, true)
    }

    pub fn upsert_skill_pack_deferred(
        &self,
        pack: UpsertPackInput,
    ) -> Result<SkillPackDetail, String> {
        self.upsert_skill_pack_with_sync(pack, false)
    }

    fn upsert_skill_pack_with_sync(
        &self,
        pack: UpsertPackInput,
        sync_applied_agents: bool,
    ) -> Result<SkillPackDetail, String> {
        if pack.id == DEFAULT_SKILL_PACK_ID {
            return Err("全量技能包是系统内置入口，cannot be edited.".to_string());
        }
        if pack.name.trim().is_empty() {
            return Err("Pack name is required.".to_string());
        }
        // validate members exist
        for m in &pack.skill_ids {
            if !self.skill_id_known(m)? {
                return Err(format!("Pack member '{}' is not in the center library.", m));
            }
        }
        let existing = if pack.id.is_empty() {
            None
        } else {
            self.db.with_conn(|c| {
                c.query_row(
                    "SELECT revision FROM skill_packs WHERE id = ?1",
                    params![pack.id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| e.to_string())
            })?
        };
        let old_revision = existing.unwrap_or(0);
        let old_members = if existing.is_some() {
            self.pack_member_skill_ids(&pack.id)?
        } else {
            Vec::new()
        };
        let member_changed = existing.is_some() && old_members != pack.skill_ids;
        let applied_agents = if member_changed {
            self.pack_applied_agent_ids(&pack.id)?
        } else {
            Vec::new()
        };
        let id = if pack.id.is_empty() {
            format!("pack-{}", uuid_short())
        } else {
            pack.id.clone()
        };
        let next_revision = if member_changed {
            old_revision + 1
        } else {
            existing.unwrap_or(1).max(1)
        };
        let now = db::now_iso();
        let tags_json = serde_json::to_string(&pack.tags).map_err(|e| e.to_string())?;
        self.db.transaction(|tx| {
            tx.execute(
                "INSERT INTO skill_packs(id, name, description, tags_json, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, tags_json = excluded.tags_json, revision = excluded.revision, updated_at = excluded.updated_at",
                params![id, pack.name, pack.description, tags_json, next_revision, now],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM skill_pack_members WHERE pack_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
            for (idx, skill_id) in pack.skill_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO skill_pack_members(pack_id, skill_id, sort_order, required)
                     VALUES (?1, ?2, ?3, 1)",
                    params![id, skill_id, idx as i64],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })?;
        if member_changed && !applied_agents.is_empty() {
            self.mark_pack_agents_pending(&id, &applied_agents, old_revision)?;
            if sync_applied_agents && self.settings()?.auto_sync_skill_packs {
                let _ = self.sync_skill_pack_to_agents(&id, applied_agents);
            } else {
                self.update_pack_rollup_status(&id, next_revision)?;
            }
        }
        let detail = self.get_skill_pack_detail(&id)?;
        self.refresh_snapshot_best_effort();
        Ok(detail)
    }

    pub fn delete_skill_pack(&self, pack_id: &str) -> Result<(), String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Err("全量技能包是系统内置入口，不能删除。".to_string());
        }
        let applied = self.pack_applied_agent_count(pack_id)?;
        if applied > 0 {
            return Err(format!(
                "Pack '{}' is applied to {} agent(s). Revoke it from all agents first.",
                pack_id, applied
            ));
        }
        self.db.with_conn(|c| {
            c.execute("DELETE FROM skill_packs WHERE id = ?1", params![pack_id])
                .map(|_| ())
                .map_err(|e| e.to_string())
        })?;
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    pub fn sync_skill_pack_to_agents(
        &self,
        pack_id: &str,
        target_agents: Vec<String>,
    ) -> Result<SkillPackSyncResult, String> {
        let pack_name = self.pack_name(pack_id)?;
        let revision = self.pack_revision(pack_id)?;
        let agents = if target_agents.is_empty() {
            self.pack_applied_agent_ids(pack_id)?
        } else {
            target_agents
        };
        let mut results = Vec::new();
        for agent_id in agents {
            self.mark_pack_agent_status(pack_id, &agent_id, revision, "syncing", None)?;
            match self.sync_skill_pack_to_agent(pack_id, &agent_id) {
                Ok(()) => {
                    self.mark_pack_agent_status(pack_id, &agent_id, revision, "synced", None)?;
                    results.push(SkillPackSyncAgentResult {
                        agent_id: agent_id.clone(),
                        display_name: agent_meta::display_name(&agent_id),
                        status: "synced".to_string(),
                        error: None,
                    });
                }
                Err(error) => {
                    self.mark_pack_agent_status(
                        pack_id,
                        &agent_id,
                        revision.saturating_sub(1),
                        "failed",
                        Some(&error),
                    )?;
                    results.push(SkillPackSyncAgentResult {
                        agent_id: agent_id.clone(),
                        display_name: agent_meta::display_name(&agent_id),
                        status: "failed".to_string(),
                        error: Some(error),
                    });
                }
            }
        }
        let status = self.update_pack_rollup_status(pack_id, revision)?;
        self.refresh_snapshot_best_effort();
        Ok(SkillPackSyncResult {
            pack_id: pack_id.to_string(),
            pack_name,
            revision,
            status,
            agents: results,
        })
    }

    fn sync_skill_pack_to_agent(&self, pack_id: &str, agent_id: &str) -> Result<(), String> {
        let member_ids = self.pack_member_skill_ids(pack_id)?;
        let member_set: BTreeSet<String> = member_ids.iter().cloned().collect();
        let claimed_targets: Vec<(String, String, String, String)> = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT DISTINCT t.id, t.skill_id, t.target_path, t.status
                     FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id
                     WHERE c.pack_id = ?1 AND t.agent_id = ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![pack_id, agent_id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
            Ok(out)
        })?;

        let mut satisfied_member_ids = BTreeSet::new();
        for (target_id, skill_id, target_path, status) in claimed_targets {
            if member_set.contains(&skill_id) {
                if status == "ok"
                    && !matches!(
                        inspect_path(Path::new(&target_path)),
                        PathKind::Missing | PathKind::BrokenSymlink
                    )
                {
                    satisfied_member_ids.insert(skill_id);
                }
                continue;
            }

            self.db.with_conn(|c| {
                c.execute(
                    "DELETE FROM skill_target_claims WHERE target_id = ?1 AND pack_id = ?2",
                    params![target_id, pack_id],
                )
                .map_err(|e| e.to_string())
            })?;
            if self.count_claims(&target_id)? == 0 {
                self.remove_target_completely(&target_id)?;
            }
        }

        let changed_member_ids = member_ids
            .into_iter()
            .filter(|skill_id| !satisfied_member_ids.contains(skill_id))
            .collect::<Vec<_>>();
        if changed_member_ids.is_empty() {
            return Ok(());
        }

        let preview = self.preview_distribute_skill(
            changed_member_ids,
            vec![agent_id.to_string()],
            self.settings()?.default_distribute_mode,
        )?;
        if !preview.blockers.is_empty() {
            return Err(format!(
                "{} blocker(s) need manual resolution before syncing.",
                preview.blockers.len()
            ));
        }
        self.execute_distribute_skill_internal(
            preview,
            ClaimOrigin::Pack(pack_id.to_string()),
            false,
        )?;
        Ok(())
    }

    fn mark_pack_agents_pending(
        &self,
        pack_id: &str,
        agent_ids: &[String],
        synced_revision: i64,
    ) -> Result<(), String> {
        for agent_id in agent_ids {
            self.mark_pack_agent_status(pack_id, agent_id, synced_revision, "pending", None)?;
        }
        Ok(())
    }

    fn mark_pack_agents_synced(
        &self,
        pack_id: &str,
        agent_ids: &[String],
        synced_revision: i64,
    ) -> Result<(), String> {
        for agent_id in agent_ids {
            self.mark_pack_agent_status(pack_id, agent_id, synced_revision, "synced", None)?;
        }
        Ok(())
    }

    fn mark_pack_agent_status(
        &self,
        pack_id: &str,
        agent_id: &str,
        synced_revision: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO skill_pack_agent_syncs(pack_id, agent_id, synced_revision, status, error, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(pack_id, agent_id) DO UPDATE SET
                   synced_revision = excluded.synced_revision,
                   status = excluded.status,
                   error = excluded.error,
                   updated_at = excluded.updated_at",
                params![pack_id, agent_id, synced_revision, status, error, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    fn update_pack_rollup_status(&self, pack_id: &str, revision: i64) -> Result<String, String> {
        let rollup = self.pack_sync_rollup(pack_id, revision)?;
        let now = db::now_iso();
        let error = if rollup.failed_count > 0 {
            Some(format!("{} agent(s) failed to sync.", rollup.failed_count))
        } else {
            None
        };
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE skill_packs SET last_sync_status = ?1, last_sync_error = ?2, last_synced_at = ?3 WHERE id = ?4",
                params![rollup.status, error, now, pack_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(rollup.status)
    }

    fn pack_member_skill_ids(&self, pack_id: &str) -> Result<Vec<String>, String> {
        Ok(self
            .pack_members(pack_id)?
            .into_iter()
            .map(|member| member.skill_id)
            .collect())
    }

    pub fn apply_skill_pack(
        &self,
        pack_id: &str,
        target_agents: Vec<String>,
        requested_mode: String,
    ) -> Result<DistributionPreview, String> {
        self.apply_skill_pack_with_decisions(pack_id, target_agents, requested_mode, Vec::new())
    }

    pub fn apply_skill_pack_with_decisions(
        &self,
        pack_id: &str,
        target_agents: Vec<String>,
        requested_mode: String,
        blocker_decisions: Vec<DistributionBlockerDecision>,
    ) -> Result<DistributionPreview, String> {
        let detail = self.get_skill_pack_detail(pack_id)?;
        let skill_ids: Vec<String> = detail.members.iter().map(|m| m.skill_id.clone()).collect();
        let mut preview =
            self.preview_distribute_skill(skill_ids, target_agents, requested_mode)?;
        preview.blocker_decisions = blocker_decisions;
        if !preview.blockers.is_empty() && preview.blocker_decisions.is_empty() {
            return Ok(preview);
        }
        let result =
            self.execute_distribute_skill(preview, ClaimOrigin::Pack(pack_id.to_string()))?;
        if pack_id != DEFAULT_SKILL_PACK_ID && result.blockers.is_empty() {
            let revision = self.pack_revision(pack_id)?;
            self.mark_pack_agents_synced(pack_id, &result.target_agents, revision)?;
            self.update_pack_rollup_status(pack_id, revision)?;
        }
        Ok(result)
    }

    /// Revoke a pack from an agent: remove only the pack's claims; delete the
    /// target file only if no claims remain.
    pub fn remove_skill_pack_from_agent(
        &self,
        pack_id: &str,
        agent_id: &str,
    ) -> Result<RevokeResult, String> {
        let claims = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT c.id, c.target_id FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id
                     WHERE c.pack_id = ?1 AND t.agent_id = ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![pack_id, agent_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;

        let mut removed_claims = 0usize;
        let mut removed_targets = 0usize;
        let mut preserved_targets = 0usize;
        for (claim_id, target_id) in &claims {
            self.db.with_conn(|c| {
                c.execute(
                    "DELETE FROM skill_target_claims WHERE id = ?1",
                    params![claim_id],
                )
                .map_err(|e| e.to_string())
            })?;
            removed_claims += 1;
            let remaining = self.count_claims(target_id)?;
            if remaining == 0 {
                self.remove_target_completely(target_id)?;
                removed_targets += 1;
            } else {
                preserved_targets += 1;
            }
        }
        self.scan_one_agent_into_db(agent_id)?;
        self.refresh_snapshot_best_effort();
        Ok(RevokeResult {
            pack_id: pack_id.to_string(),
            agent_id: agent_id.to_string(),
            removed_claims,
            removed_targets,
            preserved_targets,
        })
    }

    pub fn remove_skill_from_pack(
        &self,
        pack_id: &str,
        skill_id: &str,
        also_remove_targets: bool,
    ) -> Result<(), String> {
        if pack_id == DEFAULT_SKILL_PACK_ID {
            return Err("全量技能包始终包含中心库全部 Skills，不能编辑成员。".to_string());
        }
        // Are there pack claims for this skill under this pack?
        let applied = self.pack_applied_agent_count(pack_id)?;
        if applied > 0 {
            // revoke this skill's pack claims across all agents
            let targets = self.db.with_conn(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT t.id, t.agent_id FROM skill_target_claims c
                         JOIN skill_targets t ON t.id = c.target_id
                         WHERE c.pack_id = ?1 AND t.skill_id = ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![pack_id, skill_id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(|e| e.to_string())?;
                let mut v = Vec::new();
                for r in rows {
                    v.push(r.map_err(|e| e.to_string())?);
                }
                Ok(v)
            })?;
            for (target_id, agent_id) in targets {
                if !also_remove_targets {
                    self.ensure_direct_claim_for_target(&target_id)?;
                }
                self.db.with_conn(|c| {
                    c.execute(
                        "DELETE FROM skill_target_claims WHERE target_id = ?1 AND pack_id = ?2",
                        params![target_id, pack_id],
                    )
                    .map_err(|e| e.to_string())
                })?;
                if also_remove_targets && self.count_claims(&target_id)? == 0 {
                    self.remove_target_completely(&target_id)?;
                }
                self.scan_one_agent_into_db(&agent_id)?;
            }
        }
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM skill_pack_members WHERE pack_id = ?1 AND skill_id = ?2",
                params![pack_id, skill_id],
            )
            .map_err(|e| e.to_string())
        })?;
        self.refresh_snapshot_best_effort();
        Ok(())
    }

    // ── Agent detail ──────────────────────────────────────────────

    pub fn get_agent_detail(&self, agent_id: &str) -> Result<AgentDetail, String> {
        if !agent_meta::visible_agent_ids()
            .iter()
            .any(|id| id == agent_id)
        {
            return Err(format!("Agent not found: {agent_id}"));
        }
        let persisted = self.db.with_conn(|c| {
            c.query_row(
                "SELECT version, latest_version, skills_dir FROM agents WHERE id = ?1",
                [agent_id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())
        })?;
        let (version, latest_version, persisted_skills_dir) =
            persisted.unwrap_or((None, None, None));
        let skills_dir = persisted_skills_dir.or_else(|| {
            agent_meta::agent_skills_dir(&self.home, agent_id)
                .map(|path| path.display().to_string())
        });

        let targets = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, skill_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at
                     FROM skill_targets WHERE agent_id = ?1 ORDER BY target_path",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([agent_id], |r| {
                    Ok(TargetRow {
                        id: r.get(0)?,
                        skill_id: r.get(1)?,
                        agent_id: agent_id.to_string(),
                        target_path: r.get(2)?,
                        install_mode: r.get(3)?,
                        actual_mode: r.get(4)?,
                        source_hash: r.get(5)?,
                        current_hash: r.get(6)?,
                        status: r.get(7)?,
                        created_at: r.get(8)?,
                        updated_at: r.get(9)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok::<_, String>(v)
        })?;

        let mut skills = Vec::new();
        for t in targets {
            let claims = self.claims_for_target(&t.id).unwrap_or_default();
            skills.push(SkillTargetDetail {
                id: t.id,
                skill_id: t.skill_id,
                agent_id: t.agent_id,
                resolved_target_path: resolved_target_path(&t.target_path),
                target_path: t.target_path,
                install_mode: t.install_mode,
                actual_mode: t.actual_mode,
                source_hash: t.source_hash,
                current_hash: t.current_hash,
                status: t.status,
                created_at: t.created_at,
                updated_at: t.updated_at,
                claims,
            });
        }
        let mut inherited_skills = self.inherited_skills_for_agent(agent_id)?;
        inherited_skills.sort_by(|left, right| {
            left.skill_id
                .to_lowercase()
                .cmp(&right.skill_id.to_lowercase())
        });
        let inherits_shared_skills = agent_meta::inherits_shared_agents_skills(agent_id);

        let applied_packs = self.applied_packs_for_agent(agent_id)?;
        let available_packs = self.list_skill_packs()?;
        let mcp_servers = crate::skills::v2::diagnosis::read_mcp_servers(self, agent_id);
        let plugins = crate::skills::v2::diagnosis::read_plugins(self, agent_id);
        let health = crate::skills::v2::diagnosis::agent_health(self, agent_id);
        let agent_paths = crate::skills::agent_paths::paths_for_agent(agent_id);
        let kimi_home = (agent_id == "kimi")
            .then(|| crate::skills::agent_paths::kimi_code_home_for(&self.home));
        let mcp_config_path = kimi_home
            .as_ref()
            .map(|home| home.join("mcp.json"))
            .or(agent_paths.mcp_config)
            .map(|path| path.display().to_string());
        let config_path = kimi_home
            .as_ref()
            .map(|home| home.join("config.toml"))
            .or_else(|| {
                crate::agents::profiles::profile_for_agent(agent_id).and_then(|profile| {
                    crate::agents::profiles::activation_url(&profile)
                        .or_else(|| Some(crate::agents::profiles::configuration_url(&profile)))
                })
            })
            .or(agent_paths.settings_file)
            .map(|path| path.display().to_string());
        let plugin_dir = kimi_home
            .as_ref()
            .map(|home| home.join("plugins").join("managed"))
            .or_else(|| crate::skills::agent_paths::plugin_cache_dir(agent_id))
            .map(|path| path.display().to_string());
        let agent_dir = kimi_home
            .as_ref()
            .map(|home| home.join("agents").display().to_string());

        Ok(AgentDetail {
            id: agent_id.to_string(),
            display_name: agent_meta::display_name(agent_id),
            icon_key: agent_meta::icon_key(agent_id),
            version,
            latest_version,
            skills_dir,
            config_path,
            mcp_config_path,
            plugin_dir,
            agent_dir,
            skills,
            inherits_shared_skills,
            inherited_skills,
            applied_packs,
            available_packs,
            mcp_servers,
            plugins,
            health,
        })
    }

    fn inherited_skills_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<InheritedSkillDetail>, String> {
        if !agent_meta::inherits_shared_agents_skills(agent_id) {
            return Ok(Vec::new());
        }
        let rows = self.db.with_conn(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT 'target:' || id, skill_id, target_path
                     FROM skill_targets
                     WHERE agent_id = ?1
                     UNION ALL
                     SELECT 'unmanaged:' || id, COALESCE(inferred_skill_id, ''), path
                     FROM unmanaged_items
                     WHERE agent_id = ?1 AND item_type IN ('skill', 'agent_skill')
                     ORDER BY 3",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([SHARED_SKILLS_AGENT_ID], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| error.to_string())?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row.map_err(|error| error.to_string())?);
            }
            Ok::<_, String>(values)
        })?;
        let mut seen_paths = BTreeSet::new();
        Ok(rows
            .into_iter()
            .filter(|(_, _, path)| seen_paths.insert(path.clone()))
            .map(|(id, skill_id, path)| InheritedSkillDetail {
                id,
                skill_id: if skill_id.trim().is_empty() {
                    infer_name_from_path(&path)
                } else {
                    skill_id
                },
                resolved_path: resolved_target_path(&path),
                path,
            })
            .collect())
    }

    fn applied_packs_for_agent(&self, agent_id: &str) -> Result<Vec<AppliedPackSummary>, String> {
        let pack_ids: Vec<String> = self.db.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT DISTINCT c.pack_id FROM skill_target_claims c
                     JOIN skill_targets t ON t.id = c.target_id
                     WHERE c.pack_id IS NOT NULL AND t.agent_id = ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([agent_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;
        let mut out = Vec::new();
        for pid in pack_ids {
            let name = self.pack_name(&pid).unwrap_or_else(|_| pid.clone());
            let member_count = self.pack_member_count(&pid).unwrap_or(0);
            let pack_revision = self.pack_revision(&pid).unwrap_or(1);
            let state = self
                .pack_agent_sync_state(&pid, agent_id, pack_revision)
                .unwrap_or_else(|_| PackAgentSyncState {
                    synced_revision: pack_revision,
                    status: "synced".to_string(),
                    error: None,
                });
            out.push(AppliedPackSummary {
                pack_id: pid,
                pack_name: name,
                member_count,
                agent_id: None,
                display_name: None,
                icon_key: None,
                pack_revision,
                synced_revision: state.synced_revision,
                sync_status: state.status,
                sync_error: state.error,
            });
        }
        Ok(out)
    }

    // ── DB handle for diagnosis/snapshot ──────────────────────────
    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
}

// ── helper structs & free functions ──────────────────────────────

#[derive(Debug, Clone)]
pub enum ClaimOrigin {
    Direct,
    Pack(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptPreview {
    pub agent_id: String,
    pub unmanaged_id: String,
    pub skill_path: String,
    pub inferred_skill_id: String,
    pub hash: String,
    pub center_has_same_id: bool,
    pub can_quick_adopt: bool,
    pub options: Vec<AdoptOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptOption {
    pub value: String,
    pub label: String,
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopySyncPreview {
    pub target_id: String,
    pub skill_id: String,
    pub target_path: String,
    pub source_hash: String,
    pub center_hash: String,
    pub copy_hash: String,
    pub state: String,
    pub suggested: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTargetDiffPreview {
    pub target_id: String,
    pub skill_id: String,
    pub target_path: String,
    pub center_path: String,
    pub state: String,
    pub files: Vec<CopyTargetDiffFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTargetDiffFile {
    pub path: String,
    pub change_type: String,
    pub center_content: Option<String>,
    pub copy_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeResult {
    pub pack_id: String,
    pub agent_id: String,
    pub removed_claims: usize,
    pub removed_targets: usize,
    pub preserved_targets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertPackInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentScanResult {
    pub managed: usize,
    pub unmanaged: usize,
    pub read_only: usize,
    pub included_shared: bool,
    pub shared_managed: usize,
    pub shared_unmanaged: usize,
    pub shared_read_only: usize,
}

struct SkillRow {
    id: String,
    name: String,
    description: String,
    skill_type: String,
    current_hash: String,
    center_path: String,
    source_type: Option<String>,
    source_uri: Option<String>,
}
struct TargetRow {
    id: String,
    skill_id: String,
    agent_id: String,
    target_path: String,
    install_mode: String,
    actual_mode: String,
    source_hash: String,
    current_hash: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct ProjectRow {
    id: String,
    name: String,
    root_path: String,
    created_at: String,
    updated_at: String,
    last_scanned_at: Option<String>,
}

fn resolved_target_path(path: &str) -> Option<String> {
    fsutil::resolved_symlink_target(Path::new(path)).map(|p| p.display().to_string())
}

fn existing_target_mode(path: &Path) -> &'static str {
    if fsutil::resolved_symlink_target(path).is_some() {
        "link"
    } else {
        "copy"
    }
}

fn unmanaged_delete_path_allowed(root: &Path, path: &Path) -> bool {
    let root = fsutil::normalized_path(root);
    if path.is_symlink() {
        return path
            .parent()
            .map(fsutil::normalized_path)
            .map(|parent| parent.starts_with(&root))
            .unwrap_or(false);
    }
    let path = fsutil::normalized_path(path);
    path.starts_with(&root) && path != root
}

fn filesystem_target_mode(path: &Path) -> Option<String> {
    match inspect_path(path) {
        PathKind::Symlink(_) => Some("link".to_string()),
        PathKind::Dir | PathKind::File => Some("copy".to_string()),
        PathKind::Missing | PathKind::BrokenSymlink => None,
    }
}

fn collect_relative_files(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    collect_relative_files_inner(root, root, &mut out)?;
    Ok(out)
}

fn collect_relative_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut BTreeSet<String>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("read dir {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if fsutil::is_ignored_entry(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if path.is_symlink() {
                continue;
            }
            collect_relative_files_inner(root, &path, out)?;
        } else if file_type.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

fn read_relative_file(root: &Path, rel: &str) -> Result<Option<Vec<u8>>, String> {
    let path = root.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    std::fs::read(&path)
        .map(Some)
        .map_err(|e| format!("read {}: {}", path.display(), e))
}

fn fixed_center_path(home: &Path) -> PathBuf {
    home.join(".agentbro").join("skills")
}

fn normalize_fixed_center_path(
    home: &Path,
    settings: &mut SkillManagerSettings,
) -> Option<PathBuf> {
    let configured = fsutil::normalized_path(&fsutil::expand_tilde(&settings.center_path));
    let fixed = fixed_center_path(home);
    if configured == fsutil::normalized_path(&fixed) {
        settings.center_path = fixed.to_string_lossy().to_string();
        return None;
    }
    settings.center_path = fixed.to_string_lossy().to_string();
    Some(configured)
}

fn migrate_center_skills_best_effort(source: &Path, target: &Path) {
    if fsutil::normalized_path(source) == fsutil::normalized_path(target) || !source.is_dir() {
        return;
    }
    if let Err(error) = std::fs::create_dir_all(target) {
        log::warn!(
            "Create fixed Skill center {} before migration failed: {}",
            target.display(),
            error
        );
        return;
    }
    let Ok(entries) = std::fs::read_dir(source) else {
        return;
    };
    for entry in entries.flatten() {
        let source_skill = entry.path();
        if !fsutil::is_skill_dir(&source_skill) {
            continue;
        }
        let target_skill = target.join(entry.file_name());
        if target_skill.exists() || target_skill.symlink_metadata().is_ok() {
            continue;
        }
        if let Err(error) = fsutil::copy_dir_recursive(&source_skill, &target_skill) {
            log::warn!(
                "Migrate Skill {} into fixed center failed: {}",
                source_skill.display(),
                error
            );
        }
    }
}

fn existing_path_info(path: &Path) -> (Option<String>, Option<String>) {
    match inspect_path(path) {
        PathKind::Symlink(target) => (
            Some("symlink".to_string()),
            Some(target.display().to_string()),
        ),
        PathKind::Dir => (Some("directory".to_string()), None),
        PathKind::File => (Some("file".to_string()), None),
        PathKind::BrokenSymlink => (Some("broken_symlink".to_string()), None),
        PathKind::Missing => (Some("missing".to_string()), None),
    }
}

struct PackRow {
    id: String,
    name: String,
    description: String,
    tags_json: String,
    revision: i64,
}
struct PackDetailRow {
    id: String,
    name: String,
    description: String,
    tags_json: String,
    revision: i64,
    created_at: String,
    updated_at: String,
}

struct PackSyncRollup {
    status: String,
    pending_count: usize,
    failed_count: usize,
}

struct PackAgentSyncState {
    synced_revision: i64,
    status: String,
    error: Option<String>,
}

// I had a bug: TargetRow query in get_agent_detail selects skill_id at col 1
// but I mapped it wrong. Fix by reading into a dedicated struct there.

pub fn uuid_short() -> String {
    let id = uuid::Uuid::new_v4().to_string();
    id.replace('-', "").chars().take(12).collect()
}

fn sanitize_for_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn infer_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(fsutil::sanitize_id)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "skill".to_string())
}

fn normalize_project_root(input: &str) -> Result<PathBuf, String> {
    let expanded = fsutil::expand_tilde(input);
    let normalized = fsutil::normalized_path(&expanded);
    if !normalized.is_dir() {
        return Err(format!(
            "Project path is not a directory: {}",
            normalized.display()
        ));
    }
    Ok(normalized)
}

fn unique_skill_ids(skill_ids: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for skill_id in skill_ids {
        if skill_id.trim().is_empty() {
            continue;
        }
        if seen.insert(skill_id.clone()) {
            unique.push(skill_id);
        }
    }
    unique
}

fn project_id_for_path(path: &Path) -> String {
    let normalized = fsutil::normalized_path(path);
    let mut hasher = Sha256::new();
    hasher.update(normalized.to_string_lossy().as_bytes());
    let digest = fsutil::hex_encode(&hasher.finalize());
    format!("project-{}", &digest[..16])
}

fn project_name_for_path(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn scan_project_skills(
    agent_id: &str,
    skills_dir: &Path,
    center_hashes: &HashMap<String, String>,
) -> Result<Vec<ProjectSkillItem>, String> {
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(skills_dir)
        .map_err(|e| format!("read project skills {}: {}", skills_dir.display(), e))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if fsutil::is_ignored_entry(&name) || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() || !fsutil::is_skill_dir(&path) {
            continue;
        }
        let id = fsutil::infer_skill_id(&path);
        let fm = fsutil::read_frontmatter(&path);
        let hash = fsutil::hash_dir(&path);
        let status = match center_hashes.get(&id) {
            Some(center_hash) if center_hash == &hash => "centerSynced",
            Some(_) => "centerDiff",
            None => "projectOnly",
        };
        out.push(ProjectSkillItem {
            id: id.clone(),
            name: fm.name().map(str::to_string).unwrap_or_else(|| id.clone()),
            description: fm.description().to_string(),
            agent_id: agent_id.to_string(),
            path: path.display().to_string(),
            hash,
            status: status.to_string(),
            importable: true,
        });
    }
    out.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

fn existing_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .collect()
}

fn project_instruction_files(root: &Path) -> Vec<ProjectInstructionFile> {
    let candidates = [
        ("codex", root.join("AGENTS.override.md")),
        ("codex", root.join("AGENTS.md")),
        ("claude-code", root.join(".claude").join("CLAUDE.md")),
        ("claude-code", root.join("CLAUDE.md")),
    ];
    candidates
        .into_iter()
        .filter_map(|(agent_id, path)| {
            if !path.is_file() {
                return None;
            }
            let bytes = std::fs::metadata(&path).ok().map(|m| m.len());
            Some(ProjectInstructionFile {
                agent_id: agent_id.to_string(),
                path: path.display().to_string(),
                exists: true,
                bytes,
            })
        })
        .collect()
}

fn project_agent_issue(agent_id: &str, kind: &str, path: &Path) -> ProjectHealthIssue {
    ProjectHealthIssue {
        agent_id: Some(agent_id.to_string()),
        kind: kind.to_string(),
        message: format!("{}: {}", kind.replace('_', " "), path.display()),
        severity: "warning".to_string(),
    }
}

fn project_agent_skills_dir(root: &Path, agent_id: &str) -> Result<PathBuf, String> {
    match agent_id {
        "claude-code" => Ok(root.join(".claude").join("skills")),
        "codex" => Ok(root.join(".agents").join("skills")),
        "kimi" => Ok(root.join(".kimi-code").join("skills")),
        other => Err(format!(
            "Project-level skills are not supported for {other} yet."
        )),
    }
}

fn read_json_mcp_servers_path(path: &Path) -> Vec<McpServerStatus> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let Some(servers) = json
        .get("mcpServers")
        .or_else(|| json.get("mcp_servers"))
        .and_then(|value| value.as_object())
    else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(name, cfg)| {
            let command = cfg
                .get("command")
                .and_then(|value| value.as_str())
                .or_else(|| cfg.get("url").and_then(|value| value.as_str()))
                .unwrap_or("")
                .to_string();
            let args = cfg
                .get("args")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let valid = !command.is_empty();
            McpServerStatus {
                name: name.clone(),
                command,
                args,
                valid,
                message: if valid {
                    "configured".to_string()
                } else {
                    "missing command".to_string()
                },
            }
        })
        .collect()
}

fn read_toml_mcp_servers_path(path: &Path) -> Vec<McpServerStatus> {
    crate::skills::codex_config::read_mcp_servers_path(path)
        .into_iter()
        .map(|server| {
            let valid = !server.command.is_empty();
            McpServerStatus {
                name: server.name,
                command: server.command,
                args: server.args,
                valid,
                message: if valid {
                    "configured".to_string()
                } else {
                    "missing command".to_string()
                },
            }
        })
        .collect()
}

fn read_claude_project_plugins_path(path: &Path) -> Vec<PluginStatus> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let Some(plugins) = json
        .get("enabledPlugins")
        .and_then(|value| value.as_object())
    else {
        return Vec::new();
    };
    plugins
        .iter()
        .filter_map(|(id, enabled)| {
            enabled.as_bool().map(|enabled| PluginStatus {
                id: id.clone(),
                name: id.clone(),
                version: None,
                enabled,
                source: Some("project-settings".to_string()),
            })
        })
        .collect()
}

fn read_codex_project_plugins_path(path: &Path) -> Vec<PluginStatus> {
    crate::skills::codex_config::read_project_plugins_path(path)
        .into_iter()
        .map(|plugin| PluginStatus {
            id: plugin.id.clone(),
            name: plugin.id,
            version: None,
            enabled: plugin.enabled,
            source: Some("project-config".to_string()),
        })
        .collect()
}

fn sources_match_for_candidate(
    input: &AddCenterSkillInput,
    candidate_dir: &Path,
    existing_type: &str,
    existing_uri: Option<&str>,
) -> bool {
    if input.source_type != existing_type {
        return false;
    }
    let candidate_uri = source_uri_for_candidate(input, candidate_dir);
    if candidate_uri.as_deref() == existing_uri {
        return true;
    }
    if input.source_type != "local_folder" {
        return false;
    }
    let Some(existing_uri) = existing_uri else {
        return false;
    };
    paths_refer_to_same_local_source(candidate_dir, existing_uri)
}

fn source_uri_for_candidate(input: &AddCenterSkillInput, candidate_dir: &Path) -> Option<String> {
    if input.source_type == "local_folder" && !is_remote_skill_source(input) {
        Some(candidate_dir.display().to_string())
    } else {
        input.source_uri.clone()
    }
}

fn center_import_mode_matches(
    input: &AddCenterSkillInput,
    center_path: &str,
    source_dir: &Path,
) -> bool {
    let center = Path::new(center_path);
    match input.import_mode.as_deref().unwrap_or("copy") {
        "link" => center.is_symlink() && paths_equal(center, source_dir),
        "copy" => !center.is_symlink(),
        _ => false,
    }
}

fn skill_directories_match(source: &Path, center: &Path, source_hash: &str) -> bool {
    if !center.is_dir() {
        return false;
    }
    if source.file_name() == center.file_name() {
        return source_hash == fsutil::hash_dir(center);
    }
    fsutil::hash_dir_contents(source) == fsutil::hash_dir_contents(center)
}

fn paths_refer_to_same_local_source(candidate_dir: &Path, existing_uri: &str) -> bool {
    let existing = Path::new(existing_uri.strip_prefix("file://").unwrap_or(existing_uri));
    paths_equal(candidate_dir, existing)
        || existing.starts_with(candidate_dir)
        || candidate_dir.starts_with(existing)
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    canonical_or_original(a) == canonical_or_original(b)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_remote_skill_source(input: &AddCenterSkillInput) -> bool {
    matches!(input.source_type.as_str(), "github" | "git" | "skillssh")
        || input.source_path.starts_with("github:")
        || input.source_path.starts_with("https://github.com/")
        || input.source_path.starts_with("http://github.com/")
}

fn is_local_folder_import(input: &AddCenterSkillInput) -> bool {
    input.source_type == "local_folder" && !is_remote_skill_source(input)
}

// transactional helpers
fn skill_exists(tx: &rusqlite::Transaction<'_>, skill_id: &str) -> Result<bool, String> {
    tx.query_row(
        "SELECT 1 FROM skills WHERE id = ?1",
        params![skill_id],
        |_| Ok(()),
    )
    .optional()
    .map_err(|e| e.to_string())
    .map(|o| o.is_some())
}

fn upsert_skill(
    tx: &rusqlite::Transaction<'_>,
    skill_id: &str,
    name: &str,
    description: &str,
    skill_type: &str,
    center_path: &str,
    hash: &str,
    frontmatter: &serde_json::Value,
) -> Result<(), String> {
    let now = db::now_iso();
    let fm_json = serde_json::to_string(frontmatter).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO skills(id, name, description, skill_type, center_path, current_hash, frontmatter_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, skill_type = excluded.skill_type, center_path = excluded.center_path, current_hash = excluded.current_hash, frontmatter_json = excluded.frontmatter_json, updated_at = excluded.updated_at",
        params![skill_id, name, description, skill_type, center_path, hash, fm_json, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_skill_full(
    tx: &rusqlite::Transaction<'_>,
    skill_id: &str,
    name: &str,
    description: &str,
    skill_type: &str,
    center_path: &str,
    hash: &str,
    frontmatter: &serde_json::Value,
    now: &str,
) -> Result<(), String> {
    let fm_json = serde_json::to_string(frontmatter).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO skills(id, name, description, skill_type, center_path, current_hash, frontmatter_json, created_at, updated_at, last_scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, skill_type = excluded.skill_type, center_path = excluded.center_path, current_hash = excluded.current_hash, frontmatter_json = excluded.frontmatter_json, updated_at = excluded.updated_at, last_scanned_at = excluded.last_scanned_at",
        params![skill_id, name, description, skill_type, center_path, hash, fm_json, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_source(
    tx: &rusqlite::Transaction<'_>,
    skill_id: &str,
    source_type: &str,
    source_uri: Option<&str>,
    source_ref: Option<&str>,
    imported_from_agent: Option<&str>,
    imported_from_path: Option<&str>,
    installed_via: &str,
) -> Result<(), String> {
    let now = db::now_iso();
    tx.execute(
        "INSERT INTO skill_sources(skill_id, source_type, source_uri, source_ref, imported_from_agent, imported_from_path, installed_via, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(skill_id) DO UPDATE SET source_type = excluded.source_type, source_uri = excluded.source_uri, source_ref = excluded.source_ref, imported_from_agent = excluded.imported_from_agent, imported_from_path = excluded.imported_from_path, installed_via = excluded.installed_via, updated_at = excluded.updated_at",
        params![skill_id, source_type, source_uri, source_ref, imported_from_agent, imported_from_path, installed_via, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// silence unused import warning when not needed

fn discover_agent_skill_paths(
    root: &Path,
    recursive: bool,
    include_dependency_dirs: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    discover_agent_skill_paths_inner(root, recursive, include_dependency_dirs, 0, &mut out)?;
    out.sort_by(|left, right| {
        let left_is_link = std::fs::symlink_metadata(left)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        let right_is_link = std::fs::symlink_metadata(right)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        left_is_link
            .cmp(&right_is_link)
            .then_with(|| left.cmp(right))
    });
    let mut seen = BTreeSet::new();
    out.retain(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.to_path_buf())));
    Ok(out)
}

fn discover_agent_skill_paths_inner(
    dir: &Path,
    recursive: bool,
    include_dependency_dirs: bool,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > 8 {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ignored =
            fsutil::is_ignored_entry(&name) && !(include_dependency_dirs && name == "node_modules");
        if ignored || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if fsutil::is_skill_dir(&path) {
            out.push(path);
            continue;
        }
        if recursive {
            discover_agent_skill_paths_inner(
                &path,
                recursive,
                include_dependency_dirs,
                depth + 1,
                out,
            )?;
        }
    }
    Ok(())
}

fn is_shared_agents_skill_path(home: &Path, path: &Path) -> bool {
    let shared = home.join(".agents").join("skills");
    path.starts_with(shared)
}

// ── ZIP extraction for local archive import ──────────────────────

fn cleanup_old_temp_imports() {
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("agentbro-skill-import-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("read zip: {}", e))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("mkdir: {}", e))?;
    // Canonicalize dest so the zip-slip guard is consistent even when temp_dir()
    // is behind a symlink (macOS /var → /private/var).
    let dest = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {}: {}", i, e))?;
        let entry_name = entry.name().to_string();
        // guard against path traversal (zip-slip): every component must stay under dest
        let outpath = dest.join(&entry_name);
        let mut cur = outpath.clone();
        let mut ok = false;
        for _ in 0..32 {
            match cur.parent() {
                Some(p) if p == dest => {
                    ok = true;
                    break;
                }
                Some(p) => cur = p.to_path_buf(),
                None => break,
            }
        }
        if !ok {
            continue;
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| format!("mkdir entry: {}", e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir parent: {}", e))?;
            }
            let mut outfile =
                std::fs::File::create(&outpath).map_err(|e| format!("create file: {}", e))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("copy: {}", e))?;
        }
    }
    Ok(())
}
