use super::{agent_paths, installer, registry, scanner};
use super::{ConflictResolution, InstallMode, SkillType, SyncPreview, SyncResult, TargetConfig};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn push_to_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta
        .sync
        .as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta
        .sync
        .as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync");
    let _ = fs::remove_dir_all(&tmp_dir);

    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone_result = Command::new(crate::agents::executable::command_path("git"))
        .args([
            "clone",
            "--depth",
            "1",
            &repo_url,
            &tmp_dir.display().to_string(),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone_result.status.success() {
        fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        Command::new(crate::agents::executable::command_path("git"))
            .args(["init"])
            .current_dir(&tmp_dir)
            .output()
            .map_err(|e| e.to_string())?;
        Command::new(crate::agents::executable::command_path("git"))
            .args(["remote", "add", "origin", &repo_url])
            .current_dir(&tmp_dir)
            .output()
            .map_err(|e| e.to_string())?;
    }

    let meta_content = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(tmp_dir.join("metadata.json"), meta_content).map_err(|e| e.to_string())?;

    let skills_src = agent_paths::agentbro_skills_dir();
    if skills_src.is_dir() {
        let skills_dest = tmp_dir.join("skills");
        installer::copy_recursive_pub(&skills_src, &skills_dest)?;
    }

    Command::new(crate::agents::executable::command_path("git"))
        .args(["add", "."])
        .current_dir(&tmp_dir)
        .output()
        .map_err(|e| e.to_string())?;
    Command::new(crate::agents::executable::command_path("git"))
        .args(["commit", "-m", "AgentBro sync"])
        .current_dir(&tmp_dir)
        .output()
        .map_err(|e| e.to_string())?;
    let branch = detect_default_branch(&tmp_dir);
    let push = Command::new(crate::agents::executable::command_path("git"))
        .args(["push", "-u", "origin", &branch])
        .current_dir(&tmp_dir)
        .output()
        .map_err(|e| e.to_string())?;

    let _ = fs::remove_dir_all(&tmp_dir);

    if push.status.success() {
        let mut meta = registry::load();
        if let Some(ref mut sync) = meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = registry::save(&meta);

        Ok(SyncResult {
            success: true,
            message: "推送成功".to_string(),
            conflicts: vec![],
        })
    } else {
        let stderr = String::from_utf8_lossy(&push.stderr).to_string();
        Ok(SyncResult {
            success: false,
            message: format!("推送失败: {}", stderr),
            conflicts: vec![],
        })
    }
}

pub fn pull_from_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta
        .sync
        .as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta
        .sync
        .as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync-pull");
    let _ = fs::remove_dir_all(&tmp_dir);

    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone = Command::new(crate::agents::executable::command_path("git"))
        .args([
            "clone",
            "--depth",
            "1",
            &repo_url,
            &tmp_dir.display().to_string(),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone.status.success() {
        return Ok(SyncResult {
            success: false,
            message: "拉取失败：无法克隆仓库".to_string(),
            conflicts: vec![],
        });
    }

    let remote_meta_path = tmp_dir.join("metadata.json");
    let mut conflicts = Vec::new();
    if remote_meta_path.exists() {
        let content = fs::read_to_string(&remote_meta_path).map_err(|e| e.to_string())?;
        let remote_meta: registry::Metadata =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let mut local_meta = registry::load();
        for pack in remote_meta.packs {
            if let Some(local_pack) = local_meta.packs.iter().find(|p| p.id == pack.id) {
                let local = serde_json::to_string(local_pack).map_err(|e| e.to_string())?;
                let remote = serde_json::to_string(&pack).map_err(|e| e.to_string())?;
                if local != remote {
                    conflicts.push(super::SyncConflict {
                        skill_id: format!("pack:{}", pack.id),
                        local_modified: "local".to_string(),
                        remote_modified: "remote".to_string(),
                    });
                }
            } else {
                local_meta.packs.push(pack);
            }
        }
        for (k, v) in remote_meta.sources {
            if let Some(local_source) = local_meta.sources.get(&k) {
                if local_source.origin != v.origin {
                    conflicts.push(super::SyncConflict {
                        skill_id: k,
                        local_modified: local_source.origin.clone(),
                        remote_modified: v.origin,
                    });
                }
            } else {
                local_meta.sources.insert(k, v);
            }
        }
        if let Some(ref mut sync) = local_meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        if conflicts.is_empty() {
            registry::save(&local_meta)?;
        }
    }

    let remote_skills = tmp_dir.join("skills");
    if remote_skills.is_dir() {
        let local_skills = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&local_skills).map_err(|e| e.to_string())?;
        conflicts.extend(skill_file_conflicts(&remote_skills, &local_skills)?);
        if conflicts.is_empty() {
            installer::copy_recursive_pub(&remote_skills, &local_skills)?;
        }
    }

    if conflicts.is_empty() {
        let _ = fs::remove_dir_all(pending_pull_dir());
    } else {
        persist_pending_pull(&tmp_dir)?;
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    Ok(SyncResult {
        success: conflicts.is_empty(),
        message: if conflicts.is_empty() {
            "拉取成功".to_string()
        } else {
            format!("发现 {} 个同步冲突", conflicts.len())
        },
        conflicts,
    })
}

pub fn resolve_conflicts(resolutions: Vec<ConflictResolution>) -> Result<(), String> {
    if resolutions.iter().any(|item| {
        !matches!(
            item.action.as_str(),
            "keep_local" | "use_remote" | "keep_both"
        )
    }) {
        return Err("Unsupported conflict resolution action".to_string());
    }
    let pending = pending_pull_dir();
    if !pending.exists() {
        return Err("No pending sync conflicts found".to_string());
    }

    let remote_meta = load_pending_metadata(&pending)?;
    let mut local_meta = registry::load();
    let suffix = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();

    for resolution in resolutions {
        if resolution.action == "keep_local" {
            continue;
        }
        if let Some(pack_id) = resolution.skill_id.strip_prefix("pack:") {
            let Some(remote_pack) = remote_meta
                .packs
                .iter()
                .find(|pack| pack.id == pack_id)
                .cloned()
            else {
                continue;
            };
            match resolution.action.as_str() {
                "use_remote" => {
                    if let Some(local_pack) =
                        local_meta.packs.iter_mut().find(|pack| pack.id == pack_id)
                    {
                        *local_pack = remote_pack;
                    } else {
                        local_meta.packs.push(remote_pack);
                    }
                }
                "keep_both" => {
                    let mut copy = remote_pack;
                    copy.id = unique_id(
                        &format!("{}-remote-{}", copy.id, suffix),
                        local_meta.packs.iter().map(|pack| pack.id.as_str()),
                    );
                    copy.name = format!("{} (远端)", copy.name);
                    local_meta.packs.push(copy);
                }
                _ => {}
            }
            continue;
        }

        if let Some(remote_source) = remote_meta.sources.get(&resolution.skill_id) {
            match resolution.action.as_str() {
                "use_remote" => {
                    local_meta
                        .sources
                        .insert(resolution.skill_id.clone(), remote_source.clone());
                }
                "keep_both" => {
                    let key = unique_id(
                        &format!("{}-remote-{}", resolution.skill_id, suffix),
                        local_meta.sources.keys().map(|key| key.as_str()),
                    );
                    local_meta.sources.insert(key, remote_source.clone());
                }
                _ => {}
            }
            continue;
        }

        resolve_skill_file_conflict(&pending, &resolution.skill_id, &resolution.action, &suffix)?;
    }

    registry::save(&local_meta)?;
    let _ = fs::remove_dir_all(pending);
    Ok(())
}

pub fn sync_agent_to_agent(from: &str, to: &str) -> Result<SyncPreview, String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> =
        to_skills.iter().map(|s| s.id.clone()).collect();

    let mut to_copy = 0u32;
    let mut to_skip = 0u32;
    let mut details = Vec::new();

    for skill in &from_skills {
        if to_ids.contains(&skill.id) {
            to_skip += 1;
            details.push(format!("跳过: {} (已存在)", skill.id));
        } else {
            to_copy += 1;
            details.push(format!("复制: {}", skill.id));
        }
    }

    Ok(SyncPreview {
        to_copy,
        to_skip,
        to_update: 0,
        details,
    })
}

pub fn execute_agent_sync(from: &str, to: &str) -> Result<(), String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> =
        to_skills.iter().map(|s| s.id.clone()).collect();

    let targets = vec![TargetConfig {
        agent: to.to_string(),
        install_mode: InstallMode::Direct,
    }];

    for skill in &from_skills {
        if !to_ids.contains(&skill.id) {
            if matches!(skill.skill_type, SkillType::Mcp) {
                if let Some(server_name) = skill.id.strip_prefix("mcp:") {
                    let Some(server) = scanner::read_mcp_server_config(from, server_name) else {
                        continue;
                    };
                    installer::upsert_mcp_server(to, &server)?;
                }
            } else {
                installer::install_skill(&skill.file_path, &targets, &InstallMode::Direct)?;
            }
        }
    }

    Ok(())
}

