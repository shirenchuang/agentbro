//! Skill Manager v2 — unit + integration tests.

#![cfg(test)]

use crate::skills::v2::fsutil;
use crate::skills::v2::models::*;
use crate::skills::v2::service::{ClaimOrigin, Service, UpsertPackInput};
use rusqlite::params;
use std::fs;
use std::io::Write;
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
        format!("---\nname: {name}\ndescription: Test skill {name}\n---\n# {name}\n"),
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
    assert_eq!(
        fsutil::sanitize_id("github.code.review"),
        "github-code-review"
    );
    assert_eq!(fsutil::sanitize_id("  --weird--  "), "weird");
}

// ── Add to center + source conflict ──────────────────────────────

#[test]
fn add_center_skill_creates_and_updates() {
    let (_home, svc, _lock) = fresh_service("add");
    let src = write_skill(
        &svc.home.join("incoming"),
        "gh-review",
        "github-code-review",
        Some("v1"),
    );
    let preview = svc
        .preview_add_center_skill(AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some("file://incoming".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
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
                import_mode: None,
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
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    assert!(result2.skill_ids.is_empty());
    assert_eq!(result2.updated, vec!["github-code-review".to_string()]);
    assert_eq!(svc.list_center_skills().unwrap().len(), 1);
}

#[test]
fn add_center_skill_can_link_to_local_source() {
    let (_home, svc, _lock) = fresh_service("add-link");
    let src = write_skill(
        &svc.home.join("dev-skills"),
        "live-review",
        "live-review",
        Some("v1"),
    );

    let result = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: Some(src.display().to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: Some("link".to_string()),
            },
            vec![],
        )
        .unwrap();

    assert_eq!(result.skill_ids, vec!["live-review".to_string()]);
    let center_skill = fsutil::default_center_path().join("live-review");
    assert!(center_skill.is_symlink());
    assert_eq!(fs::read_link(&center_skill).unwrap(), src);

    fs::write(
        src.join("SKILL.md"),
        "---\nname: live-review\ndescription: linked source\n---\n# updated\n",
    )
    .unwrap();
    let center_doc = fs::read_to_string(center_skill.join("SKILL.md")).unwrap();
    assert!(center_doc.contains("# updated"));
}

#[test]
fn local_parent_import_matches_existing_child_source() {
    let (_home, svc, _lock) = fresh_service("parent-child-source");
    let root = svc.home.join("dev-skills");
    let alpha = write_skill(&root, "alpha-review", "alpha-review", Some("v1"));
    let beta = write_skill(&root, "beta-review", "beta-review", Some("v1"));

    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: alpha.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some(alpha.display().to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: Some("link".to_string()),
        },
        vec![],
    )
    .unwrap();

    let preview = svc
        .preview_add_center_skill(AddCenterSkillInput {
            source_path: root.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some(root.display().to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: Some(true),
            import_mode: Some("link".to_string()),
        })
        .unwrap();

    assert!(
        preview.blockers.is_empty(),
        "parent folder import should not conflict with the same child source"
    );
    let alpha_preview = preview
        .candidates
        .iter()
        .find(|candidate| candidate.skill_id == "alpha-review")
        .unwrap();
    assert_eq!(alpha_preview.action, "update");

    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: root.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some(root.display().to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: Some(true),
            import_mode: Some("link".to_string()),
        },
        vec![],
    )
    .unwrap();

    let stored_beta_uri: String = svc
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT source_uri FROM skill_sources WHERE skill_id = 'beta-review'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(stored_beta_uri, beta.display().to_string());
}

#[test]
#[cfg(target_os = "windows")]
fn windows_center_link_import_falls_back_to_copy() {
    let (_home, svc, _lock) = fresh_service("add-link-windows-copy");
    let src = write_skill(
        &svc.home.join("dev-skills"),
        "live-review",
        "live-review",
        Some("v1"),
    );

    let result = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: Some(src.display().to_string()),
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: Some("link".to_string()),
            },
            vec![],
        )
        .unwrap();

    assert_eq!(result.skill_ids, vec!["live-review".to_string()]);
    let center_skill = fsutil::default_center_path().join("live-review");
    assert!(center_skill.join("SKILL.md").is_file());
    assert!(!center_skill.is_symlink());
}

#[test]
fn linked_center_skill_file_tree_expands_source_directory() {
    let (_home, svc, _lock) = fresh_service("add-link-tree");
    let src = write_skill(
        &svc.home.join("dev-skills"),
        "live-tree",
        "live-tree",
        Some("reference"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some(src.display().to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: Some("link".to_string()),
        },
        vec![],
    )
    .unwrap();
    let center_skill = fsutil::default_center_path().join("live-tree");

    let tree = fsutil::build_file_tree(&center_skill, 4).unwrap();

    assert_eq!(tree.node_type, "dir");
    let children = tree.children.unwrap();
    assert!(children
        .iter()
        .any(|child| child.node_type == "file" && child.name == "SKILL.md"));
    assert!(children
        .iter()
        .any(|child| child.node_type == "file" && child.name == "reference.md"));
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
            import_mode: None,
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
            import_mode: None,
        })
        .unwrap();
    assert_eq!(preview.blockers.len(), 1);
    assert!(preview.candidates.is_empty());

    // executing without decision → error (no silent overwrite)
    let err = svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: b.display().to_string(),
            source_type: "github".to_string(),
            source_uri: Some("uri-b".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    );
    assert!(
        err.is_err(),
        "must refuse to auto-overwrite different source"
    );

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
                import_mode: None,
            },
            vec![AddCenterSkillDecision {
                skill_id: "shared".to_string(),
                proposed_skill_id: Some("shared-github".to_string()),
                resolution: "create".to_string(),
            }],
        )
        .unwrap();
    assert_eq!(res.skill_ids, vec!["shared-github".to_string()]);
    let ids: Vec<_> = svc
        .list_center_skills()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
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
        import_mode: None,
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
            import_mode: None,
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
    let preview = svc
        .execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
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
    let preview2 = svc
        .execute_distribute_skill(preview2, ClaimOrigin::Direct)
        .unwrap();
    assert_eq!(preview2.changes[0].actual_mode.as_deref(), Some("copy"));
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert_eq!(detail.targets.len(), 2);
}

