use std::fs;
use std::path::{Path, PathBuf};
use super::{InstallMode, TargetConfig};
use super::agent_paths;

pub fn install_skill(
    source_path: &str,
    targets: &[TargetConfig],
    mode: &InstallMode,
) -> Result<(), String> {
    let src = PathBuf::from(source_path);
    if !src.exists() {
        return Err(format!("Source not found: {}", source_path));
    }

    let skill_name = src.file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    let central_path = if matches!(mode, InstallMode::Symlink) {
        let central_dir = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;
        let dest = central_dir.join(&skill_name);
        copy_recursive(&src, &dest)?;
        Some(dest)
    } else {
        None
    };

    for target in targets {
        let paths = agent_paths::paths_for_agent(&target.agent);
        let target_dir = paths.skill_dirs.first()
            .ok_or_else(|| format!("No skill directory for agent: {}", target.agent))?;

        fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
        let dest = target_dir.join(&skill_name);

        match mode {
            InstallMode::Direct => {
                copy_recursive(&src, &dest)?;
            }
            InstallMode::Symlink => {
                if let Some(ref central) = central_path {
                    if dest.exists() || dest.symlink_metadata().is_ok() {
                        fs::remove_file(&dest).or_else(|_| fs::remove_dir_all(&dest))
                            .map_err(|e| e.to_string())?;
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(central, &dest)
                        .map_err(|e| e.to_string())?;
                    #[cfg(not(unix))]
                    copy_recursive(central, &dest)?;
                }
            }
        }
    }

    Ok(())
}

pub fn uninstall_skill(skill_path: &str) -> Result<(), String> {
    let path = PathBuf::from(skill_path);
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

pub fn toggle_skill(skill_id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let paths = agent_paths::paths_for_agent(agent);
    let settings_path = match paths.settings_file {
        Some(p) => p,
        None => return Err(format!("Agent {} has no settings file", agent)),
    };

    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| e.to_string())?;

    let disabled = settings
        .as_object_mut()
        .ok_or("Settings is not an object")?
        .entry("disabledSkills")
        .or_insert_with(|| serde_json::json!([]));

    if let Some(arr) = disabled.as_array_mut() {
        let skill_val = serde_json::Value::String(skill_id.to_string());
        if enabled {
            arr.retain(|v| v != &skill_val);
        } else if !arr.contains(&skill_val) {
            arr.push(skill_val);
        }
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, content).map_err(|e| e.to_string())
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }

    fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub fn copy_recursive_pub(src: &Path, dest: &Path) -> Result<(), String> {
    copy_recursive(src, dest)
}
