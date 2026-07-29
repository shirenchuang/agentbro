//! Diagnosis engine — scans DB vs filesystem and produces actionable issues.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_question_mark)]

use crate::skills::v2::agent_meta;
use crate::skills::v2::db::now_iso;
use crate::skills::v2::fsutil::{self, inspect_path, PathKind};
use crate::skills::v2::models::*;
use crate::skills::v2::service::Service;
use rusqlite::{params, types::ValueRef, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn run(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let mut issues = Vec::new();
    // NOTE: do not call svc.refresh() here — init() already populated the DB and
    // every mutating op updates it. Refreshing on every overview/diagnosis call
    // re-scans every agent directory and makes the page feel laggy. The
    // DiagnosisPage exposes an explicit "run diagnosis" button that rescans.
    issues.extend(unmanaged_center_dirs(svc)?);
    issues.extend(unmanaged_agent_skills(svc)?);
    issues.extend(managed_shared_agent_skills(svc)?);
    issues.extend(broken_or_missing_targets(svc)?);
    issues.extend(copy_divergence_issues(svc)?);
    issues.extend(pack_member_missing(svc)?);
    issues.extend(orphan_claims(svc)?);
    issues.extend(snapshot_stale(svc)?);
    persist_issues(svc, &issues)?;
    Ok(issues)
}

fn info(
    id: &str,
    itype: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    entity: &str,
    eid: Option<String>,
) -> DiagnosisIssue {
    DiagnosisIssue {
        id: id.to_string(),
        issue_type: itype.to_string(),
        severity: "info".to_string(),
        fix_kind: "info".to_string(),
        title: title.into(),
        detail: detail.into(),
        entity_type: entity.to_string(),
        entity_id: eid,
        actions: vec![],
    }
}

fn auto(
    id: &str,
    itype: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    entity: &str,
    eid: Option<String>,
    action_label: &str,
) -> DiagnosisIssue {
    DiagnosisIssue {
        id: id.to_string(),
        issue_type: itype.to_string(),
        severity: "warning".to_string(),
        fix_kind: "auto".to_string(),
        title: title.into(),
        detail: detail.into(),
        entity_type: entity.to_string(),
        entity_id: eid,
        actions: vec![DiagnosisAction {
            id: format!("fix:{itype}"),
            label: action_label.to_string(),
            destructive: false,
        }],
    }
}

fn confirm(
    id: &str,
    itype: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    entity: &str,
    eid: Option<String>,
    action_label: &str,
    destructive: bool,
) -> DiagnosisIssue {
    DiagnosisIssue {
        id: id.to_string(),
        issue_type: itype.to_string(),
        severity: "error".to_string(),
        fix_kind: "confirm".to_string(),
        title: title.into(),
        detail: detail.into(),
        entity_type: entity.to_string(),
        entity_id: eid,
        actions: vec![DiagnosisAction {
            id: format!("fix:{itype}"),
            label: action_label.to_string(),
            destructive,
        }],
    }
}

fn unmanaged_center_dirs(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let center = svc.center_path()?;
    let mut out = Vec::new();
    if !center.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&center)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if fsutil::is_ignored_entry(&name)
            || name.starts_with('.')
            || name == "agentbro-skills.snapshot.json"
        {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() || !fsutil::is_skill_dir(&path) {
            continue;
        }
        let inferred = fsutil::infer_skill_id(&path);
        if !svc.db().with_conn(|c| skill_known(c, &inferred))? {
            out.push(info(
                &format!("center-unmanaged-{inferred}"),
                "center_unmanaged",
                "Center library directory not registered",
                format!(
                    "'{}' exists in the center library but is not tracked. Import it to manage it.",
                    name
                ),
                "skill",
                Some(inferred),
            ));
        }
    }
    Ok(out)
}

fn unmanaged_agent_skills(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let items = svc.list_unmanaged()?;
    let mut out = Vec::new();
    for it in items {
        if it.read_only {
            continue;
        }
        let display = it
            .agent_id
            .as_ref()
            .map(|a| agent_meta::display_name(a))
            .unwrap_or_default();
        out.push(info(
            &format!("agent-unmanaged-{}", it.id),
            "agent_unmanaged",
            format!("Unmanaged skill in {}", display),
            format!("{} — reason: {}", it.path, it.reason),
            "target",
            Some(it.id),
        ));
    }
    Ok(out)
}