pub fn export_backup(path: &str) -> Result<(), String> {
    let dest = expand_user_path(path);
    let meta = registry::load();
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;

    let file = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    zip.start_file("metadata.json", options)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut zip, meta_json.as_bytes()).map_err(|e| e.to_string())?;

    let skills_dir = agent_paths::agentbro_skills_dir();
    if skills_dir.is_dir() {
        add_dir_to_zip(&mut zip, &skills_dir, "skills", options)?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn import_backup(path: &str) -> Result<(), String> {
    let file = fs::File::open(expand_user_path(path)).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let extract_dir = std::env::temp_dir().join("agentbro-import");
    let _ = fs::remove_dir_all(&extract_dir);
    archive.extract(&extract_dir).map_err(|e| e.to_string())?;

    let meta_path = extract_dir.join("metadata.json");
    if meta_path.exists() {
        let content = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let imported: registry::Metadata =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let mut local = registry::load();
        for (k, v) in imported.sources {
            local.sources.entry(k).or_insert(v);
        }
        for pack in imported.packs {
            if !local.packs.iter().any(|p| p.id == pack.id) {
                local.packs.push(pack);
            }
        }
        registry::save(&local)?;
    }

    let skills_dir = extract_dir.join("skills");
    if skills_dir.is_dir() {
        let dest = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        installer::copy_recursive_pub(&skills_dir, &dest)?;
    }

    let _ = fs::remove_dir_all(&extract_dir);
    Ok(())
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(rest) = path.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn skill_file_conflicts(
    remote_skills: &Path,
    local_skills: &Path,
) -> Result<Vec<super::SyncConflict>, String> {
    let mut conflicts = Vec::new();
    for entry in fs::read_dir(remote_skills).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let remote = entry.path();
        let local = local_skills.join(entry.file_name());
        if !local.exists() {
            continue;
        }
        let remote_fingerprint = path_fingerprint(&remote)?;
        let local_fingerprint = path_fingerprint(&local)?;
        if remote_fingerprint != local_fingerprint {
            conflicts.push(super::SyncConflict {
                skill_id: entry.file_name().to_string_lossy().to_string(),
                local_modified: local_fingerprint.to_string(),
                remote_modified: remote_fingerprint.to_string(),
            });
        }
    }
    Ok(conflicts)
}

fn path_fingerprint(path: &Path) -> Result<u64, String> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_path(path, path, &mut hasher)?;
    Ok(hasher.finish())
}