#[test]
#[cfg(target_os = "windows")]
fn windows_link_distribution_falls_back_to_copy_even_with_ask_policy() {
    let (_home, svc, _lock) = fresh_service("windows-link-fallback");
    svc.update_settings(SettingsUpdate {
        center_path: None,
        sqlite_path: None,
        default_distribute_mode: Some("link".to_string()),
        link_fail_policy: Some("ask".to_string()),
        startup_scan: None,
        show_unmanaged: None,
        auto_sync_skill_packs: None,
    })
    .unwrap();
    let src = write_skill(
        &svc.home.join("s"),
        "windows-copy",
        "windows-copy",
        Some("v1"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let preview = svc
        .preview_distribute_skill(
            vec!["windows-copy".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    assert!(preview.blockers.is_empty());
    assert_eq!(preview.changes[0].actual_mode.as_deref(), Some("copy"));

    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("windows-copy").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].install_mode, "link");
    assert_eq!(detail.targets[0].actual_mode, "copy");
    assert!(Path::new(&detail.targets[0].target_path)
        .join("SKILL.md")
        .is_file());
}

#[test]
fn distribute_rejects_shared_agents_directory_as_target() {
    let (_home, svc, _lock) = fresh_service("distribute-reject-agents-target");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let err = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["agents".to_string()],
            "link".to_string(),
        )
        .unwrap_err();
    assert!(err.contains("shared .agents skills directory"));
}

#[test]
fn reuse_target_appends_claim_without_dup_files() {
    let (_home, svc, _lock) = fresh_service("reuse");
    let src = write_skill(
        &svc.home.join("s"),
        "tdd",
        "test-driven-development",
        Some("v1"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
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
        svc.execute_distribute_skill(p, ClaimOrigin::Direct)
            .unwrap();
    }
    let detail = svc.get_skill_detail("test-driven-development").unwrap();
    assert_eq!(detail.targets.len(), 1, "no duplicate target row");
    let claims = &detail.targets[0].claims;
    // direct claims are unique by (target, claim_type, pack_id) — one direct claim
    assert_eq!(claims.len(), 1);
}

#[test]
fn redistributing_modified_copy_requires_source_decision() {
    let (_home, svc, _lock) = fresh_service("redistribute-copy-decision");
    let src = write_skill(
        &svc.home.join("s"),
        "release-checklist",
        "release-checklist",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let copy_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(copy_preview, ClaimOrigin::Direct)
        .unwrap();
    let target_path = svc.get_skill_detail("release-checklist").unwrap().targets[0]
        .target_path
        .clone();
    fs::write(
        Path::new(&target_path).join("reference.md"),
        "agent-version",
    )
    .unwrap();

    let copy_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    assert!(copy_preview.changes.is_empty());
    assert_eq!(copy_preview.blockers.len(), 1);
    assert!(copy_preview.blockers[0].reason.contains("Managed copy"));
}

#[test]
#[cfg(unix)]
fn distributing_existing_managed_target_with_different_mode_converts_it() {
    let (_home, svc, _lock) = fresh_service("distribute-convert-mode");
    let src = write_skill(
        &svc.home.join("s"),
        "release-checklist",
        "release-checklist",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let copy_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(copy_preview, ClaimOrigin::Direct)
        .unwrap();

    let link_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    assert!(link_preview.blockers.is_empty());
    assert_eq!(link_preview.changes.len(), 1);
    assert_eq!(link_preview.changes[0].action, "convert");
    assert_eq!(link_preview.changes[0].actual_mode.as_deref(), Some("link"));

    svc.execute_distribute_skill(link_preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].actual_mode, "link");
    assert!(Path::new(&detail.targets[0].target_path).is_symlink());
}

#[test]
#[cfg(unix)]
fn converting_modified_copy_to_link_requires_source_decision() {
    let (_home, svc, _lock) = fresh_service("distribute-convert-copy-decision");
    let src = write_skill(
        &svc.home.join("s"),
        "release-checklist",
        "release-checklist",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let copy_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(copy_preview, ClaimOrigin::Direct)
        .unwrap();
    let target_path = svc.get_skill_detail("release-checklist").unwrap().targets[0]
        .target_path
        .clone();
    fs::write(
        Path::new(&target_path).join("reference.md"),
        "agent-version",
    )
    .unwrap();

    let mut link_preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    assert!(link_preview.changes.is_empty());
    assert_eq!(link_preview.blockers.len(), 1);
    assert!(link_preview.blockers[0].reason.contains("Managed copy"));
    assert!(svc
        .execute_distribute_skill(link_preview.clone(), ClaimOrigin::Direct)
        .is_err());

    link_preview.blocker_decisions = vec![DistributionBlockerDecision {
        skill_id: "release-checklist".to_string(),
        agent_id: "claude-code".to_string(),
        action: "agent_over_center".to_string(),
    }];
    svc.execute_distribute_skill(link_preview, ClaimOrigin::Direct)
        .unwrap();

    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].actual_mode, "link");
    assert!(Path::new(&detail.targets[0].target_path).is_symlink());
    let center = svc.center_path().unwrap().join("release-checklist");
    assert_eq!(
        fs::read_to_string(center.join("reference.md")).unwrap(),
        "agent-version"
    );
}

#[test]
fn distribute_blocker_can_be_skipped_or_overwritten() {
    let (_home, svc, _lock) = fresh_service("distribute-blocker-decision");
    let src = write_skill(
        &svc.home.join("s"),
        "release-checklist",
        "release-checklist",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let unmanaged = write_skill(
        &svc.home.join(".claude/skills"),
        "release-checklist",
        "release-checklist",
        Some("agent-version"),
    );
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    assert_eq!(preview.blockers.len(), 1);
    assert!(svc
        .execute_distribute_skill(preview.clone(), ClaimOrigin::Direct)
        .is_err());

    let mut skip_preview = preview.clone();
    skip_preview.blocker_decisions = vec![DistributionBlockerDecision {
        skill_id: "release-checklist".to_string(),
        agent_id: "claude-code".to_string(),
        action: "skip".to_string(),
    }];
    let skipped = svc
        .execute_distribute_skill(skip_preview, ClaimOrigin::Direct)
        .unwrap();
    assert_eq!(skipped.blockers.len(), 0);
    assert_eq!(skipped.changes[0].action, "skip");
    assert_eq!(
        svc.get_skill_detail("release-checklist")
            .unwrap()
            .targets
            .len(),
        0
    );
    assert_eq!(
        fs::read_to_string(unmanaged.join("reference.md")).unwrap(),
        "agent-version"
    );

    let mut overwrite_preview = preview;
    overwrite_preview.blocker_decisions = vec![DistributionBlockerDecision {
        skill_id: "release-checklist".to_string(),
        agent_id: "claude-code".to_string(),
        action: "overwrite".to_string(),
    }];
    let overwritten = svc
        .execute_distribute_skill(overwrite_preview, ClaimOrigin::Direct)
        .unwrap();
    assert!(overwritten
        .changes
        .iter()
        .any(|change| change.action == "overwrite"));
    assert_eq!(
        svc.get_skill_detail("release-checklist")
            .unwrap()
            .targets
            .len(),
        1
    );
    assert_eq!(
        fs::read_to_string(unmanaged.join("reference.md")).unwrap(),
        "center-version"
    );
}

#[test]
#[cfg(unix)]
fn distribute_overwrite_can_replace_unmanaged_symlink_target() {
    let (_home, svc, _lock) = fresh_service("distribute-overwrite-symlink");
    let src = write_skill(
        &svc.home.join("s"),
        "find-skills",
        "find-skills",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let old_source = write_skill(
        &svc.home.join(".skills-manager/skills"),
        "find-skills",
        "find-skills",
        Some("old-version"),
    );
    let codex_dir = svc.home.join(".codex/skills");
    fs::create_dir_all(&codex_dir).unwrap();
    let old_link = codex_dir.join("find-skills");
    std::os::unix::fs::symlink(&old_source, &old_link).unwrap();

    let preview = svc
        .preview_distribute_skill(
            vec!["find-skills".to_string()],
            vec!["codex".to_string()],
            "link".to_string(),
        )
        .unwrap();
    assert_eq!(preview.blockers.len(), 1);
    assert_eq!(
        preview.blockers[0].existing_path_kind.as_deref(),
        Some("symlink")
    );
    assert_eq!(
        preview.blockers[0].resolved_existing_path.as_deref(),
        Some(old_source.display().to_string().as_str())
    );

    let mut overwrite_preview = preview;
    overwrite_preview.blocker_decisions = vec![DistributionBlockerDecision {
        skill_id: "find-skills".to_string(),
        agent_id: "codex".to_string(),
        action: "overwrite".to_string(),
    }];
    svc.execute_distribute_skill(overwrite_preview, ClaimOrigin::Direct)
        .unwrap();

    let detail = svc.get_skill_detail("find-skills").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].agent_id, "codex");
    assert_eq!(detail.targets[0].actual_mode, "link");
    assert_eq!(
        detail.targets[0].resolved_target_path.as_deref(),
        Some(
            svc.center_path()
                .unwrap()
                .join("find-skills")
                .display()
                .to_string()
                .as_str()
        )
    );
    assert!(
        old_source.exists(),
        "overwrite must not delete symlink target"
    );
    assert_ne!(std::fs::read_link(&old_link).unwrap(), old_source);
}

#[test]
fn distribute_multiple_skills_to_multiple_agents_is_isolated_and_idempotent() {
    let (_home, svc, _lock) = fresh_service("multi-agent-distribution");
    let review = write_skill(
        &svc.home.join("s"),
        "review",
        "github-code-review",
        Some("review-v1"),
    );
    let debug = write_skill(
        &svc.home.join("s"),
        "debug",
        "database-debugging",
        Some("debug-v1"),
    );

    for src in [&review, &debug] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }

    let skill_ids = vec![
        "github-code-review".to_string(),
        "database-debugging".to_string(),
    ];
    let target_agents = vec!["claude-code".to_string(), "codex".to_string()];
    let preview = svc
        .preview_distribute_skill(skill_ids.clone(), target_agents.clone(), "copy".to_string())
        .unwrap();

    assert!(preview.blockers.is_empty());
    assert_eq!(preview.changes.len(), 4);
    for skill_id in &skill_ids {
        for agent_id in &target_agents {
            assert!(preview.changes.iter().any(|change| {
                change.skill_id == *skill_id
                    && change.agent_id == *agent_id
                    && change.action == "create"
                    && change.actual_mode.as_deref() == Some("copy")
            }));
        }
    }

    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();

    for skill_id in &skill_ids {
        let detail = svc.get_skill_detail(skill_id).unwrap();
        assert_eq!(
            detail.targets.len(),
            2,
            "{skill_id} is installed on both agents"
        );
        for agent_id in &target_agents {
            let target = detail
                .targets
                .iter()
                .find(|target| target.agent_id == *agent_id)
                .expect("target for agent");
            assert_eq!(target.actual_mode, "copy");
            assert_eq!(target.claims.len(), 1);
            assert_eq!(target.claims[0].claim_type, "direct");
            assert!(Path::new(&target.target_path).exists());
        }
    }

    for agent_id in &target_agents {
        let detail = svc.get_agent_detail(agent_id).unwrap();
        let mut installed: Vec<_> = detail
            .skills
            .iter()
            .map(|target| target.skill_id.as_str())
            .collect();
        installed.sort_unstable();
        assert_eq!(installed, vec!["database-debugging", "github-code-review"]);
    }

    let reuse = svc
        .preview_distribute_skill(skill_ids.clone(), target_agents, "copy".to_string())
        .unwrap();
    assert!(reuse.blockers.is_empty());
    assert_eq!(reuse.changes.len(), 4);
    assert!(reuse
        .changes
        .iter()
        .all(|change| change.action == "reinstall"));
    svc.execute_distribute_skill(reuse, ClaimOrigin::Direct)
        .unwrap();

    for skill_id in skill_ids {
        let detail = svc.get_skill_detail(&skill_id).unwrap();
        assert_eq!(detail.targets.len(), 2);
        assert!(detail
            .targets
            .iter()
            .all(|target| target.claims.len() == 1 && target.claims[0].claim_type == "direct"));
    }
}

#[test]
fn distribution_execution_rejects_preview_paths_outside_agent_skill_dir() {
    let (_home, svc, _lock) = fresh_service("distribution-path-guard");
    let src = write_skill(
        &svc.home.join("s"),
        "review",
        "github-code-review",
        Some("review-v1"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let outside_target = svc.home.join("outside").join("github-code-review");
    let mut create_preview = svc
        .preview_distribute_skill(
            vec!["github-code-review".to_string()],
            vec!["claude-code".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    create_preview.changes[0].target_path = outside_target.display().to_string();
    let err = svc.execute_distribute_skill(create_preview, ClaimOrigin::Direct);
    assert!(err.is_err());
    assert!(!outside_target.exists());
    assert_eq!(
        svc.get_skill_detail("github-code-review")
            .unwrap()
            .targets
            .len(),
        0
    );

    fs::create_dir_all(&outside_target).unwrap();
    fs::write(outside_target.join("SKILL.md"), "# user-owned").unwrap();
    let overwrite_preview = DistributionPreview {
        skill_ids: vec!["github-code-review".to_string()],
        target_agents: vec!["claude-code".to_string()],
        requested_mode: "copy".to_string(),
        changes: vec![],
        blockers: vec![ConflictBlocker {
            skill_id: "github-code-review".to_string(),
            agent_id: "claude-code".to_string(),
            reason: "injected blocker".to_string(),
            existing_path: Some(outside_target.display().to_string()),
            existing_path_kind: Some("directory".to_string()),
            resolved_existing_path: None,
        }],
        blocker_decisions: vec![DistributionBlockerDecision {
            skill_id: "github-code-review".to_string(),
            agent_id: "claude-code".to_string(),
            action: "overwrite".to_string(),
        }],
    };

    let err = svc.execute_distribute_skill(overwrite_preview, ClaimOrigin::Direct);
    assert!(err.is_err());
    assert!(
        outside_target.exists(),
        "guard must run before overwrite deletion"
    );
    assert_eq!(
        fs::read_to_string(outside_target.join("SKILL.md")).unwrap(),
        "# user-owned"
    );
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
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    // pack A
    let _pack_a = svc
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

    svc.apply_skill_pack(
        "pack-a",
        vec!["claude-code".to_string()],
        "copy".to_string(),
    )
    .unwrap();
    svc.apply_skill_pack(
        "pack-b",
        vec!["claude-code".to_string()],
        "copy".to_string(),
    )
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
    assert!(
        Path::new(&target_path).exists(),
        "file preserved while pack B active"
    );

    // revoke pack B → last claim → file removed
    let res = svc
        .remove_skill_pack_from_agent("pack-b", "claude-code")
        .unwrap();
    assert_eq!(res.removed_targets, 1);
    assert!(
        !Path::new(&target_path).exists(),
        "file removed when no claim left"
    );
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
            import_mode: None,
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

#[test]
fn updating_applied_pack_auto_syncs_new_members_by_default() {
    let (_home, svc, _lock) = fresh_service("pack-auto-sync");
    let first = write_skill(&svc.home.join("s"), "one", "skill-one", Some("v1"));
    let second = write_skill(&svc.home.join("s"), "two", "skill-two", Some("v1"));
    for src in [first, second] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }
    svc.upsert_skill_pack(UpsertPackInput {
        id: "p-sync".to_string(),
        name: "Sync".to_string(),
        description: "".to_string(),
        tags: vec![],
        skill_ids: vec!["skill-one".to_string()],
    })
    .unwrap();
    svc.apply_skill_pack("p-sync", vec!["codex".to_string()], "copy".to_string())
        .unwrap();

    let updated = svc
        .upsert_skill_pack(UpsertPackInput {
            id: "p-sync".to_string(),
            name: "Sync".to_string(),
            description: "".to_string(),
            tags: vec![],
            skill_ids: vec!["skill-one".to_string(), "skill-two".to_string()],
        })
        .unwrap();

    assert_eq!(updated.sync_status, "synced");
    assert_eq!(updated.pending_sync_count, 0);
    let second_detail = svc.get_skill_detail("skill-two").unwrap();
    assert!(second_detail.targets.iter().any(|target| {
        target.agent_id == "codex"
            && target
                .claims
                .iter()
                .any(|claim| claim.pack_id.as_deref() == Some("p-sync"))
    }));
}

#[test]
fn updating_applied_pack_can_defer_and_manually_sync() {
    let (_home, svc, _lock) = fresh_service("pack-manual-sync");
    svc.update_settings(SettingsUpdate {
        center_path: None,
        sqlite_path: None,
        default_distribute_mode: None,
        link_fail_policy: None,
        startup_scan: None,
        show_unmanaged: None,
        auto_sync_skill_packs: Some(false),
    })
    .unwrap();
    let first = write_skill(&svc.home.join("s"), "one", "skill-one", Some("v1"));
    let second = write_skill(&svc.home.join("s"), "two", "skill-two", Some("v1"));
    for src in [first, second] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }
    svc.upsert_skill_pack(UpsertPackInput {
        id: "p-manual".to_string(),
        name: "Manual".to_string(),
        description: "".to_string(),
        tags: vec![],
        skill_ids: vec!["skill-one".to_string()],
    })
    .unwrap();
    svc.apply_skill_pack("p-manual", vec!["codex".to_string()], "copy".to_string())
        .unwrap();

    let pending = svc
        .upsert_skill_pack(UpsertPackInput {
            id: "p-manual".to_string(),
            name: "Manual".to_string(),
            description: "".to_string(),
            tags: vec![],
            skill_ids: vec!["skill-one".to_string(), "skill-two".to_string()],
        })
        .unwrap();

    assert_eq!(pending.sync_status, "pending");
    assert_eq!(pending.pending_sync_count, 1);
    assert!(svc
        .get_skill_detail("skill-two")
        .unwrap()
        .targets
        .is_empty());

    let result = svc
        .sync_skill_pack_to_agents("p-manual", vec!["codex".to_string()])
        .unwrap();
    assert_eq!(result.status, "synced");
    let synced = svc.get_skill_pack_detail("p-manual").unwrap();
    assert_eq!(synced.sync_status, "synced");
    assert_eq!(synced.pending_sync_count, 0);
    assert_eq!(svc.get_skill_detail("skill-two").unwrap().targets.len(), 1);
}

#[test]
fn apply_pack_accepts_blocker_decisions_for_unmanaged_targets() {
    let (_home, svc, _lock) = fresh_service("pack-blocker-decisions");
    let src = write_skill(
        &svc.home.join("s"),
        "find-skills",
        "find-skills",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let unmanaged = write_skill(
        &svc.home.join(".codex/skills"),
        "find-skills",
        "find-skills",
        Some("agent-version"),
    );

    let preview = svc
        .apply_skill_pack("default", vec!["codex".to_string()], "copy".to_string())
        .unwrap();
    assert_eq!(preview.blockers.len(), 1);
    assert_eq!(
        fs::read_to_string(unmanaged.join("reference.md")).unwrap(),
        "agent-version"
    );

    let applied = svc
        .apply_skill_pack_with_decisions(
            "default",
            vec!["codex".to_string()],
            "copy".to_string(),
            vec![DistributionBlockerDecision {
                skill_id: "find-skills".to_string(),
                agent_id: "codex".to_string(),
                action: "overwrite".to_string(),
            }],
        )
        .unwrap();
    assert!(applied.blockers.is_empty());
    assert!(applied
        .changes
        .iter()
        .any(|change| change.action == "overwrite"));
    assert_eq!(
        fs::read_to_string(unmanaged.join("reference.md")).unwrap(),
        "center-version"
    );

    let detail = svc.get_skill_detail("find-skills").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert!(detail.targets[0]
        .claims
        .iter()
        .any(|claim| claim.claim_type == "pack" && claim.pack_id.as_deref() == Some("default")));
}

#[test]
fn default_pack_is_virtual_and_always_contains_all_center_skills() {
    let (_home, svc, _lock) = fresh_service("default-pack-virtual");
    let first = write_skill(&svc.home.join("s"), "one", "skill-one", Some("v1"));
    let second = write_skill(&svc.home.join("s"), "two", "skill-two", Some("v1"));
    for src in [first, second] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }

    let packs = svc.list_skill_packs().unwrap();
    let default = packs.iter().find(|pack| pack.id == "default").unwrap();
    assert_eq!(default.name, "全量技能包");
    assert_eq!(default.member_count, 2);
    assert!(default.healthy);

    let detail = svc.get_skill_pack_detail("default").unwrap();
    assert_eq!(detail.members.len(), 2);
    assert!(detail
        .members
        .iter()
        .any(|member| member.skill_id == "skill-one"));
    assert!(detail
        .members
        .iter()
        .any(|member| member.skill_id == "skill-two"));

    let err = svc
        .upsert_skill_pack(UpsertPackInput {
            id: "default".to_string(),
            name: "Changed".to_string(),
            description: "".to_string(),
            tags: vec![],
            skill_ids: vec!["skill-one".to_string()],
        })
        .unwrap_err();
    assert!(err.contains("cannot be edited"));
}

#[test]
fn revoking_default_pack_preserves_direct_claims() {
    let (_home, svc, _lock) = fresh_service("default-pack-revoke-direct");
    let first = write_skill(&svc.home.join("s"), "one", "skill-one", Some("v1"));
    let second = write_skill(&svc.home.join("s"), "two", "skill-two", Some("v1"));
    for src in [first, second] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }

    let direct_preview = svc
        .preview_distribute_skill(
            vec!["skill-one".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(direct_preview, ClaimOrigin::Direct)
        .unwrap();
    svc.apply_skill_pack("default", vec!["codex".to_string()], "copy".to_string())
        .unwrap();

    let one = svc.get_skill_detail("skill-one").unwrap();
    assert_eq!(one.targets[0].claims.len(), 2);
    let two = svc.get_skill_detail("skill-two").unwrap();
    assert_eq!(two.targets[0].claims.len(), 1);

    let res = svc
        .remove_skill_pack_from_agent("default", "codex")
        .unwrap();
    assert_eq!(res.removed_claims, 2);
    assert_eq!(res.preserved_targets, 1);
    assert_eq!(res.removed_targets, 1);

    let one = svc.get_skill_detail("skill-one").unwrap();
    assert_eq!(one.targets.len(), 1);
    assert_eq!(one.targets[0].claims.len(), 1);
    assert_eq!(one.targets[0].claims[0].claim_type, "direct");
    assert!(svc
        .get_skill_detail("skill-two")
        .unwrap()
        .targets
        .is_empty());
}

#[test]
fn revoking_default_pack_keeps_agent_detail_and_overview_readable() {
    let (_home, svc, _lock) = fresh_service("default-pack-revoke-refresh");
    let first = write_skill(&svc.home.join("s"), "one", "skill-one", Some("v1"));
    let second = write_skill(&svc.home.join("s"), "two", "skill-two", Some("v1"));
    for src in [first, second] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }

    svc.apply_skill_pack("default", vec!["hermes".to_string()], "link".to_string())
        .unwrap();
    let detail = svc.get_agent_detail("hermes").unwrap();
    assert_eq!(detail.applied_packs.len(), 1);
    assert_eq!(detail.applied_packs[0].pack_name, "全量技能包");

    svc.remove_skill_pack_from_agent("default", "hermes")
        .unwrap();

    let detail = svc.get_agent_detail("hermes").unwrap();
    assert!(detail.applied_packs.is_empty());
    let overview = svc.overview().unwrap();
    let default_pack = overview
        .packs
        .iter()
        .find(|pack| pack.id == "default")
        .unwrap();
    assert_eq!(default_pack.applied_agent_count, 0);
}

#[test]
fn remove_skill_from_applied_pack_can_keep_standalone() {
    let (_home, svc, _lock) = fresh_service("pack-keep-standalone");
    let src = write_skill(&svc.home.join("s"), "lint", "lint-helper", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    svc.upsert_skill_pack(UpsertPackInput {
        id: "p-keep".to_string(),
        name: "Keep".to_string(),
        description: "".to_string(),
        tags: vec![],
        skill_ids: vec!["lint-helper".to_string()],
    })
    .unwrap();
    svc.apply_skill_pack(
        "p-keep",
        vec!["claude-code".to_string()],
        "copy".to_string(),
    )
    .unwrap();

    let before = svc
        .preview_remove_skill_from_pack("p-keep", "lint-helper")
        .unwrap();
    assert_eq!(before.applied_agent_count, 1);
    assert_eq!(before.affected_targets.len(), 1);

    svc.remove_skill_from_pack("p-keep", "lint-helper", false)
        .unwrap();
    let pack = svc.get_skill_pack_detail("p-keep").unwrap();
    assert!(pack.members.is_empty());
    let detail = svc.get_skill_detail("lint-helper").unwrap();
    assert_eq!(detail.targets.len(), 1, "target kept as standalone install");
    assert!(Path::new(&detail.targets[0].target_path).exists());
    assert!(detail.targets[0]
        .claims
        .iter()
        .any(|c| c.claim_type == "direct"));
    assert!(!detail.targets[0]
        .claims
        .iter()
        .any(|c| c.pack_id.as_deref() == Some("p-keep")));
}

// ── Copy sync: outdated / modified / diverged ───────────────────

#[test]
fn copy_sync_detects_outdated_modified_diverged() {
    let (_home, svc, _lock) = fresh_service("sync");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
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
    svc.execute_distribute_skill(p, ClaimOrigin::Direct)
        .unwrap();
    let target_id = svc.get_skill_detail("release-checklist").unwrap().targets[0]
        .id
        .clone();

    // center-only change → outdated
    let center = svc.center_path().unwrap().join("release-checklist");
    fs::write(center.join("reference.md"), "center-updated").unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "copy_outdated");
    assert_eq!(prev.suggested, "center_over_agent");

    // execute center_over_agent → resolves
    svc.execute_sync_copy_target(&target_id, "center_over_agent")
        .unwrap();
    let prev = svc.preview_sync_copy_target(&target_id).unwrap();
    assert_eq!(prev.state, "ok");

    // agent-only change → modified
    let target_path = svc.get_skill_detail("release-checklist").unwrap().targets[0]
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

#[test]
fn copy_target_diff_lists_changed_files_against_center() {
    let (_home, svc, _lock) = fresh_service("copy-diff");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    let target = detail.targets[0].clone();
    let center = svc.center_path().unwrap().join("release-checklist");
    let copy = Path::new(&target.target_path);

    fs::write(center.join("center-only.md"), "only in center").unwrap();
    fs::write(copy.join("reference.md"), "edited in agent").unwrap();
    fs::write(copy.join("copy-only.md"), "only in copy").unwrap();

    let diff = svc.preview_copy_target_diff(&target.id).unwrap();
    let files: Vec<_> = diff
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.change_type.as_str()))
        .collect();

    assert_eq!(diff.state, "copy_diverged");
    assert!(files.contains(&("center-only.md", "copy_removed")));
    assert!(files.contains(&("reference.md", "modified")));
    assert!(files.contains(&("copy-only.md", "copy_added")));
    let modified = diff
        .files
        .iter()
        .find(|file| file.path == "reference.md")
        .unwrap();
    assert_eq!(modified.center_content.as_deref(), Some("original"));
    assert_eq!(modified.copy_content.as_deref(), Some("edited in agent"));
}

#[test]
fn list_center_skills_refreshes_live_copy_status() {
    let (_home, svc, _lock) = fresh_service("list-refresh-copy-status");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["hermes".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    let target_path = Path::new(&detail.targets[0].target_path);
    fs::write(target_path.join("reference.md"), "edited in agent").unwrap();

    let summary = svc
        .list_center_skills()
        .unwrap()
        .into_iter()
        .find(|skill| skill.id == "release-checklist")
        .unwrap();

    assert_eq!(summary.status, "copyDiverged");
    assert_eq!(summary.installed_agents[0].status, "copy_modified");
}

#[test]
fn refresh_overview_returns_fresh_copy_status_without_second_live_list_refresh() {
    let (_home, svc, _lock) = fresh_service("refresh-overview-copy-status");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    fs::write(
        Path::new(&detail.targets[0].target_path).join("reference.md"),
        "edited in agent",
    )
    .unwrap();

    let overview = svc.refresh_overview().unwrap();
    let summary = overview
        .skills
        .into_iter()
        .find(|skill| skill.id == "release-checklist")
        .unwrap();

    assert_eq!(summary.status, "copyDiverged");
    assert_eq!(summary.installed_agents[0].status, "copy_modified");
}

#[test]
fn overview_uses_cached_target_status_without_live_filesystem_refresh() {
    let (_home, svc, _lock) = fresh_service("overview-cached-copy-status");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    fs::write(
        Path::new(&detail.targets[0].target_path).join("reference.md"),
        "edited in agent",
    )
    .unwrap();

    let overview = svc.overview().unwrap();
    let summary = overview
        .skills
        .into_iter()
        .find(|skill| skill.id == "release-checklist")
        .unwrap();

    assert_eq!(summary.status, "ok");
    assert_eq!(summary.installed_agents[0].status, "ok");
}

#[test]
fn delete_skill_target_distribution_removes_copy_and_db_target() {
    let (_home, svc, _lock) = fresh_service("delete-target-distribution");
    let src = write_skill(
        &svc.home.join("s"),
        "rev",
        "release-checklist",
        Some("original"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let target = svc.get_skill_detail("release-checklist").unwrap().targets[0].clone();
    let target_path = Path::new(&target.target_path);
    assert!(target_path.exists());

    svc.delete_skill_target_distribution(&target.id).unwrap();

    assert!(!target_path.exists());
    let detail = svc.get_skill_detail("release-checklist").unwrap();
    assert!(detail.targets.is_empty());
    assert!(detail.summary.installed_agents.is_empty());
}

#[test]
fn delete_skill_target_distributions_removes_multiple_targets() {
    let (_home, svc, _lock) = fresh_service("delete-target-distributions");
    let src_one = write_skill(
        &svc.home.join("s1"),
        "rev",
        "release-checklist",
        Some("one"),
    );
    let src_two = write_skill(&svc.home.join("s2"), "rev", "deploy-guide", Some("two"));
    for source_path in [src_one, src_two] {
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: source_path.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }
    let preview = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string(), "deploy-guide".to_string()],
            vec!["codex".to_string()],
            "copy".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(preview, ClaimOrigin::Direct)
        .unwrap();
    let first = svc.get_skill_detail("release-checklist").unwrap().targets[0].clone();
    let second = svc.get_skill_detail("deploy-guide").unwrap().targets[0].clone();
    let first_path = Path::new(&first.target_path);
    let second_path = Path::new(&second.target_path);
    assert!(first_path.exists());
    assert!(second_path.exists());

    let result = svc
        .delete_skill_target_distributions(vec![first.id.clone(), second.id.clone()])
        .unwrap();

    assert_eq!(result.deleted, 2);
    assert!(result.failures.is_empty());
    assert!(!first_path.exists());
    assert!(!second_path.exists());
    assert!(svc
        .get_skill_detail("release-checklist")
        .unwrap()
        .targets
        .is_empty());
    assert!(svc
        .get_skill_detail("deploy-guide")
        .unwrap()
        .targets
        .is_empty());
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
            import_mode: None,
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
    svc.execute_distribute_skill(p, ClaimOrigin::Direct)
        .unwrap();

    let preview = svc
        .preview_delete_center_skill("release-checklist")
        .unwrap();
    assert!(!preview.removable, "linked targets block removal");
    assert_eq!(preview.affected_targets.len(), 1);

    let target_path = PathBuf::from(&preview.affected_targets[0].target_path);
    assert!(target_path.is_symlink());

    // default action keeps Agent installs by turning links into standalone copies.
    svc.execute_delete_center_skill("release-checklist", false)
        .unwrap();
    assert!(svc.get_skill_detail("release-checklist").is_err());
    assert!(target_path.is_dir());
    assert!(!target_path.is_symlink());
    assert!(target_path.join("SKILL.md").exists());
    assert_eq!(
        svc.list_unmanaged()
            .unwrap()
            .iter()
            .filter(|item| item.path == target_path.display().to_string())
            .count(),
        1
    );
}

#[test]
fn delete_center_skill_can_remove_agent_installs() {
    let (_home, svc, _lock) = fresh_service("delete-remove-targets");
    let src = write_skill(&svc.home.join("s"), "rev", "release-checklist", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
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
    svc.execute_distribute_skill(p, ClaimOrigin::Direct)
        .unwrap();

    let preview = svc
        .preview_delete_center_skill("release-checklist")
        .unwrap();
    let target_path = PathBuf::from(&preview.affected_targets[0].target_path);
    assert!(target_path.is_symlink());

    svc.execute_delete_center_skill("release-checklist", true)
        .unwrap();
    assert!(svc.get_skill_detail("release-checklist").is_err());
    assert!(!target_path.exists());
}

#[test]
fn delete_center_skills_batch_preserves_agent_installs_once() {
    let (_home, svc, _lock) = fresh_service("delete-batch");
    for skill_id in ["release-checklist", "deploy-guide"] {
        let src = write_skill(
            &svc.home.join("s").join(skill_id),
            "rev",
            skill_id,
            Some("v1"),
        );
        svc.execute_add_center_skill(
            AddCenterSkillInput {
                source_path: src.display().to_string(),
                source_type: "local_folder".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    }
    let p = svc
        .preview_distribute_skill(
            vec!["release-checklist".to_string(), "deploy-guide".to_string()],
            vec!["claude-code".to_string()],
            "link".to_string(),
        )
        .unwrap();
    svc.execute_distribute_skill(p, ClaimOrigin::Direct)
        .unwrap();

    let preview = svc
        .preview_delete_center_skills(vec![
            "release-checklist".to_string(),
            "deploy-guide".to_string(),
        ])
        .unwrap();
    assert_eq!(preview.skill_ids.len(), 2);
    assert_eq!(preview.affected_targets.len(), 2);
    let target_paths = preview
        .affected_targets
        .iter()
        .map(|target| PathBuf::from(&target.target_path))
        .collect::<Vec<_>>();

    svc.execute_delete_center_skills(
        vec!["release-checklist".to_string(), "deploy-guide".to_string()],
        false,
    )
    .unwrap();

    assert!(svc.get_skill_detail("release-checklist").is_err());
    assert!(svc.get_skill_detail("deploy-guide").is_err());
    for target_path in target_paths {
        assert!(target_path.is_dir());
        assert!(!target_path.is_symlink());
        assert!(target_path.join("SKILL.md").exists());
    }
}

// ── Adopt unmanaged agent skill ─────────────────────────────────

#[test]
fn missing_unmanaged_adopt_record_returns_stable_error_code() {
    let (_home, svc, _lock) = fresh_service("adopt-missing-unmanaged");

    let err = svc
        .preview_adopt_agent_skill("claude-code", "unm-stale-record")
        .unwrap_err();

    assert!(
        err.starts_with("SKILL_UNMANAGED_STALE:"),
        "expected stable stale unmanaged error, got {err}"
    );
    assert!(
        !err.contains("Query returned no rows"),
        "database no-row error should not leak to callers"
    );
}

#[test]
fn adopt_import_keep_tracks_agent_skill_and_validates_option() {
    let (_home, svc, _lock) = fresh_service("adopt-keep");
    let rogue = write_skill(
        &svc.home.join(".claude/skills"),
        "rogue",
        "rogue-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| u.path == rogue.display().to_string())
        .expect("unmanaged skill found");

    let preview = svc
        .preview_adopt_agent_skill("claude-code", &unmanaged.id)
        .unwrap();
    assert!(preview.options.iter().any(|o| o.value == "import_keep"));
    assert!(svc
        .execute_adopt_agent_skill("claude-code", &unmanaged.id, "overwrite_center", None)
        .is_err());

    let adopted = svc
        .execute_adopt_agent_skill("claude-code", &unmanaged.id, "import_keep", None)
        .unwrap();
    assert_eq!(adopted, "rogue-skill");
    let detail = svc.get_skill_detail("rogue-skill").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].agent_id, "claude-code");
    assert_eq!(detail.targets[0].actual_mode, "copy");
    assert!(svc
        .list_unmanaged()
        .unwrap()
        .iter()
        .all(|u| u.id != unmanaged.id));
}

#[test]
fn delete_unmanaged_agent_skill_removes_local_copy_without_importing_center() {
    let (_home, svc, _lock) = fresh_service("delete-unmanaged-agent");
    let rogue = write_skill(
        &svc.home.join(".workbuddy/skills"),
        "rogue",
        "rogue-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| {
            u.agent_id.as_deref() == Some("workbuddy") && u.path == rogue.display().to_string()
        })
        .expect("workbuddy unmanaged skill found");

    svc.delete_unmanaged_agent_skill("workbuddy", &unmanaged.id)
        .unwrap();

    assert!(!rogue.exists());
    assert!(svc.get_skill_detail("rogue-skill").is_err());
    assert!(svc
        .list_unmanaged()
        .unwrap()
        .iter()
        .all(|u| u.id != unmanaged.id));
}

#[test]
fn delete_unmanaged_agent_skill_rejects_wrong_agent() {
    let (_home, svc, _lock) = fresh_service("delete-unmanaged-wrong-agent");
    let rogue = write_skill(
        &svc.home.join(".claude/skills"),
        "rogue",
        "rogue-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| {
            u.agent_id.as_deref() == Some("claude-code") && u.path == rogue.display().to_string()
        })
        .expect("claude unmanaged skill found");

    let err = svc
        .delete_unmanaged_agent_skill("workbuddy", &unmanaged.id)
        .unwrap_err();

    assert!(err.contains("does not belong to agent"));
    assert!(rogue.exists());
}

#[test]
fn adopt_shared_agents_skill_copies_to_center_and_removes_source() {
    let (_home, svc, _lock) = fresh_service("adopt-shared-cleanup");
    let shared = write_skill(
        &svc.home.join(".agents/skills"),
        "shared-alpha",
        "shared-alpha",
        Some("v1"),
    );
    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| u.path == shared.display().to_string())
        .expect("shared unmanaged skill found");

    let preview = svc
        .preview_adopt_agent_skill("agents", &unmanaged.id)
        .unwrap();
    assert!(preview.options.iter().any(|o| o.value == "import_cleanup"));

    let adopted = svc
        .execute_adopt_agent_skill("agents", &unmanaged.id, "import_cleanup", None)
        .unwrap();

    assert_eq!(adopted, "shared-alpha");
    assert!(!shared.exists());
    let center_skill = fsutil::default_center_path().join("shared-alpha");
    assert!(center_skill.is_dir());
    assert!(fs::read_to_string(center_skill.join("SKILL.md"))
        .unwrap()
        .contains("# shared-alpha"));
    let detail = svc.get_skill_detail("shared-alpha").unwrap();
    assert!(detail.targets.is_empty());
    assert!(svc
        .list_unmanaged()
        .unwrap()
        .iter()
        .all(|u| u.id != unmanaged.id));
}

#[test]
fn diagnosis_recommends_cleaning_managed_shared_agents_skill() {
    let (_home, svc, _lock) = fresh_service("diag-managed-shared");
    let src = write_skill(&svc.home.join("s"), "shared", "shared-skill", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let shared = svc.home.join(".agents/skills/shared-skill");
    let center = svc.center_path().unwrap().join("shared-skill");
    fsutil::copy_dir_recursive(&center, &shared).unwrap();
    let source_hash = fsutil::hash_dir(&center);
    svc.db()
        .with_conn(|c| {
            c.execute(
                "INSERT INTO skill_targets(id, skill_id, agent_id, target_path, install_mode, actual_mode, source_hash, current_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, 'agents', ?3, 'copy', 'copy', ?4, ?5, 'ok', ?6, ?6)",
                params![
                    "tgt-shared-dup",
                    "shared-skill",
                    shared.display().to_string(),
                    source_hash,
                    fsutil::hash_dir(&shared),
                    "2026-01-01T00:00:00Z",
                ],
            )
            .map_err(|e| e.to_string())
        })
        .unwrap();

    let issues = crate::skills::v2::diagnosis::run(&svc).unwrap();
    let issue = issues
        .iter()
        .find(|issue| issue.issue_type == "agents_managed_duplicate")
        .expect("managed .agents duplicate should be diagnosed");
    assert_eq!(issue.fix_kind, "auto");
    assert_eq!(issue.entity_id.as_deref(), Some("tgt-shared-dup"));

    let fixed = crate::skills::v2::diagnosis::execute_safe_fixes(&svc).unwrap();
    assert!(fixed >= 1);
    assert!(
        !shared.exists(),
        "safe fix removes the managed shared .agents copy"
    );
    let detail = svc.get_skill_detail("shared-skill").unwrap();
    assert!(detail.targets.is_empty());
    assert!(center.is_dir(), "center library skill is preserved");
}

#[test]
#[cfg(unix)]
fn adopt_conflict_can_use_center_as_source_of_truth() {
    let (_home, svc, _lock) = fresh_service("adopt-center-over-agent");
    let center_src = write_skill(
        &svc.home.join("incoming"),
        "agentbro-pet",
        "AgentBroPet",
        Some("center-version"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: center_src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: Some("file://incoming/agentbro-pet".to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();

    let agent_skill = write_skill(
        &svc.home.join(".codex/skills"),
        "agentbro-pet",
        "AgentBroPet",
        Some("agent-version"),
    );
    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| u.path == agent_skill.display().to_string())
        .expect("conflicting unmanaged skill found");

    let preview = svc
        .preview_adopt_agent_skill("codex", &unmanaged.id)
        .unwrap();
    assert!(preview.center_has_same_id);
    assert!(!preview.can_quick_adopt);
    assert_eq!(preview.options[0].value, "center_over_agent");

    let adopted = svc
        .execute_adopt_agent_skill("codex", &unmanaged.id, "center_over_agent", None)
        .unwrap();
    assert_eq!(adopted, "AgentBroPet");

    let center = svc.center_path().unwrap().join("AgentBroPet");
    assert_eq!(
        fs::read_to_string(center.join("reference.md")).unwrap(),
        "center-version"
    );
    assert!(agent_skill.is_symlink());
    assert_eq!(fs::read_link(&agent_skill).unwrap(), center);

    let detail = svc.get_skill_detail("AgentBroPet").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].agent_id, "codex");
    assert_eq!(detail.targets[0].actual_mode, "link");
    assert!(svc
        .list_unmanaged()
        .unwrap()
        .iter()
        .all(|u| u.id != unmanaged.id));
}

#[test]
#[cfg(unix)]
fn adopt_import_keep_preserves_symlink_mode_and_reports_resolved_path() {
    let (_home, svc, _lock) = fresh_service("adopt-keep-symlink");
    let source = write_skill(
        &svc.home.join(".skills-manager/skills"),
        "kuaifa",
        "kuaifa",
        Some("v1"),
    );
    let agent_dir = svc.home.join(".claude/skills");
    fs::create_dir_all(&agent_dir).unwrap();
    let link = agent_dir.join("kuaifa");
    std::os::unix::fs::symlink(&source, &link).unwrap();

    svc.refresh().unwrap();
    let unmanaged = svc
        .list_unmanaged()
        .unwrap()
        .into_iter()
        .find(|u| u.path == link.display().to_string())
        .expect("unmanaged symlink skill found");

    let adopted = svc
        .execute_adopt_agent_skill("claude-code", &unmanaged.id, "import_keep", None)
        .unwrap();
    assert_eq!(adopted, "kuaifa");

    let detail = svc.get_skill_detail("kuaifa").unwrap();
    assert_eq!(detail.targets.len(), 1);
    assert_eq!(detail.targets[0].actual_mode, "link");
    assert_eq!(
        detail.targets[0].resolved_target_path.as_deref(),
        Some(source.display().to_string().as_str())
    );

    svc.db
        .with_conn(|c| {
            c.execute(
                "UPDATE skill_targets SET actual_mode = 'copy' WHERE skill_id = 'kuaifa'",
                [],
            )
            .map_err(|e| e.to_string())
        })
        .unwrap();
    let refreshed = svc.get_skill_detail("kuaifa").unwrap();
    assert_eq!(refreshed.targets[0].actual_mode, "link");
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
    write_skill(
        &home.path.join(".agentbro/skills"),
        "legacy-skill",
        "legacy-skill",
        Some("v1"),
    );

    let sqlite = fsutil::default_sqlite_path();
    let svc = Service::new(&sqlite, fsutil::home()).unwrap();
    svc.init().unwrap();

    let packs = svc.list_skill_packs().unwrap();
    assert!(packs.iter().any(|p| p.id == "legacy-pack"));
    let detail = svc.get_skill_pack_detail("legacy-pack").unwrap();
    assert!(detail
        .members
        .iter()
        .any(|m| m.skill_id == "legacy-skill" && !m.missing));
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
            import_mode: None,
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
    svc.execute_distribute_skill(p, ClaimOrigin::Direct)
        .unwrap();
    let target_path = svc.get_skill_detail("release-checklist").unwrap().targets[0]
        .target_path
        .clone();
    let center = svc.center_path().unwrap().join("release-checklist");
    fs::remove_dir_all(&center).unwrap();

    // add an unmanaged skill to codex
    write_skill(
        &svc.home.join(".codex/skills"),
        "rogue",
        "rogue-skill",
        Some("v1"),
    );

    svc.refresh().unwrap();
    let issues = crate::skills::v2::diagnosis::run(&svc).unwrap();
    let types: Vec<_> = issues.iter().map(|i| i.issue_type.clone()).collect();
    assert!(
        types
            .iter()
            .any(|t| t == "broken_link" || t == "target_missing"),
        "found: {:?}",
        types
    );
    assert!(
        types.iter().any(|t| t == "agent_unmanaged"),
        "found: {:?}",
        types
    );

    // safe fixes clear broken link / orphan records
    let _fixed = crate::skills::v2::diagnosis::execute_safe_fixes(&svc).unwrap();
    assert!(
        !Path::new(&target_path).is_symlink(),
        "broken link file is removed during safe fix"
    );
    let issues2 = crate::skills::v2::diagnosis::run(&svc).unwrap();
    let types2: Vec<_> = issues2.iter().map(|i| i.issue_type.clone()).collect();
    assert!(
        !types2.iter().any(|t| t == "broken_link"),
        "broken link auto-cleared"
    );
}

#[test]
fn agent_inventory_includes_unmanaged_agent_skill_items() {
    let (_home, svc, _lock) = fresh_service("inventory-unmanaged");
    let rogue = write_skill(
        &svc.home.join(".codex/skills"),
        "rogue",
        "rogue-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();

    let inventory = svc.list_agent_skill_inventory().unwrap();
    let codex = inventory
        .into_iter()
        .find(|agent| agent.agent_id == "codex")
        .expect("codex inventory");

    assert_eq!(codex.unmanaged_count, 1);
    assert!(codex.items.iter().any(|item| {
        !item.managed && item.path == rogue.display().to_string() && item.status == "unmanaged"
    }));
}

#[test]
fn agent_detail_reports_mcp_plugins_and_path_health() {
    let (_home, svc, _lock) = fresh_service("agent-detail-health");
    let claude_dir = svc.home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({
            "mcpServers": {
                "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"] },
                "broken": { "args": ["missing-command"] }
            }
        })
        .to_string(),
    )
    .unwrap();

    let plugin_manifest =
        claude_dir.join("plugins/cache/agentbro/reviewer/.claude-plugin/plugin.json");
    fs::create_dir_all(plugin_manifest.parent().unwrap()).unwrap();
    fs::write(
        &plugin_manifest,
        serde_json::json!({
            "name": "reviewer",
            "displayName": "Reviewer Tools",
            "version": "1.2.3"
        })
        .to_string(),
    )
    .unwrap();

    let detail = svc.get_agent_detail("claude-code").unwrap();
    assert_eq!(
        detail.config_path,
        Some(claude_dir.join("settings.json").display().to_string())
    );
    assert_eq!(
        detail.plugin_dir,
        Some(claude_dir.join("plugins/cache").display().to_string())
    );
    let filesystem = detail
        .mcp_servers
        .iter()
        .find(|server| server.name == "filesystem")
        .expect("filesystem mcp server");
    assert!(filesystem.valid);
    assert_eq!(filesystem.command, "npx");
    assert!(detail
        .mcp_servers
        .iter()
        .any(|server| server.name == "broken" && !server.valid));
    assert!(detail.plugins.iter().any(|plugin| {
        plugin.id == "reviewer"
            && plugin.name == "Reviewer Tools"
            && plugin.version.as_deref() == Some("1.2.3")
    }));
    assert!(detail
        .health
        .iter()
        .any(|issue| issue.kind == "skills_dir_missing"));
}

#[test]
fn agent_detail_reports_codex_toml_config_paths() {
    let (_home, svc, _lock) = fresh_service("codex-config-paths");
    let codex_dir = svc.home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(
        codex_dir.join("config.toml"),
        r#"[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]

[plugins."documents@openai-primary-runtime"]
enabled = true

[plugins."archived@openai-curated"]
enabled = false
"#,
    )
    .unwrap();
    let plugin_manifest = codex_dir.join(
        "plugins/cache/openai-primary-runtime/documents/26.614.11602/.codex-plugin/plugin.json",
    );
    fs::create_dir_all(plugin_manifest.parent().unwrap()).unwrap();
    fs::write(
        &plugin_manifest,
        serde_json::json!({
            "name": "documents",
            "version": "26.614.11602",
            "interface": { "displayName": "Documents" }
        })
        .to_string(),
    )
    .unwrap();
    let disabled_plugin_manifest =
        codex_dir.join("plugins/cache/openai-curated/archived/0.1.0/.codex-plugin/plugin.json");
    fs::create_dir_all(disabled_plugin_manifest.parent().unwrap()).unwrap();
    fs::write(
        &disabled_plugin_manifest,
        serde_json::json!({
            "name": "archived",
            "version": "0.1.0",
            "interface": { "displayName": "Archived Plugin" }
        })
        .to_string(),
    )
    .unwrap();

    let detail = svc.get_agent_detail("codex").unwrap();
    assert_eq!(
        detail.config_path,
        Some(codex_dir.join("config.toml").display().to_string())
    );
    assert_eq!(
        detail.mcp_config_path,
        Some(codex_dir.join("config.toml").display().to_string())
    );
    assert_eq!(
        detail.plugin_dir,
        Some(codex_dir.join("plugins/cache").display().to_string())
    );
    assert!(detail
        .mcp_servers
        .iter()
        .any(|server| server.name == "filesystem" && server.command == "npx"));
    assert!(detail.plugins.iter().any(|plugin| {
        plugin.id == "documents@openai-primary-runtime"
            && plugin.name == "Documents"
            && plugin.version.as_deref() == Some("26.614.11602")
            && plugin.enabled
            && plugin.source.as_deref() == Some("codex-plugin:openai-primary-runtime")
    }));
    assert!(detail
        .plugins
        .iter()
        .any(|plugin| plugin.id == "archived@openai-curated" && !plugin.enabled));
}

#[test]
fn agent_detail_reports_workbuddy_plugins_without_marketplace_noise() {
    let (_home, svc, _lock) = fresh_service("workbuddy-plugins");
    let workbuddy_dir = svc.home.join(".workbuddy");
    fs::create_dir_all(&workbuddy_dir).unwrap();
    fs::write(
        workbuddy_dir.join("settings.json"),
        serde_json::json!({
            "enabledPlugins": {
                "weixinpay@workbuddy-builtin": true,
                "playwright-cli@codebuddy-plugins-official": true,
                "disabled-one@codebuddy-plugins-official": false
            }
        })
        .to_string(),
    )
    .unwrap();

    let plugin_manifest = workbuddy_dir.join(
        "plugins/marketplaces/codebuddy-plugins-official/plugins/playwright-cli/.codebuddy-plugin/plugin.json",
    );
    fs::create_dir_all(plugin_manifest.parent().unwrap()).unwrap();
    fs::write(
        &plugin_manifest,
        serde_json::json!({
            "name": "playwright-cli",
            "displayName": "Playwright CLI",
            "version": "0.1.0"
        })
        .to_string(),
    )
    .unwrap();

    let disabled_manifest = workbuddy_dir.join(
        "plugins/marketplaces/codebuddy-plugins-official/plugins/disabled-one/.codebuddy-plugin/plugin.json",
    );
    fs::create_dir_all(disabled_manifest.parent().unwrap()).unwrap();
    fs::write(
        &disabled_manifest,
        serde_json::json!({
            "name": "disabled-one",
            "displayName": "Disabled One",
            "version": "0.0.1"
        })
        .to_string(),
    )
    .unwrap();

    let unused_manifest = workbuddy_dir.join(
        "plugins/marketplaces/codebuddy-plugins-official/plugins/market-only/.codebuddy-plugin/plugin.json",
    );
    fs::create_dir_all(unused_manifest.parent().unwrap()).unwrap();
    fs::write(
        &unused_manifest,
        serde_json::json!({
            "name": "market-only",
            "displayName": "Market Only",
            "version": "9.9.9"
        })
        .to_string(),
    )
    .unwrap();

    let detail = svc.get_agent_detail("workbuddy").unwrap();
    assert_eq!(
        detail.plugin_dir,
        Some(workbuddy_dir.join("plugins").display().to_string())
    );
    assert_eq!(detail.plugins.len(), 3);
    assert!(detail.plugins.iter().any(|plugin| {
        plugin.id == "weixinpay@workbuddy-builtin"
            && plugin.name == "weixinpay"
            && plugin.enabled
            && plugin.source.as_deref() == Some("workbuddy-plugin:workbuddy-builtin")
    }));
    assert!(detail.plugins.iter().any(|plugin| {
        plugin.id == "playwright-cli@codebuddy-plugins-official"
            && plugin.name == "Playwright CLI"
            && plugin.version.as_deref() == Some("0.1.0")
            && plugin.enabled
            && plugin.source.as_deref() == Some("workbuddy-plugin:codebuddy-plugins-official")
    }));
    assert!(detail.plugins.iter().any(|plugin| {
        plugin.id == "disabled-one@codebuddy-plugins-official" && !plugin.enabled
    }));
    assert!(!detail
        .plugins
        .iter()
        .any(|plugin| plugin.id == "market-only"));
}

#[test]
fn kimi_does_not_claim_agentbro_center_skills() {
    let (_home, svc, _lock) = fresh_service("kimi-center-is-not-kimi");
    write_skill(
        &svc.home.join(".agents/skills"),
        "shared",
        "shared-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();

    let inventory = svc.list_agent_skill_inventory().unwrap();
    let kimi = inventory
        .into_iter()
        .find(|agent| agent.agent_id == "kimi")
        .expect("kimi inventory");

    assert!(!kimi.installed);
    assert_eq!(
        kimi.skills_dir,
        Some(svc.home.join(".kimi/skills").display().to_string())
    );
    assert_eq!(kimi.unmanaged_count, 0);
    assert!(kimi.items.is_empty());
}

#[test]
fn shared_agents_skills_dir_is_scanned_as_own_target() {
    let (_home, svc, _lock) = fresh_service("shared-agents-skills");
    write_skill(
        &svc.home.join(".agents/skills"),
        "shared",
        "shared-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();

    let inventory = svc.list_agent_skill_inventory().unwrap();
    let shared = inventory
        .into_iter()
        .find(|agent| agent.agent_id == "agents")
        .expect(".agents inventory");

    assert!(shared.installed);
    assert_eq!(
        shared.skills_dir,
        Some(svc.home.join(".agents/skills").display().to_string())
    );
    assert_eq!(shared.unmanaged_count, 1);
    assert_eq!(shared.importable_count, 1);
    assert_eq!(
        shared.items[0].path,
        svc.home.join(".agents/skills/shared").display().to_string()
    );
}

#[test]
fn shared_agents_skills_dir_is_not_listed_as_a_managed_agent() {
    let (_home, svc, _lock) = fresh_service("shared-agents-not-agent");
    write_skill(
        &svc.home.join(".agents/skills"),
        "shared",
        "shared-skill",
        Some("v1"),
    );
    svc.refresh().unwrap();

    let agents = svc.list_managed_agents().unwrap();

    assert!(!agents.iter().any(|agent| agent.id == "agents"));
    assert!(agents.iter().any(|agent| agent.id == "claude-code"));
}

#[test]
fn custom_agent_metadata_is_listed_and_uses_declared_paths() {
    let (_home, svc, _lock) = fresh_service("custom-agent-metadata");
    let root = svc.home.join(".codefuse/engine/cc");
    let skills_dir = root.join("skills");
    let settings_file = root.join("settings.json");
    let plugin_dir = root.join("plugins/cache");
    write_skill(
        &skills_dir,
        "internal-review",
        "internal-review",
        Some("v1"),
    );
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(&settings_file, r#"{"mcpServers":{}}"#).unwrap();
    fs::create_dir_all(svc.home.join(".agentbro")).unwrap();
    fs::write(
        svc.home.join(".agentbro/metadata.json"),
        serde_json::json!({
            "customAgents": [{
                "id": "antcc",
                "displayName": "AntCC",
                "category": "claude-compatible",
                "globalSkillsDir": skills_dir.display().to_string(),
                "iconName": "claude-code",
                "isEnabled": true,
                "configDir": root.display().to_string(),
                "settingsFile": settings_file.display().to_string(),
                "mcpConfig": settings_file.display().to_string(),
                "pluginDir": plugin_dir.display().to_string()
            }]
        })
        .to_string(),
    )
    .unwrap();

    svc.refresh().unwrap();
    let agents = svc.list_managed_agents().unwrap();
    let custom = agents
        .iter()
        .find(|agent| agent.id == "antcc")
        .expect("custom agent is listed");

    assert_eq!(custom.display_name, "AntCC");
    assert_eq!(custom.icon_key, "claude-code");
    assert!(custom.installed);
    assert_eq!(custom.skills_dir, Some(skills_dir.display().to_string()));
    assert_eq!(custom.unmanaged_skill_count, 1);

    let detail = svc.get_agent_detail("antcc").unwrap();
    assert_eq!(
        detail.config_path,
        Some(settings_file.display().to_string())
    );
    assert_eq!(
        detail.mcp_config_path,
        Some(settings_file.display().to_string())
    );
    assert_eq!(detail.plugin_dir, Some(plugin_dir.display().to_string()));
}

#[test]
fn openclaw_scans_configured_workspace_skills() {
    let (_home, svc, _lock) = fresh_service("openclaw-workspace-skills");
    let openclaw_config_dir = svc.home.join(".openclaw");
    fs::create_dir_all(&openclaw_config_dir).unwrap();
    fs::write(
        openclaw_config_dir.join("openclaw.json"),
        r#"{"agents":{"defaults":{"workspace":"~/custom-openclaw-workspace"}}}"#,
    )
    .unwrap();
    let workspace_skills = svc.home.join("custom-openclaw-workspace/skills");
    write_skill(
        &workspace_skills,
        "workspace-one",
        "workspace-one",
        Some("v1"),
    );
    write_skill(
        &workspace_skills.join("content"),
        "nested-one",
        "nested-one",
        Some("v1"),
    );
    write_skill(
        &svc.home.join(".openclaw/skills"),
        "managed-one",
        "managed-one",
        Some("v1"),
    );

    svc.refresh().unwrap();

    let inventory = svc.list_agent_skill_inventory().unwrap();
    let openclaw = inventory
        .into_iter()
        .find(|agent| agent.agent_id == "openclaw")
        .expect("OpenClaw inventory");
    let paths = openclaw
        .items
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    let workspace_skill_path = svc
        .home
        .join("custom-openclaw-workspace/skills/workspace-one")
        .display()
        .to_string();
    let managed_skill_path = svc
        .home
        .join(".openclaw/skills/managed-one")
        .display()
        .to_string();
    let nested_skill_path = svc
        .home
        .join("custom-openclaw-workspace/skills/content/nested-one")
        .display()
        .to_string();

    assert!(openclaw.installed);
    assert_eq!(
        openclaw.skills_dir,
        Some(workspace_skills.display().to_string())
    );
    assert!(openclaw.unmanaged_count >= 3);
    assert!(paths.contains(&workspace_skill_path));
    assert!(paths.contains(&managed_skill_path));
    assert!(paths.contains(&nested_skill_path));
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
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let project_root = svc.home.join("workspace/snapshot-project");
    fs::create_dir_all(&project_root).unwrap();
    svc.add_project(project_root.display().to_string()).unwrap();
    let path = crate::skills::v2::snapshot::export_to_file(&svc).unwrap();
    assert!(Path::new(&path).exists());
    let snap = crate::skills::v2::snapshot::export(&svc).unwrap();
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
    assert_eq!(snap.skills.len(), 1);
    assert_eq!(snap.projects.len(), 1);
}

#[test]
fn writes_refresh_snapshot_best_effort() {
    let (_home, svc, _lock) = fresh_service("snapshot-auto");
    let src = write_skill(&svc.home.join("s"), "auto", "auto-snapshot", Some("v1"));
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: src.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let path = crate::skills::v2::snapshot::snapshot_path(&svc).unwrap();
    assert!(path.exists(), "mutating writes refresh the JSON snapshot");
}

// ── Project inventory ────────────────────────────────────────────

#[test]
fn project_inventory_scans_codex_and_claude_resources() {
    let (_home, svc, _lock) = fresh_service("project-inventory");
    let root = svc.home.join("workspace/repo");
    write_skill(
        &root.join(".agents/skills"),
        "codex-review",
        "codex-review",
        Some("codex"),
    );
    write_skill(
        &root.join(".claude/skills"),
        "claude-docs",
        "claude-docs",
        Some("claude"),
    );
    fs::create_dir_all(root.join(".codex")).unwrap();
    fs::write(
        root.join(".codex/config.toml"),
        r#"[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[plugins."documents@openai-primary-runtime"]
enabled = true
"#,
    )
    .unwrap();
    fs::write(
        root.join(".mcp.json"),
        serde_json::json!({
            "mcpServers": {
                "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"] }
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(root.join("AGENTS.md"), "# Repo instructions").unwrap();
    fs::write(root.join(".claude/CLAUDE.md"), "# Claude instructions").unwrap();

    let detail = svc.add_project(root.display().to_string()).unwrap();
    assert_eq!(detail.summary.detected_agent_count, 2);
    assert_eq!(detail.summary.skill_count, 2);
    assert_eq!(detail.summary.mcp_count, 2);
    assert_eq!(detail.summary.plugin_count, 1);
    assert_eq!(detail.summary.instruction_count, 2);

    let codex = detail
        .agents
        .iter()
        .find(|agent| agent.agent_id == "codex")
        .expect("codex project agent");
    assert!(codex.skills.iter().any(|skill| skill.id == "codex-review"));
    assert!(codex
        .mcp_servers
        .iter()
        .any(|server| server.name == "context7" && server.command == "npx"));
    assert!(codex
        .plugins
        .iter()
        .any(|plugin| plugin.id == "documents@openai-primary-runtime" && plugin.enabled));

    let claude = detail
        .agents
        .iter()
        .find(|agent| agent.agent_id == "claude-code")
        .expect("claude project agent");
    assert!(claude.skills.iter().any(|skill| skill.id == "claude-docs"));
    assert!(claude
        .mcp_servers
        .iter()
        .any(|server| server.name == "filesystem"));
}

#[test]
fn installs_center_skill_to_project_and_rescans() {
    let (_home, svc, _lock) = fresh_service("project-install");
    let source = write_skill(
        &svc.home.join("incoming"),
        "center-alpha",
        "center-alpha",
        Some("center"),
    );
    svc.execute_add_center_skill(
        AddCenterSkillInput {
            source_path: source.display().to_string(),
            source_type: "local_folder".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        },
        vec![],
    )
    .unwrap();
    let root = svc.home.join("workspace/install-target");
    fs::create_dir_all(&root).unwrap();
    let project = svc.add_project(root.display().to_string()).unwrap();
    let detail = svc
        .install_center_skills_to_project(
            &project.summary.id,
            "codex",
            vec!["center-alpha".to_string()],
            "copy".to_string(),
        )
        .unwrap();

    let target = root.join(".agents/skills/center-alpha");
    assert!(target.join("SKILL.md").is_file());
    let codex = detail
        .agents
        .iter()
        .find(|agent| agent.agent_id == "codex")
        .expect("codex project agent");
    let skill = codex
        .skills
        .iter()
        .find(|skill| skill.id == "center-alpha")
        .expect("installed skill");
    assert_eq!(skill.status, "centerSynced");
}

// ── Settings ─────────────────────────────────────────────────────

#[test]
fn settings_round_trip() {
    let (_home, svc, _lock) = fresh_service("settings");
    assert!(svc
        .settings()
        .unwrap()
        .center_path
        .ends_with(".agentbro/skills"));
    let updated = svc
        .update_settings(SettingsUpdate {
            center_path: None,
            sqlite_path: None,
            default_distribute_mode: Some("copy".to_string()),
            link_fail_policy: Some("copy".to_string()),
            startup_scan: Some(false),
            show_unmanaged: None,
            auto_sync_skill_packs: None,
        })
        .unwrap();
    assert_eq!(updated.default_distribute_mode, "copy");
    assert_eq!(updated.link_fail_policy, "copy");
    assert!(!updated.startup_scan);

    let migrated = svc
        .update_settings(SettingsUpdate {
            center_path: Some(svc.home.join(".agents/skills").display().to_string()),
            sqlite_path: None,
            default_distribute_mode: None,
            link_fail_policy: None,
            startup_scan: None,
            show_unmanaged: None,
            auto_sync_skill_packs: None,
        })
        .unwrap();
    assert!(migrated.center_path.ends_with(".agentbro/skills"));
}

#[test]
#[ignore]
fn real_smoke_against_actual_home() {
    // Uses the REAL home (no TempHome) — mirrors exactly what the app does.
    let home = fsutil::home();
    let sqlite = fsutil::default_sqlite_path();
    eprintln!("HOME={:?} sqlite={:?}", home, sqlite);
    let svc = Service::new(&sqlite, home.clone());
    let svc = match svc {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Service::new FAILED: {e}");
            panic!("{e}")
        }
    };
    if let Err(e) = svc.init() {
        eprintln!("init FAILED: {e}");
        panic!("{e}")
    }
    match svc.overview() {
        Ok(o) => eprintln!(
            "overview OK: {} skills, {} agents, {} packs",
            o.skills.len(),
            o.agents.len(),
            o.packs.len()
        ),
        Err(e) => {
            eprintln!("overview FAILED: {e}");
            panic!("{e}")
        }
    }
    // also try get_agent_detail for the first agent
    if let Ok(o) = svc.overview() {
        if let Some(a) = o.agents.first() {
            match svc.get_agent_detail(&a.id) {
                Ok(d) => eprintln!("agent detail {} OK: {} skills", a.id, d.skills.len()),
                Err(e) => eprintln!("agent detail {} FAILED: {e}", a.id),
            }
        }
    }
}

#[test]
fn add_center_skill_from_zip() {
    let (_home, svc, _lock) = fresh_service("zip");
    // build a skill dir, then zip it
    let skill_dir = write_skill(
        &svc.home.join("src"),
        "zipped",
        "zip-skill",
        Some("payload"),
    );
    let zip_path = svc.home.join("zip-skill.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("zip-skill/SKILL.md", opts).unwrap();
    zip.write_all(b"---\nname: zip-skill\ndescription: zipped\n---\n# zip")
        .unwrap();
    zip.start_file("zip-skill/reference.md", opts).unwrap();
    zip.write_all(b"payload").unwrap();
    zip.finish().unwrap();
    let _ = &skill_dir;

    let preview = svc
        .preview_add_center_skill(AddCenterSkillInput {
            source_path: zip_path.display().to_string(),
            source_type: "archive".to_string(),
            source_uri: None,
            imported_from_agent: None,
            imported_from_path: None,
            multi: None,
            import_mode: None,
        })
        .unwrap();
    assert_eq!(preview.candidates.len(), 1);
    assert_eq!(preview.candidates[0].skill_id, "zip-skill");
    let r = svc
        .execute_add_center_skill(
            AddCenterSkillInput {
                source_path: zip_path.display().to_string(),
                source_type: "archive".to_string(),
                source_uri: None,
                imported_from_agent: None,
                imported_from_path: None,
                multi: None,
                import_mode: None,
            },
            vec![],
        )
        .unwrap();
    assert_eq!(r.skill_ids, vec!["zip-skill".to_string()]);
    assert!(svc.center_path().unwrap().join("zip-skill").is_dir());
}
