use super::{agent_paths, installer, registry, scanner};
use super::{ConflictResolution, InstallMode, SkillType, SyncPreview, SyncResult, TargetConfig};
use crate::platform::process::background_command;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct GitHubSyncRemote {
    url: String,
    token: String,
    encoded_auth: String,
}

impl GitHubSyncRemote {
    fn configured(metadata: &registry::Metadata) -> Result<Self, String> {
        let config = metadata.sync.as_ref().ok_or("No GitHub sync configured")?;
        let repository = config
            .github_repo
            .as_deref()
            .ok_or("No GitHub repo configured")?;
        let token = config
            .github_token
            .as_deref()
            .ok_or("No GitHub token configured")?;
        Self::new(repository, token)
    }

    fn new(repository: &str, token: &str) -> Result<Self, String> {
        let repository = repository.trim();
        let repository = repository
            .strip_prefix("https://github.com/")
            .unwrap_or(repository);
        let repository = repository.trim_end_matches('/');
        let Some((owner, name)) = repository.split_once('/') else {
            return Err("Use a GitHub repository in owner/repo format".to_string());
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        let valid = |part: &str| {
            !matches!(part, "" | "." | "..")
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if !valid(owner) || !valid(name) {
            return Err("Use a GitHub repository in owner/repo format".to_string());
        }
        let token = token.trim();
        if token.is_empty() {
            return Err("No GitHub token configured".to_string());
        }
        Ok(Self {
            url: format!("https://github.com/{owner}/{name}.git"),
            token: token.to_string(),
            encoded_auth: STANDARD.encode(format!("x-access-token:{token}")),
        })
    }

    fn command(&self) -> Command {
        let mut command = git();
        installer::configure_github_git_auth(&mut command, &self.url, &self.encoded_auth);
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never");
        command
    }

    fn check(&self, operation: &str, output: Output) -> Result<(), String> {
        if output.status.success() {
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr)
            .replace(&self.encoded_auth, "***")
            .replace(&self.token, "***");
        Err(git_error(operation, &output, detail.trim()))
    }

    fn clone_checkout(&self, directory: &Path) -> Result<(), String> {
        let output = self
            .command()
            .args(["clone", "--depth", "1", "--no-recurse-submodules", "--"])
            .arg(&self.url)
            .arg(directory)
            .output()
            .map_err(|error| error.to_string())?;
        self.check("clone", output)
    }

    fn push_checkout(&self, directory: &Path, branch: &str) -> Result<(), String> {
        let output = self
            .command()
            .args(["push", "--", "origin", branch])
            .current_dir(directory)
            .output()
            .map_err(|error| error.to_string())?;
        self.check("push", output)
    }
}

fn git() -> Command {
    background_command(crate::agents::executable::command_path("git"))
}

fn git_error(operation: &str, output: &Output, detail: &str) -> String {
    if detail.is_empty() {
        format!("Git {operation} failed ({})", output.status)
    } else {
        format!("Git {operation} failed ({})\n{detail}", output.status)
    }
}

fn run_local_git(directory: &Path, args: &[&str]) -> Result<Output, String> {
    git()
        .current_dir(directory)
        .args(args)
        .output()
        .map_err(|error| error.to_string())
}

fn require_git_success(operation: &str, output: Output) -> Result<Output, String> {
    if !output.status.success() {
        return Err(git_error(
            operation,
            &output,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(output)
}

fn commit_sync_snapshot(directory: &Path) -> Result<(), String> {
    require_git_success("add", run_local_git(directory, &["add", "--all"])?)?;
    let diff = run_local_git(directory, &["diff", "--cached", "--quiet"])?;
    match diff.status.code() {
        Some(0) => Ok(()),
        Some(1) => {
            require_git_success(
                "commit",
                run_local_git(directory, &["commit", "-m", "AgentBro sync"])?,
            )?;
            Ok(())
        }
        _ => Err(git_error(
            "diff",
            &diff,
            String::from_utf8_lossy(&diff.stderr).trim(),
        )),
    }
}

fn checked_out_branch(directory: &Path) -> Result<String, String> {
    let output = require_git_success(
        "symbolic-ref",
        run_local_git(directory, &["symbolic-ref", "--quiet", "--short", "HEAD"])?,
    )?;
    String::from_utf8(output.stdout)
        .map(|name| name.trim().to_string())
        .map_err(|error| error.to_string())
}

fn sync_failed(message: String) -> SyncResult {
    SyncResult {
        success: false,
        message,
        conflicts: Vec::new(),
    }
}

pub fn push_to_github() -> Result<SyncResult, String> {
    let meta = registry::read_metadata()?;
    let remote = GitHubSyncRemote::configured(&meta)?;
    let repo = meta.sync.as_ref().and_then(|sync| sync.github_repo.clone());
    let temporary = installer::TempInstallDir::create_in(&std::env::temp_dir())?;
    let checkout = temporary.path().join("repo");
    if let Err(error) = remote.clone_checkout(&checkout) {
        return Ok(sync_failed(format!("推送失败：{error}")));
    }

    fs::write(checkout.join("metadata.json"), backup_metadata_json(meta)?)
        .map_err(|error| error.to_string())?;
    let skills_src = agent_paths::agentbro_skills_dir();
    if skills_src.is_dir() {
        installer::copy_recursive_pub(&skills_src, &checkout.join("skills"))?;
    }

    if let Err(error) = commit_sync_snapshot(&checkout) {
        return Ok(sync_failed(format!("推送失败：{error}")));
    }
    let branch = checked_out_branch(&checkout)?;
    if let Err(error) = remote.push_checkout(&checkout, &branch) {
        return Ok(sync_failed(format!("推送失败：{error}")));
    }

    if let Ok(mut meta) = registry::edit() {
        if let Some(sync) = meta.sync.as_mut().filter(|sync| sync.github_repo == repo) {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = meta.save();
        }
    }
    Ok(SyncResult {
        success: true,
        message: "推送成功".to_string(),
        conflicts: vec![],
    })
}

pub fn pull_from_github() -> Result<SyncResult, String> {
    let meta = registry::read_metadata()?;
    let remote = GitHubSyncRemote::configured(&meta)?;
    let temporary = installer::TempInstallDir::create_in(&std::env::temp_dir())?;
    let tmp_dir = temporary.path().join("repo");
    if let Err(error) = remote.clone_checkout(&tmp_dir) {
        return Ok(sync_failed(format!("拉取失败：{error}")));
    }

    let remote_meta_path = tmp_dir.join("metadata.json");
    let mut conflicts = Vec::new();
    if remote_meta_path.exists() {
        let content = fs::read_to_string(&remote_meta_path).map_err(|e| e.to_string())?;
        let remote_meta: registry::Metadata =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let mut local_meta = registry::edit()?;
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
            local_meta.save()?;
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
    let mut local_meta = registry::edit()?;
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

    local_meta.save()?;
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
    let meta_json = backup_metadata_json(registry::read_metadata()?)?;

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

fn backup_metadata_json(mut metadata: registry::Metadata) -> Result<String, String> {
    metadata.sync = None;
    serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())
}

pub fn import_backup(path: &str) -> Result<(), String> {
    let file = fs::File::open(expand_user_path(path)).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let temporary = installer::TempInstallDir::create_in(&std::env::temp_dir())?;
    let extract_dir = temporary.path();
    archive.extract(extract_dir).map_err(|e| e.to_string())?;

    let meta_path = extract_dir.join("metadata.json");
    if meta_path.exists() {
        let content = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let imported: registry::Metadata =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let mut local = registry::edit()?;
        for (k, v) in imported.sources {
            local.sources.entry(k).or_insert(v);
        }
        for pack in imported.packs {
            if !local.packs.iter().any(|p| p.id == pack.id) {
                local.packs.push(pack);
            }
        }
        local.save()?;
    }

    let skills_dir = extract_dir.join("skills");
    if skills_dir.is_dir() {
        let dest = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        installer::copy_recursive_pub(&skills_dir, &dest)?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_auth_is_scoped_to_the_configured_https_repository() {
        let remote =
            GitHubSyncRemote::new("https://github.com/example/skills/", "fixture-credential")
                .unwrap();
        let command = remote.command();
        assert_eq!(remote.url, "https://github.com/example/skills.git");
        assert!(command.get_args().all(|argument| {
            let argument = argument.to_string_lossy();
            !argument.contains("fixture-credential") && !argument.contains(&remote.encoded_auth)
        }));
        let config = |url: &str| {
            remote
                .command()
                .args(["config", "--get-urlmatch", "http.extraHeader", url])
                .output()
                .unwrap()
        };
        let selected = config("https://github.com/example/skills.git/info/refs");
        assert!(selected.status.success());
        assert_eq!(
            String::from_utf8(selected.stdout).unwrap().trim(),
            format!("Authorization: Basic {}", remote.encoded_auth)
        );
        for url in [
            "https://github.com/example/another.git",
            "https://example.test/example/skills.git",
        ] {
            assert!(!String::from_utf8_lossy(&config(url).stdout).contains(&remote.encoded_auth));
        }
        let mut failure = run_local_git(Path::new("."), &["not-an-agentbro-git-command"]).unwrap();
        assert!(!failure.status.success());
        failure.stderr = format!(
            "permission denied: {} (raw {})",
            remote.encoded_auth, remote.token
        )
        .into_bytes();
        let message = remote.check("clone", failure).unwrap_err();
        assert!(message.contains("permission denied"));
        assert!(!message.contains(&remote.encoded_auth));
        assert!(!message.contains(&remote.token));
        assert!(GitHubSyncRemote::new("https://example.test/example/skills", "fixture").is_err());
    }

    #[test]
    fn commits_an_initial_snapshot_and_stops_on_a_failed_commit() {
        let temporary = installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path();
        let remote = root.join("source.git");
        let checkout = root.join("checkout");
        require_git_success(
            "init",
            git()
                .args(["init", "--bare", "--initial-branch=trunk"])
                .arg(&remote)
                .output()
                .unwrap(),
        )
        .unwrap();
        require_git_success(
            "clone",
            git()
                .args(["clone", "--"])
                .arg(&remote)
                .arg(&checkout)
                .output()
                .unwrap(),
        )
        .unwrap();
        for (key, value) in [
            ("user.name", "AgentBro test"),
            ("user.email", "agentbro-fixture@example.test"),
            ("commit.gpgsign", "false"),
        ] {
            require_git_success(
                "config",
                run_local_git(&checkout, &["config", key, value]).unwrap(),
            )
            .unwrap();
        }
        fs::write(checkout.join("metadata.json"), "{}").unwrap();
        commit_sync_snapshot(&checkout).unwrap();
        let branch = checked_out_branch(&checkout).unwrap();
        require_git_success(
            "push",
            run_local_git(&checkout, &["push", "--", "origin", &branch]).unwrap(),
        )
        .unwrap();
        let committed_head = run_local_git(&checkout, &["rev-parse", "HEAD"])
            .unwrap()
            .stdout;
        assert_eq!(
            run_local_git(&remote, &["rev-parse", &branch])
                .unwrap()
                .stdout,
            committed_head,
        );
        commit_sync_snapshot(&checkout).unwrap();
        require_git_success(
            "config",
            run_local_git(&checkout, &["config", "commit.gpgsign", "true"]).unwrap(),
        )
        .unwrap();
        require_git_success(
            "config",
            run_local_git(
                &checkout,
                &[
                    "config",
                    "gpg.program",
                    checkout.join("missing-gpg").to_str().unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        fs::write(checkout.join("metadata.json"), "{\"fixture\":true}").unwrap();
        assert!(commit_sync_snapshot(&checkout)
            .unwrap_err()
            .contains("Git commit failed"));
        assert_eq!(
            run_local_git(&checkout, &["rev-parse", "HEAD"])
                .unwrap()
                .stdout,
            committed_head
        );
    }

    #[test]
    fn backups_preserve_skill_metadata_without_local_sync_settings() {
        let metadata: registry::Metadata = serde_json::from_value(serde_json::json!({
            "sources": {
                "example-skill": {
                    "origin": "https://github.com/example/skills",
                    "installedVia": "island"
                }
            },
            "packs": [{
                "id": "example-pack",
                "name": "Example Skills",
                "description": "",
                "skills": ["example-skill"],
                "targetAgents": ["claude-code"]
            }],
            "sync": {
                "method": "github",
                "githubRepo": "example/sync-settings",
                "githubToken": "fixture-credential",
                "autoSync": false
            }
        }))
        .unwrap();

        let backup = backup_metadata_json(metadata.clone()).unwrap();
        assert!(!backup.contains("fixture-credential"));
        assert!(!backup.contains("example/sync-settings"));
        let backup: serde_json::Value = serde_json::from_str(&backup).unwrap();
        assert!(backup.get("sync").is_none());
        let imported: registry::Metadata = serde_json::from_value(backup).unwrap();
        assert_eq!(
            imported.sources["example-skill"].origin,
            "https://github.com/example/skills"
        );
        assert_eq!(imported.packs[0].skills, ["example-skill"]);
        assert_eq!(imported.packs[0].target_agents, ["claude-code"]);
        assert_eq!(
            serde_json::to_value(&metadata).unwrap()["sync"]["githubToken"],
            "fixture-credential"
        );
    }
}
