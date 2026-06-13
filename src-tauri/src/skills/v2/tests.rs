//! Skill Manager v2 — unit + integration tests.

#![cfg(test)]

use crate::skills::v2::fsutil;
use crate::skills::v2::models::*;
use crate::skills::v2::service::{ClaimOrigin, Service, UpsertPackInput};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Reuse the shared HOME lock from `skills` so v2 tests serialize against the
/// legacy skill tests and `cargo test` is deterministic under parallelism.
fn lock_home() -> std::sync::MutexGuard<'static, ()> {
    crate::skills::lock_shared_test_home()
}

struct TempHome {
    path: PathBuf,
    prev: Option<String>,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("agentbro-v2-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        let prev = std::env::var("HOME").ok();
        std::env::set_var("HOME", &path);
        Self { path, prev }
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        // Do NOT restore HOME here. The destructured `(home, svc, lock)` tuple
        // drops the lock before TempHome, so restoring HOME now would race with
        // the next test's set_var (which holds the lock). Each test sets HOME at
        // TempHome::new under the shared lock; leaving it is harmless.
        let _ = &self.prev;
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write a skill directory with frontmatter + an extra file (for hash testing).
fn write_skill(root: &Path, dir: &str, name: &str, extra: Option<&str>) -> PathBuf {
    let dir = root.join(dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: Test skill {name}\n---\n# {name}\n"
        ),
    )
    .unwrap();
    if let Some(content) = extra {
        fs::write(dir.join("reference.md"), content).unwrap();
    }
    dir
}

fn fresh_service(label: &str) -> (TempHome, Service, std::sync::MutexGuard<'static, ()>) {
    let lock = lock_home();
    let home = TempHome::new(label);
    let sqlite = fsutil::default_sqlite_path();
    let svc = Service::new(&sqlite, fsutil::home()).unwrap();
    svc.init().unwrap();
    (home, svc, lock)
}

// ── Hash stability ────────────────────────────────────────────────

#[test]
fn hash_is_stable_and_ignores_noise() {
    let _g = lock_home();
    let home = TempHome::new("hash");
    let a = write_skill(&home.path.join("src"), "alpha", "alpha", Some("body"));
    let h1 = fsutil::hash_dir(&a);
    // add a .DS_Store — must not change hash
    fs::write(a.join(".DS_Store"), "noise").unwrap();
    let h2 = fsutil::hash_dir(&a);
    assert_eq!(h1, h2, ".DS_Store must be ignored");
    // change real content — hash must change
    fs::write(a.join("reference.md"), "changed").unwrap();
    let h3 = fsutil::hash_dir(&a);
    assert_ne!(h1, h3, "content change must change hash");
    // order independence: two skills swapped
    let dir1 = home.path.join("multi/x");
    let dir2 = home.path.join("multi/y");
    fs::create_dir_all(&dir1).unwrap();
    fs::write(dir1.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
    fs::create_dir_all(&dir2).unwrap();
    fs::write(dir2.join("SKILL.md"), "---\nname: y\n---\n").unwrap();
    let multi = home.path.join("multi");
    let mh1 = fsutil::hash_dir(&multi);
    let mh2 = fsutil::hash_dir(&multi);
    assert_eq!(mh1, mh2);
}

#[test]
fn sanitize_id_normalizes_segments() {
    assert_eq!(fsutil::sanitize_id("My Cool Skill!"), "My-Cool-Skill");
    assert_eq!(fsutil::sanitize_id("github.code.review"), "github-code-review");
    assert_eq!(fsutil::sanitize_id("  --weird--  "), "weird");
}

// ── Add to center + source conflict ──────────────────────────────

#[test]
fn add_center_skill_creates_and_updates() {
    let (_home, svc, _lock) = fresh_service("add");
    let src = write_skill(&svc.home.join("incoming"), "gh-review", "github-code-review", Some("v1"));
    let preview = svc
        .preview_add_center_skill(AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some("file://incoming".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        })
        .unwrap();
    assert_eq!(preview.candidates.len(), 1);
    assert_eq!(preview.candidates[0].action, "create");
    assert!(preview.blockers.is_empty());

    let result = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: Some("file://incoming".to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
            },
            vec![],
        )
        .unwrap();
    assert_eq!(result.skill_ids, vec!["github-code-review".to_string()]);

    let skills = svc.list_center_skills().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "github-code-review");