fn managed_shared_agent_skills(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let rows: Vec<(String, String, String, String, String, String)> = svc.db().with_conn(|c| {
        let mut stmt = c
            .prepare(
                "SELECT t.id, t.skill_id, t.target_path, t.actual_mode, t.source_hash, s.center_path
                 FROM skill_targets t
                 JOIN skills s ON s.id = t.skill_id
                 WHERE t.agent_id = 'agents'
                 ORDER BY t.target_path",
            )
            .map_err(|e| e.to_string())?;
        let rs = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for r in rs {
            v.push(r.map_err(|e| e.to_string())?);
        }
        Ok(v)
    })?;

    let shared_dir = svc.home.join(".agents").join("skills");
    let mut out = Vec::new();
    for (id, skill_id, target_path, mode, source_hash, center_path) in rows {
        let target = Path::new(&target_path);
        if target
            .parent()
            .map(|parent| fsutil::normalized_path(parent) != fsutil::normalized_path(&shared_dir))
            .unwrap_or(true)
        {
            continue;
        }
        if fsutil::normalized_path(target) == fsutil::normalized_path(Path::new(&center_path)) {
            continue;
        }
        match inspect_path(target) {
            PathKind::Missing | PathKind::BrokenSymlink => continue,
            _ => {}
        }
        if mode == "copy" && target.is_dir() && fsutil::hash_dir(target) != source_hash {
            continue;
        }
        out.push(auto(
            &format!("agents-managed-duplicate-{id}"),
            "agents_managed_duplicate",
            "Managed skill still exists in shared .agents",
            format!(
                "Shared .agents skill '{}' is already managed from the center library as '{}' ({}). Remove the shared copy to avoid implicit Agent loading.",
                target_path, skill_id, mode
            ),
            "target",
            Some(id),
            "Remove .agents copy",
        ));
    }
    Ok(out)
}

fn broken_or_missing_targets(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let rows: Vec<(String, String, String, String)> = svc.db().with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT id, agent_id, target_path, actual_mode FROM skill_targets")
            .map_err(|e| e.to_string())?;
        let rs = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for r in rs {
            v.push(r.map_err(|e| e.to_string())?);
        }
        Ok(v)
    })?;
    let mut out = Vec::new();
    for (id, agent_id, target_path, mode) in rows {
        match inspect_path(Path::new(&target_path)) {
            PathKind::Missing => out.push(auto(
                &format!("target-missing-{id}"),
                "target_missing",
                "Stale target record",
                format!(
                    "{} target '{}' no longer exists on disk.",
                    agent_meta::display_name(&agent_id),
                    target_path
                ),
                "target",
                Some(id.clone()),
                "Remove record",
            )),
            PathKind::BrokenSymlink => out.push(auto(
                &format!("target-broken-{id}"),
                "broken_link",
                "Broken symlink",
                format!(
                    "{} link '{}' points at a missing skill.",
                    agent_meta::display_name(&agent_id),
                    target_path
                ),
                "target",
                Some(id.clone()),
                "Clean broken link",
            )),
            _ => {
                let _ = mode;
            }
        }
    }
    Ok(out)
}

