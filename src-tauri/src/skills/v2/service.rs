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
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Service {
    pub db: Arc<Db>,
    pub home: PathBuf,
}

impl Service {
    pub fn new(sqlite_path: &Path, home: PathBuf) -> Result<Self, String> {
        let db = Arc::new(Db::open(sqlite_path)?);
        Ok(Service { db, home })
    }

    pub fn center_path(&self) -> Result<PathBuf, String> {
        let s = self.settings()?;
        Ok(fsutil::expand_tilde(&s.center_path))
    }

    pub fn settings(&self) -> Result<SkillManagerSettings, String> {
        self.db.with_conn(|c| {
            let v = db::load_settings_json(c);
            if v.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                let def = SkillManagerSettings::default();
                // persist defaults so the file mirror + DB agree
                let val = serde_json::to_value(&def).map_err(|e| e.to_string())?;
                db::save_settings_json(c, &val)?;
                Ok(def)
            } else {
                serde_json::from_value(v).map_err(|e| e.to_string())
            }
        })
    }

    pub fn update_settings(&self, update: SettingsUpdate) -> Result<SkillManagerSettings, String> {
        let next = self.db.with_conn(|c| {
            // Read directly from this connection — calling self.settings() here
            // would re-lock the Mutex and self-deadlock.
            let raw = db::load_settings_json(c);
            let mut current: SkillManagerSettings =
                if raw.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    SkillManagerSettings::default()
                } else {
                    serde_json::from_value(raw).map_err(|e| e.to_string())?
                };
            if let Some(v) = update.center_path {
                current.center_path = v;
            }
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
            let val = serde_json::to_value(&current).map_err(|e| e.to_string())?;
            db::save_settings_json(c, &val)?;
            // mirror to file for human inspection
            let _ = std::fs::write(
                fsutil::settings_path(),
                serde_json::to_string_pretty(&val).unwrap_or_default(),
            );
            Ok(current)
        })?;
        let center = fsutil::expand_tilde(&next.center_path);
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
            self.scan_one_agent_into_db(agent_id)?;
        }
        Ok(())
    }

    pub fn scan_one_agent_into_db(&self, agent_id: &str) -> Result<AgentScanResult, String> {
        self.ensure_agent_row(agent_id)?;
        let skills_dir = match agent_meta::agent_skills_dir(&self.home, agent_id) {
            Some(d) => d,
            None => {
                return Ok(AgentScanResult {
                    managed: 0,
                    unmanaged: 0,
                });
            }
        };
        let now = db::now_iso();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE agents SET skills_dir = ?1, last_scanned_at = ?2 WHERE id = ?3",
                params![skills_dir.display().to_string(), now, agent_id],
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
        if skills_dir.is_dir() {
            let entries = std::fs::read_dir(&skills_dir).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if fsutil::is_ignored_entry(&name) || name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                if !path.is_dir() || !fsutil::is_skill_dir(&path) {
                    continue;
                }
                let inferred = fsutil::infer_skill_id(&path);
                // is there a managed target for this agent+path?
                let target = self.find_target_by_path(agent_id, &path)?;
                match target {
                    Some((target_id, skill_id, _)) => {
                        managed += 1;
                        self.refresh_target_status(&target_id, &skill_id, &path)?;
                    }
                    None => {
                        // is the inferred skill id known in center?
                        let center_known = self.skill_id_known(&inferred)?;
                        if center_known {
                            // same-name center skill exists — possible quick adopt
                            unmanaged += 1;
                            self.record_unmanaged(
                                agent_id,
                                &path,
                                &inferred,
                                Some(fsutil::hash_dir(&path)),
                                "same_name_as_center_skill",
                            )?;
                        } else {
                            unmanaged += 1;
                            self.record_unmanaged(
                                agent_id,
                                &path,
                                &inferred,
                                Some(fsutil::hash_dir(&path)),
                                "not_in_center_library",
                            )?;
                        }
                    }
                }
            }
        }
        Ok(AgentScanResult { managed, unmanaged })
    }

    fn ensure_agent_row(&self, agent_id: &str) -> Result<(), String> {
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
        Ok(())
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
        self.init_if_needed()?;
        let skills = self.list_center_skills()?;
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
            c.query_row("SELECT COUNT(*) FROM unmanaged_items", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
        })
    }

    pub fn list_center_skills(&self) -> Result<Vec<SkillSummary>, String> {
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
        let mut out = Vec::new();
        for id in agent_meta::managed_agent_ids() {
            self.ensure_agent_row(id)?;
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
            let installed = agent_meta::agent_installed(&self.home, id);
            let managed_skill_count = self.count_agent_targets(id)?;
            let unmanaged_skill_count = self.count_agent_unmanaged(id)?;
            out.push(AgentSummary {
                id: id.to_string(),
                display_name: agent_meta::display_name(id),
                icon_key: agent_meta::icon_key(id),
                enabled,
                skills_dir,
                version,
                latest_version: latest,
                installed,
                managed_skill_count,
                unmanaged_skill_count,
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
                "SELECT COUNT(*) FROM unmanaged_items WHERE agent_id = ?1",
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
        let agents = self.list_managed_agents()?;
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
            let (status, status_label, can_import) = if center_hash.is_none() {
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
                let unmanaged_count = items.iter().filter(|item| !item.managed).count();
                let importable_count = items.iter().filter(|item| item.can_import).count();
                AgentSkillInventoryAgent {
                    agent_id: agent.id,
                    display_name: agent.display_name,
                    icon_key: agent.icon_key,
                    skills_dir: agent.skills_dir,
                    installed: agent.installed,
                    managed_count,
                    unmanaged_count,
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
            let hash = fsutil::hash_dir(&dir);
            let existing = self.skill_row(&proposed)?;
            let (action, reason, existing_source) = match existing {
                None => ("create".to_string(), None, None),
                Some(row) => {
                    // same source?
                    let src_row = self.source_for_skill(&proposed)?;
                    let same_source = match &src_row {
                        Some(s) => sources_match(
                            &input.source_type,
                            input.source_uri.as_deref(),
                            &s.source_type,
                            s.source_uri.as_deref(),
                        ),
                        None => row.current_hash == hash,
                    };
                    if same_source {
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
        if dest.exists() {
            fsutil::remove_path(&dest)?;
        }
        fsutil::copy_dir_recursive(src, &dest)?;
        self.record_source_after_write(&cand.skill_id, &dest, input)?;
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
        if dest.exists() {
            fsutil::remove_path(&dest)?;
        }
        fsutil::copy_dir_recursive(src, &dest)?;
        self.record_source_after_write(new_id, &dest, input)?;
        Ok(())
    }

    fn record_source_after_write(
        &self,
        skill_id: &str,
        dest: &Path,
        input: AddCenterSkillInput,
    ) -> Result<(), String> {
        let fm = fsutil::read_frontmatter(dest);
        let hash = fsutil::hash_dir(dest);
        let now = db::now_iso();
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
                input.source_uri.as_deref(),
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
        let _row = self
            .skill_row(skill_id)?
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let targets = self.targets_for_skill(skill_id)?;
        let mut affected = Vec::new();
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
        let removable = affected.is_empty();
        Ok(DeleteCenterSkillPreview {
            skill_id: skill_id.to_string(),
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
        if !remove_linked {
            let mut agents = preview
                .affected_targets
                .iter()
                .map(|t| t.agent_id.clone())
                .collect::<Vec<_>>();
            agents.sort();
            agents.dedup();
            for agent in agents {
                self.scan_one_agent_into_db(&agent)?;
            }
        }
        self.refresh_snapshot_best_effort();
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
                            action: "reuse".to_string(),
                            actual_mode: Some(current_mode),
                            reason: Some(
                                "Already managed — will append a direct claim.".to_string(),
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
            let policy = self.settings()?.link_fail_policy;
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

    pub fn execute_distribute_skill(
        &self,
        preview: DistributionPreview,
        claim_origin: ClaimOrigin,
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
                "convert" => {
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
        self.scan_all_agents_into_db()?;
        self.refresh_snapshot_best_effort();
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
                    let policy = self.settings()?.link_fail_policy;
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
                    let policy = self.settings()?.link_fail_policy;
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
        let _path = Path::new(&item.path);
        let inferred = item.inferred_skill_id.clone().unwrap_or_default();
        let _center = self.center_path()?;
        let center_existing = self.skill_row(&inferred)?;
        let can_quick = match &center_existing {
            Some(row) => row.current_hash == item.hash.clone().unwrap_or_default(),
            None => true,
        };
        Ok(AdoptPreview {
            agent_id: agent_id.to_string(),
            unmanaged_id: unmanaged_id.to_string(),
            skill_path: item.path.clone(),
            inferred_skill_id: inferred.clone(),
            hash: item.hash.clone().unwrap_or_default(),
            center_has_same_id: center_existing.is_some(),
            can_quick_adopt: can_quick,
            options: if can_quick {
                vec![
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
                ]
            } else {
                vec![
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
        if !dest.exists() {
            fsutil::copy_dir_recursive(src, &dest)?;
            wrote_center = true;
        } else if option == "overwrite_center" {
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
                fsutil::try_symlink(&dest, src)?;
                self.upsert_target_managed(agent_id, &target_skill_id, src, "link", "link")?;
            }
            "import_copy" => {
                fsutil::remove_path(src)?;
                fsutil::copy_dir_recursive(&dest, src)?;
                self.upsert_target_managed(agent_id, &target_skill_id, src, "copy", "copy")?;
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
        self.scan_one_agent_into_db(agent_id)?;
        self.refresh_snapshot_best_effort();
        Ok(target_skill_id)
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
                    })
                },
            )
            .map_err(|e| e.to_string())
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
                .prepare("SELECT id, name, description, tags_json FROM skill_packs ORDER BY name COLLATE NOCASE")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(PackRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        tags_json: r.get(3)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            Ok(v)
        })?;
        let mut out = Vec::new();
        for p in packs {
            let member_count = self.pack_member_count(&p.id)?;
            let applied_agent_count = self.pack_applied_agent_count(&p.id)?;
            let healthy = self.pack_members_healthy(&p.id)?;
            let tags: Vec<String> = serde_json::from_str(&p.tags_json).unwrap_or_default();
            out.push(SkillPackSummary {
                id: p.id,
                name: p.name,
                description: p.description,
                tags,
                member_count,
                applied_agent_count,
                healthy,
            });
        }
        Ok(out)
    }

    fn pack_member_count(&self, pack_id: &str) -> Result<usize, String> {
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
        let row = self.db.with_conn(|c| {
            c.query_row(
                "SELECT id, name, description, tags_json, created_at, updated_at FROM skill_packs WHERE id = ?1",
                params![pack_id],
                |r| Ok(PackDetailRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    tags_json: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                }),
            )
            .optional()
            .map_err(|e| e.to_string())
        })?
        .ok_or_else(|| format!("Pack not found: {pack_id}"))?;

        let members = self.pack_members(&row.id)?;
        let applied = self.pack_applied_agents(&row.id)?;
        let tags: Vec<String> = serde_json::from_str(&row.tags_json).unwrap_or_default();
        Ok(SkillPackDetail {
            id: row.id,
            name: row.name,
            description: row.description,
            tags,
            members,
            applied_agents: applied,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn pack_members(&self, pack_id: &str) -> Result<Vec<PackMember>, String> {
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
        Ok(agents
            .into_iter()
            .map(|agent_id| AppliedPackSummary {
                pack_id: pack_id.to_string(),
                pack_name: pack_name.clone(),
                member_count,
                agent_id: Some(agent_id.clone()),
                display_name: Some(agent_meta::display_name(&agent_id)),
                icon_key: Some(agent_meta::icon_key(&agent_id)),
            })
            .collect())
    }

    fn pack_name(&self, pack_id: &str) -> Result<String, String> {
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

    pub fn upsert_skill_pack(&self, pack: UpsertPackInput) -> Result<SkillPackDetail, String> {
        if pack.name.trim().is_empty() {
            return Err("Pack name is required.".to_string());
        }
        // validate members exist
        for m in &pack.skill_ids {
            if !self.skill_id_known(m)? {
                return Err(format!("Pack member '{}' is not in the center library.", m));
            }
        }
        let id = if pack.id.is_empty() {
            format!("pack-{}", uuid_short())
        } else {
            pack.id.clone()
        };
        let now = db::now_iso();
        let tags_json = serde_json::to_string(&pack.tags).map_err(|e| e.to_string())?;
        self.db.transaction(|tx| {
            tx.execute(
                "INSERT INTO skill_packs(id, name, description, tags_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, tags_json = excluded.tags_json, updated_at = excluded.updated_at",
                params![id, pack.name, pack.description, tags_json, now],
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
        let detail = self.get_skill_pack_detail(&id)?;
        self.refresh_snapshot_best_effort();
        Ok(detail)
    }

    pub fn delete_skill_pack(&self, pack_id: &str) -> Result<(), String> {
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

    pub fn apply_skill_pack(
        &self,
        pack_id: &str,
        target_agents: Vec<String>,
        requested_mode: String,
    ) -> Result<DistributionPreview, String> {
        let detail = self.get_skill_pack_detail(pack_id)?;
        let skill_ids: Vec<String> = detail.members.iter().map(|m| m.skill_id.clone()).collect();
        let mut preview =
            self.preview_distribute_skill(skill_ids, target_agents, requested_mode)?;
        if !preview.blockers.is_empty() {
            return Ok(preview);
        }
        // execute with pack origin
        for change in preview.changes.clone() {
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
                        ClaimOrigin::Pack(pack_id.to_string()),
                    )?;
                }
                "reuse" => {
                    self.append_claim(
                        &change.target_path,
                        &change.agent_id,
                        ClaimOrigin::Pack(pack_id.to_string()),
                    )?;
                }
                _ => {}
            }
        }
        self.scan_all_agents_into_db()?;
        // mark changes as applied
        for c in preview.changes.iter_mut() {
            if c.action == "create" {
                c.reason = Some("pack claim created".to_string());
            } else if c.action == "reuse" {
                c.reason = Some("pack claim appended".to_string());
            }
        }
        self.refresh_snapshot_best_effort();
        Ok(preview)
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
        self.ensure_agent_row(agent_id)?;
        let summary = self
            .list_managed_agents()?
            .into_iter()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| format!("Agent not found: {agent_id}"))?;

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

        let applied_packs = self.applied_packs_for_agent(agent_id)?;
        let available_packs = self.list_skill_packs()?;
        let mcp_servers = crate::skills::v2::diagnosis::read_mcp_servers(self, agent_id);
        let plugins = crate::skills::v2::diagnosis::read_plugins(self, agent_id);
        let health = crate::skills::v2::diagnosis::agent_health(self, agent_id);
        let agent_paths = crate::skills::agent_paths::paths_for_agent(agent_id);
        let mcp_config_path = agent_paths.mcp_config.map(|p| p.display().to_string());
        let config_path = crate::agents::profiles::profile_for_agent(agent_id)
            .and_then(|profile| {
                crate::agents::profiles::activation_url(&profile)
                    .or_else(|| Some(crate::agents::profiles::configuration_url(&profile)))
            })
            .or(agent_paths.settings_file)
            .map(|p| p.display().to_string());
        let plugin_dir =
            crate::skills::agent_paths::plugin_cache_dir(agent_id).map(|p| p.display().to_string());

        Ok(AgentDetail {
            id: summary.id,
            display_name: summary.display_name,
            icon_key: summary.icon_key,
            version: summary.version,
            latest_version: summary.latest_version,
            skills_dir: summary.skills_dir,
            config_path,
            mcp_config_path,
            plugin_dir,
            skills,
            applied_packs,
            available_packs,
            mcp_servers,
            plugins,
            health,
        })
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
            out.push(AppliedPackSummary {
                pack_id: pid,
                pack_name: name,
                member_count,
                agent_id: None,
                display_name: None,
                icon_key: None,
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

fn filesystem_target_mode(path: &Path) -> Option<String> {
    match inspect_path(path) {
        PathKind::Symlink(_) => Some("link".to_string()),
        PathKind::Dir | PathKind::File => Some("copy".to_string()),
        PathKind::Missing | PathKind::BrokenSymlink => None,
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
}
struct PackDetailRow {
    id: String,
    name: String,
    description: String,
    tags_json: String,
    created_at: String,
    updated_at: String,
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

fn sources_match(a_type: &str, a_uri: Option<&str>, b_type: &str, b_uri: Option<&str>) -> bool {
    a_type == b_type && a_uri == b_uri
}

fn is_remote_skill_source(input: &AddCenterSkillInput) -> bool {
    matches!(input.source_type.as_str(), "github" | "git" | "skillssh")
        || input.source_path.starts_with("github:")
        || input.source_path.starts_with("https://github.com/")
        || input.source_path.starts_with("http://github.com/")
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