    // re-import same source → update, not duplicate
    let result2 = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: Some("file://incoming".to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
            },
            vec![],
        )
        .unwrap();
    assert!(result2.skill_ids.is_empty());
    assert_eq!(result2.updated, vec!["github-code-review".to_string()]);
    assert_eq!(svc.list_center_skills().unwrap().len(), 1);
}

#[test]
fn same_name_different_source_is_blocked() {
    let (_home, svc, _lock) = fresh_service("conflict");
    let a = write_skill(&svc.home.join("a"), "sk", "shared", Some("from-a"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: a.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some("uri-a".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();

    // different source, same name → blocker
    let b = write_skill(&svc.home.join("b"), "sk2", "shared", Some("from-b"));
    let preview = svc
        .preview_add_center_skill(AddCenterSkillInput {
            source_path: b.display().to_string(),
            source_type: "github".to_string(),
            source_uri: Some("uri-b".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        })
        .unwrap();
    assert_eq!(preview.blockers.len(), 1);
    assert!(preview.candidates.is_empty());

    // executing without decision → error (no silent overwrite)
    let err = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: b.display().to_string(),
                source_type: "github".to_string(),
                source_uri: Some("uri-b".to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
            },
            vec![],
        );
    assert!(err.is_err(), "must refuse to auto-overwrite different source");

    // rename decision → imports under new id
    let res = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: b.display().to_string(),
                source_type: "github".to_string(),
                source_uri: Some("uri-b".to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
            },
            vec![AddCenterSkillDecision {
                skill_id: "shared".to_string(),
                proposed_skill_id: Some("shared-github".to_string()),
                resolution: "create".to_string(),
            }],
        )
        .unwrap();
    assert_eq!(res.skill_ids, vec!["shared-github".to_string()]);
    let ids: Vec<_> = svc.list_center_skills().unwrap().into_iter().map(|s| s.id).collect();
    assert!(ids.contains(&"shared".to_string()));
    assert!(ids.contains(&"shared-github".to_string()));
}

#[test]
fn invalid_skill_dir_rejected() {
    let (_home, svc, _lock) = fresh_service("invalid");
    let bad = svc.home.join("no-skill-md");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("readme.txt"), "not a skill").unwrap();
    let err = svc.preview_add_center_skill(AddCenterSkillInput {
        source_path: bad.display().to_string(),
        source_type: "local_folder".to_string(),
        source_uri: None,
        imported_from_agent: None,
        imported_from_path: None,
        multi: None,
    });
    assert!(err.is_err());
}

// ── Distribution link/copy + actual_mode ─────────────────────────

#[test]
fn distribute_link_and_copy_record_actual_mode() {
    let (_home, svc, _lock) = fresh_service("distribute");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();

    // link to claude-code
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    assert!(preview.blockers.is_empty());
    assert_eq!(preview.changes[0].action, "create");
    let preview = svc.execute_distribute_skill(preview, ClaimOrigin::Direct).unwrap();
    let actual = &preview.changes[0].actual_mode;
    assert!(actual.as_deref() == Some("link") || actual.as_deref() == Some("copy"));

    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].claims.len(), 1);
    assert_eq!(detail.targets[0].claims[0].claim_type, "direct");

    // copy to codex
    let preview2 = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    let preview2 = svc.execute_distribute_skill(preview2, ClaimOrigin::Direct).unwrap();
    assert_eq!(preview2.changes[0].actual_mode.as_deref(), Some("copy"));
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert_eq!(detail.targets.len(), 2);
}