fn hash_path(root: &Path, path: &Path, hasher: &mut impl Hasher) -> Result<(), String> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.display().to_string().hash(hasher);
    if path.is_file() {
        fs::read(path).map_err(|e| e.to_string())?.hash(hasher);
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for entry in entries {
        hash_path(root, &entry, hasher)?;
    }
    Ok(())
}

fn detect_default_branch(repo_dir: &std::path::Path) -> String {
    let output = Command::new(crate::agents::executable::command_path("git"))
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(repo_dir)
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let full = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(branch) = full.strip_prefix("origin/") {
                return branch.to_string();
            }
            return full;
        }
    }
    let head = repo_dir.join(".git/refs/heads/main");
    if head.exists() {
        "main".to_string()
    } else {
        "master".to_string()
    }
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
        if path.is_file() {
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let data = fs::read(&path).map_err(|e| e.to_string())?;
            std::io::Write::write_all(zip, &data).map_err(|e| e.to_string())?;
        } else if path.is_dir() {
            add_dir_to_zip(zip, &path, &name, options)?;
        }
    }
    Ok(())
}

fn pending_pull_dir() -> PathBuf {
    agent_paths::agentbro_metadata_path()
        .parent()
        .map(|parent| parent.join("sync").join("pending-pull"))
        .unwrap_or_else(|| std::env::temp_dir().join("agentbro-pending-pull"))
}

fn persist_pending_pull(tmp_dir: &Path) -> Result<(), String> {
    let pending = pending_pull_dir();
    let _ = fs::remove_dir_all(&pending);
    fs::create_dir_all(&pending).map_err(|e| e.to_string())?;
    let metadata = tmp_dir.join("metadata.json");
    if metadata.exists() {
        fs::copy(&metadata, pending.join("metadata.json")).map_err(|e| e.to_string())?;
    }
    let skills = tmp_dir.join("skills");
    if skills.exists() {
        installer::copy_recursive_pub(&skills, &pending.join("skills"))?;
    }
    Ok(())
}

fn load_pending_metadata(pending: &Path) -> Result<registry::Metadata, String> {
    let path = pending.join("metadata.json");
    if !path.exists() {
        return Ok(registry::Metadata::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn resolve_skill_file_conflict(
    pending: &Path,
    skill_id: &str,
    action: &str,
    suffix: &str,
) -> Result<(), String> {
    let remote = pending.join("skills").join(skill_id);
    if !remote.exists() {
        return Ok(());
    }
    let local_dir = agent_paths::agentbro_skills_dir();
    fs::create_dir_all(&local_dir).map_err(|e| e.to_string())?;
    let local = local_dir.join(skill_id);
    match action {
        "use_remote" => {
            remove_path_if_exists(&local)?;
            installer::copy_recursive_pub(&remote, &local)
        }
        "keep_both" => {
            let dest_name = unique_path_name(&local_dir, &format!("{skill_id}-remote-{suffix}"));
            installer::copy_recursive_pub(&remote, &local_dir.join(dest_name))
        }
        _ => Ok(()),
    }
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

fn unique_id<'a>(base: &str, existing: impl Iterator<Item = &'a str>) -> String {
    let existing: std::collections::HashSet<&str> = existing.collect();
    if !existing.contains(base) {
        return base.to_string();
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}-{index}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
        index += 1;
    }
}

fn unique_path_name(dir: &Path, base: &str) -> String {
    if !dir.join(base).exists() {
        return base.to_string();
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}-{index}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        index += 1;
    }
}
