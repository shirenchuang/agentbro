use std::fs;
use std::path::PathBuf;
use std::process::Command;
use super::{SyncResult, SyncPreview, TargetConfig, InstallMode};
use super::{registry, scanner, installer, agent_paths};

pub fn push_to_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta.sync.as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta.sync.as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync");
    let _ = fs::remove_dir_all(&tmp_dir);

    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone_result = Command::new("git")
        .args(["clone", "--depth", "1", &repo_url, &tmp_dir.display().to_string()])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone_result.status.success() {
        fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        Command::new("git").args(["init"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
        Command::new("git").args(["remote", "add", "origin", &repo_url]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    }

    let meta_content = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(tmp_dir.join("metadata.json"), meta_content).map_err(|e| e.to_string())?;

    let skills_src = agent_paths::agentbro_skills_dir();
    if skills_src.is_dir() {
        let skills_dest = tmp_dir.join("skills");
        installer::copy_recursive_pub(&skills_src, &skills_dest)?;
    }

    Command::new("git").args(["add", "."]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    Command::new("git").args(["commit", "-m", "AgentBro sync"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    let push = Command::new("git").args(["push", "-u", "origin", "main"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;

    let _ = fs::remove_dir_all(&tmp_dir);

    if push.status.success() {
        let mut meta = registry::load();
        if let Some(ref mut sync) = meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = registry::save(&meta);

        Ok(SyncResult { success: true, message: "推送成功".to_string(), conflicts: vec![] })
    } else {
        let stderr = String::from_utf8_lossy(&push.stderr).to_string();
        Ok(SyncResult { success: false, message: format!("推送失败: {}", stderr), conflicts: vec![] })
    }
}

pub fn pull_from_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta.sync.as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta.sync.as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync-pull");
    let _ = fs::remove_dir_all(&tmp_dir);

    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone = Command::new("git")
        .args(["clone", "--depth", "1", &repo_url, &tmp_dir.display().to_string()])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone.status.success() {
        return Ok(SyncResult { success: false, message: "拉取失败：无法克隆仓库".to_string(), conflicts: vec![] });
    }

    let remote_meta_path = tmp_dir.join("metadata.json");
    if remote_meta_path.exists() {
        let content = fs::read_to_string(&remote_meta_path).map_err(|e| e.to_string())?;
        let remote_meta: registry::Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let mut local_meta = registry::load();
        for pack in remote_meta.packs {
            if !local_meta.packs.iter().any(|p| p.id == pack.id) {
                local_meta.packs.push(pack);
            }
        }
        for (k, v) in remote_meta.sources {
            local_meta.sources.entry(k).or_insert(v);
        }
        if let Some(ref mut sync) = local_meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        registry::save(&local_meta)?;
    }

    let remote_skills = tmp_dir.join("skills");
    if remote_skills.is_dir() {
        let local_skills = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&local_skills).map_err(|e| e.to_string())?;
        installer::copy_recursive_pub(&remote_skills, &local_skills)?;
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    Ok(SyncResult { success: true, message: "拉取成功".to_string(), conflicts: vec![] })
}

pub fn sync_agent_to_agent(from: &str, to: &str) -> Result<SyncPreview, String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> = to_skills.iter().map(|s| s.id.clone()).collect();

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

    Ok(SyncPreview { to_copy, to_skip, to_update: 0, details })
}

pub fn execute_agent_sync(from: &str, to: &str) -> Result<(), String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> = to_skills.iter().map(|s| s.id.clone()).collect();

    let targets = vec![TargetConfig {
        agent: to.to_string(),
        install_mode: InstallMode::Direct,
    }];

    for skill in &from_skills {
        if !to_ids.contains(&skill.id) {
            installer::install_skill(&skill.file_path, &targets, &InstallMode::Direct)?;
        }
    }

    Ok(())
}

pub fn export_backup(path: &str) -> Result<(), String> {
    let dest = PathBuf::from(path);
    let meta = registry::load();
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;

    let file = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    zip.start_file("metadata.json", options).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut zip, meta_json.as_bytes()).map_err(|e| e.to_string())?;

    let skills_dir = agent_paths::agentbro_skills_dir();
    if skills_dir.is_dir() {
        add_dir_to_zip(&mut zip, &skills_dir, "skills", options)?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn import_backup(path: &str) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let extract_dir = std::env::temp_dir().join("agentbro-import");
    let _ = fs::remove_dir_all(&extract_dir);
    archive.extract(&extract_dir).map_err(|e| e.to_string())?;

    let meta_path = extract_dir.join("metadata.json");
    if meta_path.exists() {
        let content = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let imported: registry::Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;
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