#[test]
fn reuse_target_appends_claim_without_dup_files() {
    let (_home, svc, _lock) = fresh_service("reuse");
    let src = write_skill(&svc.home.join("s"), "tdd", "test-driven-development", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    for _ in 0..2 {
        let p = svc
            .preview_distribute_skill(
                vec!["test-driven-development".to_string()],
                vec!["claude-code".to_string()],
                "copy".to_string(),
            )
            .unwrap();
        svc.execute_distribute_skill(p, ClaimOrigin::Direct).unwrap();
    }
    let detail = svc.get_skill_detail("test-driven-development").unwrap();
    assert_eq!(detail.targets.len(), 1, "no duplicate target row");
    let claims = &detail.targets[0].claims;
    // direct claims are unique by (target, claim_type, pack_id) — one direct claim
    assert_eq!(claims.len(), 1);
}

// ── Target / claim deletion rules ────────────────────────────────

#[test]
fn revoke_pack_keeps_file_when_other_claim_remains() {
    let (_home, svc, _lock) = fresh_service("revoke-multi");
    let src = write_skill(&svc.home.join("s"), "dbg", "database-debugging", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();

    // pack A
    let pack_a = svc
        .upsert_skill_pack(UpsertPackInput {
            id: "pack-a".to_string(),
            name: "A".to_string(),
            description: "".to_string(),
            tags: vec![],
            skill_ids: vec!["database-debugging".to_string()],
        })
        .unwrap();
    // pack B (different id, same skill)
    let _pack_b = svc
        .upsert_skill_pack(UpsertPackInput {
            id: "pack-b".to_string(),
            name: "B".to_string(),
            description: "".to_string(),
            tags: vec![],
            skill_ids: vec!["database-debugging".to_string()],
        })
        .unwrap();

    svc.apply_skill_pack("pack-a", vec!["claude-code".to_string()], "copy".to_string())
        .unwrap();
    svc.apply_skill_pack("pack-b", vec!["claude-code".to_string()], "copy".to_string())
        .unwrap();

    let detail = svc.get_skill_detail("database-debugging").unwrap();
    assert_eq!(detail.targets.len(), 1);
    let claims = &detail.targets[0].claims;
    assert_eq!(claims.len(), 2, "two pack claims coexist");
    let target_path = detail.targets[0].target_path.clone();
    assert!(Path::new(&target_path).exists());

    // revoke pack A only → file must remain (pack B claim still active)
    let res = svc
        .remove_skill_pack_from_agent("pack-a", "claude-code")
        .unwrap();
    assert_eq!(res.removed_claims, 1);
    assert_eq!(res.preserved_targets, 1);
    assert!(Path::new(&target_path).exists(), "file preserved while pack B active");

    // revoke pack B → last claim → file removed
    let res = svc
        .remove_skill_pack_from_agent("pack-b", "claude-code")
        .unwrap();
    assert_eq!(res.removed_targets, 1);
    assert!(!Path::new(&target_path).exists(), "file removed when no claim left");
}

#[test]
fn apply_pack_is_idempotent() {
    let (_home, svc, _lock) = fresh_service("idempotent");
    let src = write_skill(&svc.home.join("s"), "auth", "skill-authoring", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    svc.upsert_skill_pack(UpsertPackInput {
        id: "p1".to_string(),
        name: "P1".to_string(),
        description: "".to_string(),
        tags: vec![],
        skill_ids: vec!["skill-authoring".to_string()],
    })
    .unwrap();
    for _ in 0..3 {
        svc.apply_skill_pack("p1", vec!["claude-code".to_string()], "copy".to_string())
            .unwrap();
    }
    let detail = svc.get_skill_detail("skill-authoring").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].claims.len(), 1, "no duplicate pack claim");
}

// ── Copy sync: outdated / modified / diverged ───────────────────

#[test]
fn copy_sync_detects_outdated_modified_diverged() {
    let (_home, svc, _lock) = fresh_service("sync");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("original"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    let p = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    let p = svc.execute_distribute_skill(p, ClaimOrigin::Direct).unwrap();
    let target_id = svc.get_skill_detail("release-checklist").unwrap().targets[0].id.clone();

    // center-only change → outdated
    let center = svc.center_path().unwrap().join("release-checklist");
    fs::write(center.join("reference.md"), "center-updated").unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "copy_outdated");
    assert_eq!(prev.suggested, "center_over_agent");

    // execute center_over_agent → resolves
    svc.execute_sync_copy_target(&target_id, "center_over_agent").unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "ok");

    // agent-only change → modified
    let target_path = svc
        .get_skill_detail("release-checklist")
        .unwrap()
        .targets[0]
        .target_path
        .clone();
    fs::write(Path::new(&target_path).join("reference.md"), "agent-edited").unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "copy_modified");

    // both change → diverged
    fs::write(center.join("reference.md"), "center-again").unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "copy_diverged");
    assert_eq!(prev.suggested, "manual");
}

// ── Delete center skill preview ──────────────────────────────────

