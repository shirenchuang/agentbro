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

pub fn read_mcp_servers(_svc: &Service, agent_id: &str) -> Vec<McpServerStatus> {
    let paths = crate::skills::agent_paths::paths_for_agent(agent_id);
    let Some(config) = paths.mcp_config else {
        return vec![];
    };
    let Ok(content) = std::fs::read_to_string(&config) else {
        return vec![];
    };
    if config.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        return read_toml_mcp_servers(&content);
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    let servers = v
        .get("mcpServers")
        .or_else(|| v.get("mcpServers"))
        .and_then(|s| s.as_object());
    let mut out = Vec::new();
    if let Some(servers) = servers {
        for (name, cfg) in servers {
            let command = cfg
                .get("command")
                .and_then(|c| c.as_str())
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

fn read_toml_mcp_servers(content: &str) -> Vec<McpServerStatus> {
    #[derive(Default)]
    struct Pending {
        name: String,
        command: String,
        args: Vec<String>,
    }

    fn push_pending(out: &mut Vec<McpServerStatus>, pending: Option<Pending>) {
        if let Some(server) = pending {
            let valid = !server.command.is_empty();
            out.push(McpServerStatus {
                name: server.name,
                command: server.command,
                args: server.args,
                valid,
                message: if valid {
                    "configured".to_string()
                } else {
                    "missing command".to_string()
                },
            });
        }
    }

    let mut out = Vec::new();
    let mut pending: Option<Pending> = None;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("[mcp_servers.")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            push_pending(&mut out, pending.take());
            pending = Some(Pending {
                name: name.trim_matches('"').to_string(),
                ..Pending::default()
            });
            continue;
        }
        let Some(server) = pending.as_mut() else {
            continue;
        };
        if let Some(value) = line.strip_prefix("command").and_then(|v| v.split_once('=')) {
            server.command = value.1.trim().trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("args").and_then(|v| v.split_once('=')) {
            let args = value
                .1
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .filter_map(|part| {
                    let arg = part.trim().trim_matches('"');
                    (!arg.is_empty()).then(|| arg.to_string())
                })
                .collect();
            server.args = args;
        }
    }
    push_pending(&mut out, pending);
    out
}

pub fn read_plugins(svc: &Service, agent_id: &str) -> Vec<PluginStatus> {
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
    let mut out = Vec::new();
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
        let enabled = enabled_plugins
            .get(&config_key)
            .or_else(|| enabled_plugins.get(&id))
            .copied()
            .unwrap_or(true);
        let status_id = if agent_id == "codex" {
            config_key
        } else {
            id.clone()
        };
        out.push(PluginStatus {
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
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
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
        return read_toml_plugin_enabled_config(&content);
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

fn read_toml_plugin_enabled_config(content: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    let mut current_plugin: Option<String> = None;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            current_plugin = parse_toml_plugin_header(line);
            if let Some(plugin) = current_plugin.as_ref() {
                out.entry(plugin.clone()).or_insert(true);
            }
            continue;
        }
        let Some(plugin) = current_plugin.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "enabled" {
            match value.trim() {
                "true" => {
                    out.insert(plugin.clone(), true);
                }
                "false" => {
                    out.insert(plugin.clone(), false);
                }
                _ => {}
            }
        }
    }
    out
}

fn parse_toml_plugin_header(line: &str) -> Option<String> {
    let inner = line.trim_start_matches('[').trim_end_matches(']').trim();
    let key = inner.strip_prefix("plugins.")?.trim();
    Some(strip_toml_quotes(key).to_string())
}

fn strip_toml_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
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