fn copy_divergence_issues(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let rows: Vec<(String, String, String, String, Option<String>)> = svc.db().with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT id, skill_id, target_path, source_hash, current_hash FROM skill_targets WHERE actual_mode = 'copy'")
            .map_err(|e| e.to_string())?;
        let rs = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for r in rs {
            v.push(r.map_err(|e| e.to_string())?);
        }
        Ok(v)
    })?;
    let mut out = Vec::new();
    for (id, skill_id, target_path, source_hash, current) in rows {
        let center_hash = svc
            .db()
            .with_conn(|c| skill_hash(c, &skill_id))
            .unwrap_or_default();
        let copy_hash = current
            .or_else(|| {
                let p = Path::new(&target_path);
                if p.is_dir() {
                    Some(fsutil::hash_dir(p))
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let center_changed = center_hash != source_hash;
        let copy_changed = copy_hash != source_hash;
        if center_changed && copy_changed {
            out.push(confirm(
                &format!("copy-diverged-{id}"),
                "copy_diverged",
                "Copy diverged from center",
                format!(
                    "Both the center skill and the copy at '{}' changed. Choose how to reconcile.",
                    target_path
                ),
                "target",
                Some(id),
                "Reconcile copy",
                true,
            ));
        } else if center_changed {
            out.push(confirm(
                &format!("copy-outdated-{id}"),
                "copy_outdated",
                "Copy can be updated from center",
                format!("'{}' is behind the center library.", target_path),
                "target",
                Some(id.clone()),
                "Update copy",
                true,
            ));
        } else if copy_changed {
            out.push(confirm(
                &format!("copy-modified-{id}"),
                "copy_modified",
                "Copy was modified locally",
                format!("'{}' differs from the center snapshot.", target_path),
                "target",
                Some(id.clone()),
                "Push to center",
                true,
            ));
        }
    }
    Ok(out)
}

fn pack_member_missing(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let rows: Vec<(String, String)> = svc.db().with_conn(|c| {
        let mut stmt = c
            .prepare(
                "SELECT m.pack_id, m.skill_id FROM skill_pack_members m
                 LEFT JOIN skills s ON s.id = m.skill_id WHERE s.id IS NULL",
            )
            .map_err(|e| e.to_string())?;
        let rs = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for r in rs {
            v.push(r.map_err(|e| e.to_string())?);
        }
        Ok(v)
    })?;
    let mut out = Vec::new();
    for (pack_id, skill_id) in rows {
        out.push(info(
            &format!("pack-missing-{pack_id}-{skill_id}"),
            "pack_member_missing",
            "Pack member missing from center",
            format!(
                "Pack '{}' references skill '{}' which is no longer in the center library.",
                pack_id, skill_id
            ),
            "pack",
            Some(pack_id),
        ));
    }
    Ok(out)
}

fn orphan_claims(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let rows: Vec<(String, String)> = svc.db().with_conn(|c| {
        let mut stmt = c
            .prepare(
                "SELECT c.id, c.pack_id FROM skill_target_claims c
                 LEFT JOIN skill_targets t ON t.id = c.target_id
                 WHERE c.pack_id IS NOT NULL AND t.id IS NULL",
            )
            .map_err(|e| e.to_string())?;
        let rs = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for r in rs {
            v.push(r.map_err(|e| e.to_string())?);
        }
        Ok(v)
    })?;
    let mut out = Vec::new();
    for (claim_id, pack_id) in rows {
        out.push(auto(
            &format!("orphan-claim-{claim_id}"),
            "orphan_claim",
            "Orphan pack claim",
            format!(
                "Pack '{}' has a claim whose target no longer exists.",
                pack_id
            ),
            "target",
            Some(claim_id),
            "Remove orphan claim",
        ));
    }
    Ok(out)
}

fn snapshot_stale(svc: &Service) -> Result<Vec<DiagnosisIssue>, String> {
    let snap = crate::skills::v2::snapshot::snapshot_path(svc)?;
    if !snap.exists() {
        return Ok(vec![auto(
            "snapshot-missing",
            "snapshot_stale",
            "JSON snapshot missing",
            "The snapshot file does not exist yet. Refresh it for a human-readable backup.",
            "snapshot",
            None,
            "Refresh snapshot",
        )]);
    }
    // compare snapshot mtime to last DB write (max updated_at of skills)
    let last_db: Option<String> = svc
        .db()
        .with_conn(|c| {
            c.query_row("SELECT MAX(updated_at) FROM skills", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .map_err(|e| e.to_string())
        })
        .ok()
        .flatten();
    let snap_meta = std::fs::metadata(&snap).ok();
    if let (Some(meta), Some(db_when)) = (snap_meta, last_db) {
        let snap_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let db_secs = chrono::DateTime::parse_from_rfc3339(&db_when)
            .ok()
            .map(|t| t.timestamp().max(0) as u64)
            .unwrap_or(0);
        if db_secs > snap_mtime + 1 {
            return Ok(vec![auto(
                "snapshot-stale",
                "snapshot_stale",
                "JSON snapshot is out of date",
                "The center library changed since the snapshot was last written.",
                "snapshot",
                None,
                "Refresh snapshot",
            )]);
        }
    }
    Ok(vec![])
}

fn persist_issues(svc: &Service, issues: &[DiagnosisIssue]) -> Result<(), String> {
    let now = now_iso();
    svc.db().transaction(|tx| {
        // mark all prior unresolved as resolved, then re-insert current
        tx.execute("UPDATE diagnosis_issues SET resolved_at = ?1 WHERE resolved_at IS NULL", params![now])
            .map_err(|e| e.to_string())?;
        for iss in issues {
            let payload = serde_json::json!({
                "issueType": iss.issue_type,
                "entityId": iss.entity_id,
            });
            tx.execute(
                "INSERT INTO diagnosis_issues(id, issue_type, severity, entity_type, entity_id, title, detail, fix_kind, payload_json, created_at, resolved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
                 ON CONFLICT(id) DO UPDATE SET resolved_at = NULL, severity = excluded.severity, title = excluded.title, detail = excluded.detail",
                params![iss.id, iss.issue_type, iss.severity, iss.entity_type, iss.entity_id, iss.title, iss.detail, iss.fix_kind, payload.to_string(), now],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    Ok(())
}

fn skill_known(c: &rusqlite::Connection, skill_id: &str) -> Result<bool, String> {
    Ok(c.query_row(
        "SELECT 1 FROM skills WHERE id = ?1",
        params![skill_id],
        |_| Ok(()),
    )
    .ok()
    .is_some())
}

fn skill_hash(c: &rusqlite::Connection, skill_id: &str) -> Result<String, String> {
    let row: Result<String, rusqlite::Error> = c.query_row(
        "SELECT current_hash FROM skills WHERE id = ?1",
        params![skill_id],
        |r| {
            // current_hash is NOT NULL
            match r.get_ref(0) {
                Ok(ValueRef::Text(t)) => Ok(String::from_utf8_lossy(t).to_string()),
                _ => Ok(String::new()),
            }
        },
    );
    row.or(Ok(String::new()))
}

// ── Fix execution ─────────────────────────────────────────────────

pub fn execute_fix(svc: &Service, issue_type: &str, entity_id: &str) -> Result<(), String> {
    match issue_type {
        "broken_link" | "target_missing" => {
            let target_path: Option<String> = svc.db().with_conn(|c| {
                c.query_row(
                    "SELECT target_path FROM skill_targets WHERE id = ?1",
                    params![entity_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())
            })?;
            if let Some(path) = target_path {
                fsutil::remove_path(Path::new(&path))?;
            }
            svc.db().with_conn(|c| {
                c.execute(
                    "DELETE FROM skill_targets WHERE id = ?1",
                    params![entity_id],
                )
                .map_err(|e| e.to_string())
            })?;
            Ok(())
        }
        "orphan_claim" => {
            svc.db().with_conn(|c| {
                c.execute(
                    "DELETE FROM skill_target_claims WHERE id = ?1",
                    params![entity_id],
                )
                .map_err(|e| e.to_string())
            })?;
            Ok(())
        }
        "agents_managed_duplicate" => {
            svc.delete_skill_target_distribution(entity_id)?;
            Ok(())
        }
        "snapshot_stale" => {
            crate::skills::v2::snapshot::export_to_file(svc)?;
            Ok(())
        }
        _ => Err(format!("Issue type '{}' cannot be auto-fixed.", issue_type)),
    }
}

pub fn execute_safe_fixes(svc: &Service) -> Result<usize, String> {
    let issues = run(svc)?;
    let mut count = 0;
    for iss in issues {
        if iss.fix_kind == "auto" {
            if let Some(eid) = iss.entity_id {
                execute_fix(svc, &iss.issue_type, &eid)?;
                count += 1;
            } else if iss.issue_type == "snapshot_stale" {
                execute_fix(svc, "snapshot_stale", "")?;
                count += 1;
            }
        }
    }
    Ok(count)
}

// ── MCP / Plugin reads (best-effort) ──────────────────────────────

pub fn read_mcp_servers(svc: &Service, agent_id: &str) -> Vec<McpServerStatus> {
    let config = if agent_id == "kimi" {
        Some(crate::skills::agent_paths::kimi_code_home_for(&svc.home).join("mcp.json"))
    } else {
        crate::skills::agent_paths::paths_for_agent(agent_id).mcp_config
    };
    let Some(config) = config else {
        return vec![];
    };
    let Ok(content) = std::fs::read_to_string(&config) else {
        return vec![];
    };
    if config.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        return crate::skills::codex_config::parse_mcp_servers(&content)
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
            .collect();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    let servers = if agent_id == "zcode" {
        crate::skills::zcode_config::mcp_servers(&v)
    } else {
        v.get("mcpServers")
            .or_else(|| v.get("mcp_servers"))
            .and_then(|s| s.as_object())
    };
    let mut out = Vec::new();
    if let Some(servers) = servers {
        for (name, cfg) in servers {
            let command = cfg
                .get("command")
                .and_then(|c| c.as_str())
                .or_else(|| cfg.get("url").and_then(|url| url.as_str()))
                .unwrap_or("")
                .to_string();
            let args: Vec<String> = cfg
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let valid = !command.is_empty();
            out.push(McpServerStatus {
                name: name.clone(),
                command,
                args,
                valid,
                message: if valid {
                    "configured".to_string()
                } else {
                    "missing command".to_string()
                },
            });
        }
    }
    out
}

pub fn read_plugins(svc: &Service, agent_id: &str) -> Vec<PluginStatus> {
    if agent_id == "antigravity" {
        return read_antigravity_plugins(svc);
    }
    if agent_id == "workbuddy" {
        return read_workbuddy_plugins(svc);
    }
    if agent_id == "zcode" {
        return read_zcode_plugins(svc);
    }
    if agent_id == "kimi" {
        return read_kimi_plugins(svc);
    }

    let (cache, marker_dir, source_label, config_path) = match agent_id {
        "claude-code" => (
            svc.home.join(".claude/plugins/cache"),
            ".claude-plugin",
            "claude-plugin",
            Some(svc.home.join(".claude/settings.json")),
        ),
        "codex" => (
            svc.home.join(".codex/plugins/cache"),
            ".codex-plugin",
            "codex-plugin",
            Some(svc.home.join(".codex/config.toml")),
        ),
        _ => return vec![],
    };
    let enabled_plugins = config_path
        .as_ref()
        .map(|path| read_plugin_enabled_config(path))
        .unwrap_or_default();
    let mut out = HashMap::new();
    for manifest in plugin_manifests(&cache, marker_dir) {
        let Ok(content) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let id = v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let source = plugin_cache_source(&cache, &manifest);
        let config_key = source
            .as_deref()
            .map(|source| format!("{id}@{source}"))
            .unwrap_or_else(|| id.clone());
        if agent_id == "codex" && !enabled_plugins.contains_key(&config_key) {
            continue;
        }
        let enabled = enabled_plugins
            .get(&config_key)
            .or_else(|| enabled_plugins.get(&id))
            .copied()
            .unwrap_or(true);
        let status_id = if enabled_plugins.contains_key(&config_key) {
            config_key
        } else {
            id.clone()
        };
        let plugin = PluginStatus {
            id: status_id,
            name: v
                .pointer("/interface/displayName")
                .and_then(|n| n.as_str())
                .or_else(|| v.get("displayName").and_then(|n| n.as_str()))
                .or_else(|| v.get("name").and_then(|n| n.as_str()))
                .unwrap_or("")
                .to_string(),
            version: v.get("version").and_then(|n| n.as_str()).map(String::from),
            enabled,
            source: source
                .map(|source| format!("{source_label}:{source}"))
                .or_else(|| Some(source_label.to_string())),
        };
        let replace = out
            .get(&plugin.id)
            .and_then(|current: &PluginStatus| current.version.as_ref())
            < plugin.version.as_ref();
        if replace || !out.contains_key(&plugin.id) {
            out.insert(plugin.id.clone(), plugin);
        }
    }
    if agent_id == "codex" {
        for (id, enabled) in enabled_plugins {
            out.entry(id.clone()).or_insert_with(|| {
                let (name, marketplace) = split_plugin_config_key(&id);
                PluginStatus {
                    id,
                    name,
                    version: None,
                    enabled,
                    source: marketplace.map(|source| format!("codex-plugin:{source}")),
                }
            });
        }
    }
    let mut out = out.into_values().collect::<Vec<_>>();
    out.sort_by_key(|plugin| plugin.name.to_lowercase());
    out
}

#[derive(Debug, Clone)]
pub(crate) struct PluginLocation {
    pub root: PathBuf,
    pub manifest: Option<PathBuf>,
}

pub(crate) fn find_plugin_location(
    svc: &Service,
    agent_id: &str,
    plugin: &PluginStatus,
) -> Option<PluginLocation> {
    match agent_id {
        "claude-code" => find_manifest_plugin_location(
            &svc.home.join(".claude/plugins/cache"),
            ".claude-plugin",
            plugin,
        ),
        "codex" => find_manifest_plugin_location(
            &svc.home.join(".codex/plugins/cache"),
            ".codex-plugin",
            plugin,
        ),
        "workbuddy" => find_manifest_plugin_location(
            &svc.home.join(".workbuddy/plugins/marketplaces"),
            ".codebuddy-plugin",
            plugin,
        ),
        "zcode" => find_manifest_plugin_location(
            &svc.home.join(".zcode/cli/plugins/cache"),
            ".zcode-plugin",
            plugin,
        ),
        "kimi" => find_kimi_plugin_location(svc, plugin),
        "antigravity" => find_antigravity_plugin_location(svc, plugin),
        _ => None,
    }
}

fn find_manifest_plugin_location(
    cache: &Path,
    marker_dir: &str,
    plugin: &PluginStatus,
) -> Option<PluginLocation> {
    let (requested_id, requested_source) = split_plugin_config_key(&plugin.id);
    let manifest = plugin_manifests(cache, marker_dir)
        .into_iter()
        .filter_map(|manifest| {
            let info = read_plugin_manifest_info(cache, &manifest)?;
            if info.id != requested_id {
                return None;
            }
            if requested_source
                .as_deref()
                .is_some_and(|source| info.source.as_deref() != Some(source))
            {
                return None;
            }
            let exact_version = plugin
                .version
                .as_deref()
                .is_some_and(|version| info.version.as_deref() == Some(version));
            Some((exact_version, info.version.unwrap_or_default(), manifest))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(_, _, manifest)| manifest)?;
    let root = manifest.parent()?.parent()?.to_path_buf();
    Some(PluginLocation {
        root,
        manifest: Some(manifest),
    })
}

fn find_kimi_plugin_location(svc: &Service, plugin: &PluginStatus) -> Option<PluginLocation> {
    let kimi_home = crate::skills::agent_paths::kimi_code_home_for(&svc.home);
    let installed = std::fs::read_to_string(kimi_home.join("plugins/installed.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&installed).ok()?;
    let entry = value
        .get("plugins")
        .and_then(|plugins| plugins.as_array())?
        .iter()
        .find(|entry| entry.get("id").and_then(|id| id.as_str()) == Some(&plugin.id))?;
    let configured_root = entry.get("root").and_then(|root| root.as_str())?;
    let root = PathBuf::from(configured_root);
    let root = if root.is_absolute() {
        root
    } else {
        kimi_home.join(root)
    };
    let manifest = [
        root.join("kimi.plugin.json"),
        root.join(".kimi-plugin/plugin.json"),
    ]
    .into_iter()
    .find(|path| path.is_file());
    Some(PluginLocation { root, manifest })
}

fn read_antigravity_plugins(svc: &Service) -> Vec<PluginStatus> {
    let root = svc.home.join(".gemini/config/plugins");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut plugins = entries
        .flatten()
        .filter_map(|entry| {
            let plugin_root = entry.path();
            if !plugin_root.is_dir() {
                return None;
            }
            let content = std::fs::read_to_string(plugin_root.join("plugin.json")).ok()?;
            let manifest = serde_json::from_str::<serde_json::Value>(&content).ok()?;
            let fallback_id = entry.file_name().to_string_lossy().to_string();
            let id = manifest
                .get("name")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&fallback_id)
                .to_string();
            let name = manifest
                .get("displayName")
                .or_else(|| manifest.get("name"))
                .and_then(|value| value.as_str())
                .unwrap_or(&id)
                .to_string();
            Some(PluginStatus {
                id,
                name,
                version: manifest
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                enabled: true,
                source: Some("antigravity-plugin".to_string()),
            })
        })
        .collect::<Vec<_>>();
    plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
    plugins
}

fn find_antigravity_plugin_location(
    svc: &Service,
    plugin: &PluginStatus,
) -> Option<PluginLocation> {
    let root = svc.home.join(".gemini/config/plugins");
    let entries = std::fs::read_dir(root).ok()?;
    entries.flatten().find_map(|entry| {
        let plugin_root = entry.path();
        let manifest_path = plugin_root.join("plugin.json");
        let content = std::fs::read_to_string(&manifest_path).ok()?;
        let manifest = serde_json::from_str::<serde_json::Value>(&content).ok()?;
        let fallback_id = entry.file_name().to_string_lossy().to_string();
        let id = manifest
            .get("name")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&fallback_id);
        (id == plugin.id).then_some(PluginLocation {
            root: plugin_root,
            manifest: Some(manifest_path),
        })
    })
}

fn read_zcode_plugins(svc: &Service) -> Vec<PluginStatus> {
    let cache = svc.home.join(".zcode/cli/plugins/cache");
    let data = svc.home.join(".zcode/cli/plugins/data");
    let config = svc.home.join(".zcode/cli/config.json");
    let enabled_plugins = std::fs::read_to_string(config)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| crate::skills::zcode_config::enabled_plugins(&json).cloned())
        .unwrap_or_default();
    let mut out = Vec::new();
    for manifest in plugin_manifests(&cache, ".zcode-plugin") {
        let Some(info) = read_plugin_manifest_info(&cache, &manifest) else {
            continue;
        };
        let config_key = info
            .source
            .as_deref()
            .map(|source| format!("{}@{source}", info.id))
            .unwrap_or_else(|| info.id.clone());
        let enabled = enabled_plugins
            .get(&config_key)
            .or_else(|| enabled_plugins.get(&info.id))
            .and_then(|value| value.as_bool())
            .unwrap_or_else(|| data.join(&config_key).exists());
        out.push(PluginStatus {
            id: config_key,
            name: info.name,
            version: info.version,
            enabled,
            source: info
                .source
                .map(|source| format!("zcode-plugin:{source}"))
                .or_else(|| Some("zcode-plugin".to_string())),
        });
    }
    out.sort_by_key(|plugin| plugin.name.to_lowercase());
    out
}

fn read_kimi_plugins(svc: &Service) -> Vec<PluginStatus> {
    let kimi_home = crate::skills::agent_paths::kimi_code_home_for(&svc.home);
    let installed_path = kimi_home.join("plugins").join("installed.json");
    let Ok(content) = std::fs::read_to_string(installed_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let Some(entries) = value.get("plugins").and_then(|plugins| plugins.as_array()) else {
        return Vec::new();
    };

    let mut out = entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|id| id.as_str())?.to_string();
            let root = entry
                .get("root")
                .and_then(|root| root.as_str())
                .map(PathBuf::from)
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        kimi_home.join(path)
                    }
                });
            let manifest = root.as_ref().and_then(|root| {
                [
                    root.join("kimi.plugin.json"),
                    root.join(".kimi-plugin").join("plugin.json"),
                ]
                .into_iter()
                .find_map(|path| {
                    std::fs::read_to_string(path).ok().and_then(|content| {
                        serde_json::from_str::<serde_json::Value>(&content).ok()
                    })
                })
            });
            let name = manifest
                .as_ref()
                .and_then(|manifest| {
                    manifest
                        .pointer("/interface/displayName")
                        .and_then(|name| name.as_str())
                        .or_else(|| manifest.get("displayName").and_then(|name| name.as_str()))
                        .or_else(|| manifest.get("name").and_then(|name| name.as_str()))
                })
                .unwrap_or(&id)
                .to_string();
            let version = manifest.as_ref().and_then(|manifest| {
                manifest
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string)
            });
            let source = entry
                .get("originalSource")
                .and_then(|source| source.as_str())
                .or_else(|| entry.get("source").and_then(|source| source.as_str()))
                .map(|source| format!("kimi-plugin:{source}"));

            Some(PluginStatus {
                id,
                name,
                version,
                enabled: entry
                    .get("enabled")
                    .and_then(|enabled| enabled.as_bool())
                    .unwrap_or(true),
                source,
            })
        })
        .collect::<Vec<_>>();
    out.sort_by_key(|plugin| plugin.name.to_lowercase());
    out
}

#[derive(Debug, Clone)]
struct PluginManifestInfo {
    id: String,
    name: String,
    version: Option<String>,
    source: Option<String>,
}

fn read_workbuddy_plugins(svc: &Service) -> Vec<PluginStatus> {
    let enabled_plugins = read_plugin_enabled_config(&svc.home.join(".workbuddy/settings.json"));
    if enabled_plugins.is_empty() {
        return Vec::new();
    }

    let marketplace_root = svc.home.join(".workbuddy/plugins/marketplaces");
    let manifests = plugin_manifests(&marketplace_root, ".codebuddy-plugin")
        .into_iter()
        .filter_map(|manifest| read_plugin_manifest_info(&marketplace_root, &manifest))
        .collect::<Vec<_>>();

    let mut out = Vec::new();
    for (config_key, enabled) in enabled_plugins {
        let (plugin_id, source_from_key) = split_plugin_config_key(&config_key);
        let manifest = manifests.iter().find(|manifest| {
            workbuddy_manifest_matches(manifest, &config_key, &plugin_id, &source_from_key)
        });
        let source = manifest
            .and_then(|manifest| manifest.source.clone())
            .or(source_from_key)
            .map(|source| format!("workbuddy-plugin:{source}"))
            .or_else(|| Some("workbuddy-plugin:settings".to_string()));

        out.push(PluginStatus {
            id: config_key,
            name: manifest
                .map(|manifest| manifest.name.clone())
                .unwrap_or(plugin_id),
            version: manifest.and_then(|manifest| manifest.version.clone()),
            enabled,
            source,
        });
    }

    out.sort_by_key(|plugin| plugin.name.to_lowercase());
    out
}

fn read_plugin_manifest_info(cache: &Path, manifest: &Path) -> Option<PluginManifestInfo> {
    let content = std::fs::read_to_string(manifest).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let id = value
        .get("name")
        .and_then(|name| name.as_str())?
        .to_string();
    if id.is_empty() {
        return None;
    }

    Some(PluginManifestInfo {
        name: value
            .pointer("/interface/displayName")
            .and_then(|name| name.as_str())
            .or_else(|| value.get("displayName").and_then(|name| name.as_str()))
            .or_else(|| value.get("name").and_then(|name| name.as_str()))
            .unwrap_or("")
            .to_string(),
        version: value
            .get("version")
            .and_then(|version| version.as_str())
            .map(String::from),
        source: plugin_cache_source(cache, manifest),
        id,
    })
}

fn split_plugin_config_key(config_key: &str) -> (String, Option<String>) {
    config_key
        .split_once('@')
        .map(|(id, source)| (id.to_string(), Some(source.to_string())))
        .unwrap_or_else(|| (config_key.to_string(), None))
}

fn workbuddy_manifest_matches(
    manifest: &PluginManifestInfo,
    config_key: &str,
    plugin_id: &str,
    source_from_key: &Option<String>,
) -> bool {
    if manifest
        .source
        .as_deref()
        .map(|source| format!("{}@{}", manifest.id, source) == config_key)
        .unwrap_or(false)
    {
        return true;
    }

    manifest.id == plugin_id
        && source_from_key
            .as_deref()
            .map(|source| manifest.source.as_deref() == Some(source))
            .unwrap_or(true)
}

fn plugin_manifests(cache: &Path, marker_dir: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_plugin_manifests(cache, marker_dir, 0, &mut out);
    out
}

fn collect_plugin_manifests(dir: &Path, marker_dir: &str, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let manifest = dir.join(marker_dir).join("plugin.json");
    if manifest.is_file() {
        out.push(manifest);
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_plugin_manifests(&path, marker_dir, depth + 1, out);
        }
    }
}

fn plugin_cache_source(cache: &Path, manifest: &Path) -> Option<String> {
    manifest
        .strip_prefix(cache)
        .ok()?
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
}

fn read_plugin_enabled_config(path: &Path) -> HashMap<String, bool> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        return crate::skills::codex_config::parse_plugin_enabled_config(&content);
    }
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|json| {
            json.get("enabledPlugins")
                .and_then(|value| value.as_object())
                .cloned()
        })
        .map(|plugins| {
            plugins
                .into_iter()
                .filter_map(|(key, value)| value.as_bool().map(|enabled| (key, enabled)))
                .collect()
        })
        .unwrap_or_default()
}

pub fn agent_health(svc: &Service, agent_id: &str) -> Vec<AgentHealthIssue> {
    let mut out = Vec::new();
    if let Some(dir) = agent_meta::agent_skills_dir(svc.home(), agent_id) {
        if !dir.exists() {
            out.push(AgentHealthIssue {
                kind: "skills_dir_missing".to_string(),
                message: format!("Skills directory does not exist: {}", dir.display()),
                severity: "warning".to_string(),
            });
        }
    }
    out
}