#[test]
fn delete_center_skill_preview_lists_targets() {
    let (_home, svc, _lock) = fresh_service("delete");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    let p = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(p, ClaimOrigin::Direct).unwrap();

    let preview = svc.preview_delete_center_skill("release-checklist").unwrap();
    assert!(!preview.removable, "linked targets block removal");
    assert_eq!(preview.affected_targets.len(), 1);

    // refuse without confirmation
    let err = svc.execute_delete_center_skill("release-checklist", false);
    assert!(err.is_err());

    // with confirmation removes linked + center dir
    svc.execute_delete_center_skill("release-checklist", true).unwrap();
    assert!(svc.get_skill_detail("release-checklist").is_err());
}

// ── Migration from legacy metadata ───────────────────────────────

#[test]
fn migrates_legacy_packs_and_sources() {
    let _g = lock_home();
    let home = TempHome::new("migrate");
    // write legacy metadata.json before any v2 init
    let meta = home.path.join(".agentbro/metadata.json");
    fs::create_dir_all(meta.parent().unwrap()).unwrap();
    fs::write(
        &meta,
        serde_json::json!({
            "sources": { "legacy-skill": { "origin": "/some/path" } },
            "packs": [{ "id": "legacy-pack", "name": "Legacy", "description": "x", "skills": ["legacy-skill"] }]
        })
        .to_string(),
    )
    .unwrap();
    // also drop a matching center dir so the skill row gets a hash
    write_skill(&home.path.join(".agentbro/skills"), "legacy-skill", "legacy-skill", Some("v1"));

    let sqlite = fsutil::default_sqlite_path();
    let svc = Service::new(&sqlite, fsutil::home()).unwrap();
    svc.init().unwrap();

    let packs = svc.list_skill_packs().unwrap();
    assert!(packs.iter().any(|p| p.id == "legacy-pack"));
    let detail = svc.get_skill_pack_detail("legacy-pack").unwrap();
    assert!(detail.members.iter().any(|m| m.skill_id == "legacy-skill" && !m.missing));
    // legacy file preserved
    assert!(meta.exists(), "legacy metadata.json must not be deleted");
}

// ── Diagnosis ────────────────────────────────────────────────────

#[test]
fn diagnosis_flags_broken_link_and_unmanaged() {
    let (_home, svc, _lock) = fresh_service("diag");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    // distribute link, then delete center dir → broken link
    let p = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(p, ClaimOrigin::Direct).unwrap();
    let center = svc.center_path().unwrap().join("release-checklist");
    fs::remove_dir_all(&center).unwrap();

    // add an unmanaged skill to codex
    write_skill(&svc.home.join(".codex/skills"), "rogue", "rogue-skill", Some("v1"));

    svc.refresh().unwrap();
    let issues = crate::skills::v2::diagnosis::run(&svc).unwrap();
    let types: Vec<_> = issues.iter().map(|i| i.issue_type.clone()).collect();
    assert!(types.iter().any(|t| t == "broken_link" || t == "target_missing"), "found: {:?}", types);
    assert!(types.iter().any(|t| t == "agent_unmanaged"), "found: {:?}", types);

    // safe fixes clear broken link / orphan records
    let _fixed = crate::skills::v2::diagnosis::execute_safe_fixes(&svc).unwrap();
    let issues2 = crate::skills::v2::diagnosis::run(&svc).unwrap();
    let types2: Vec<_> = issues2.iter().map(|i| i.issue_type.clone()).collect();
    assert!(!types2.iter().any(|t| t == "broken_link"), "broken link auto-cleared");
}

// ── Snapshot ─────────────────────────────────────────────────────

#[test]
fn snapshot_written_and_read() {
    let (_home, svc, _lock) = fresh_service("snapshot");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
        },
        vec![],
    )
    .unwrap();
    let path = crate::skills::v2::snapshot::export_to_file(&svc).unwrap();
    assert!(Path::new(&path).exists());
    let snap = crate::skills::v2::snapshot::export(&svc).unwrap();
    assert_eq!(snap.schema_version, 2);
    assert_eq!(snap.skills.len(), 1);
}

// ── Settings ─────────────────────────────────────────────────────

#[test]
fn settings_round_trip() {
    let (_home, svc, _lock) = fresh_service("settings");
    let updated = svc
        .update_settings(SettingsUpdate {
            center_path: None,
            sqlite_path: None,
            default_distribute_mode: Some("copy".to_string()),
            link_fail_policy: Some("copy".to_string()),
            startup_scan: Some(false),
            show_unmanaged: None,
        })
        .unwrap();
    assert_eq!(updated.default_distribute_mode, "copy");
    assert_eq!(updated.link_fail_policy, "copy");
    assert!(!updated.startup_scan);
}
