use super::RuntimePlatform;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(PartialEq, Eq)]
pub(super) struct NpmInstallation {
    pub prefix: PathBuf,
    pub launcher: PathBuf,
    pub version: String,
}

pub(super) fn npm_installation(launcher: &Path, package: &str) -> Option<NpmInstallation> {
    let directory = launcher.parent()?;
    let windows = RuntimePlatform::current() == RuntimePlatform::Windows;
    let prefix = if windows {
        if !launcher.extension()?.eq_ignore_ascii_case("cmd") {
            return None;
        }
        directory
    } else {
        if directory.file_name()? != "bin" {
            return None;
        }
        directory.parent()?
    };
    let relative_launcher = launcher.strip_prefix(prefix).ok()?;
    let prefix = fs::canonicalize(if prefix.as_os_str().is_empty() {
        Path::new(".")
    } else {
        prefix
    })
    .ok()?;
    #[cfg(windows)]
    let prefix = crate::platform::windows_shell::ordinary_path(&prefix)
        .filter(|path| path.canonicalize().is_ok_and(|physical| physical == prefix))
        .unwrap_or(prefix);
    let launcher = prefix.join(relative_launcher);
    let (modules, command) = if windows {
        (prefix.join("node_modules"), launcher.file_stem()?.to_str()?)
    } else {
        (
            prefix.join("lib/node_modules"),
            launcher.file_name()?.to_str()?,
        )
    };
    let package_dir = modules.join(package);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(package_dir.join("package.json")).ok()?).ok()?;
    if manifest["name"].as_str()? != package {
        return None;
    }
    let bin = &manifest["bin"];
    let command_matches = |name: &str| {
        if windows {
            name.eq_ignore_ascii_case(command)
        } else {
            name == command
        }
    };
    let entry = match bin {
        serde_json::Value::String(entry) if command_matches(package.rsplit('/').next()?) => {
            entry.as_str()
        }
        serde_json::Value::Object(entries) => entries
            .iter()
            .find_map(|(name, entry)| command_matches(name).then_some(entry))
            .and_then(serde_json::Value::as_str)?,
        _ => return None,
    };
    let entry = Path::new(entry);
    if !entry
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let target = package_dir.join(entry.components().collect::<PathBuf>());
    if !target.is_file() {
        return None;
    }
    let matches = if windows {
        let relative = target.strip_prefix(&prefix).ok()?;
        let script = fs::read_to_string(&launcher).ok()?;
        windows_shim_matches_entry(&script, relative)
    } else {
        launcher.canonicalize().ok()? == target.canonicalize().ok()?
    };
    if !matches {
        return None;
    }
    let version = manifest["version"].as_str()?.trim();
    if version.is_empty() {
        return None;
    }
    Some(NpmInstallation {
        prefix,
        launcher,
        version: version.to_string(),
    })
}

fn windows_shim_matches_entry(script: &str, relative: &Path) -> bool {
    let path = relative.components().collect::<PathBuf>();
    let Some(relative) = path.to_str() else {
        return false;
    };
    let relative = relative.replace('/', "\\").to_ascii_lowercase();
    let target = format!(r#""%dp0%\{relative}" %*"#);
    let legacy = format!(r#""%~dp0{relative}" %*"#);
    let legacy_with_separator = format!(r#""%~dp0\{relative}" %*"#);
    script.lines().any(|line| {
        let line = line.trim_end().to_ascii_lowercase();
        line.ends_with(&target) || line.ends_with(&legacy) || line.ends_with(&legacy_with_separator)
    })
}

#[cfg(test)]
mod tests {
    use super::{npm_installation, windows_shim_matches_entry};
    use crate::skills::installer::TempInstallDir;
    use std::{fs, path::Path};

    const PACKAGE: &str = "@anthropic-ai/claude-code";

    #[test]
    fn windows_npm_shim_matches_scoped_entry_with_mixed_separators() {
        let script = r#""%_prog%" "%dp0%\node_modules\@anthropic-ai\claude-code\bin\cli.js" %*"#;
        assert!(windows_shim_matches_entry(
            script,
            Path::new(r"node_modules\@anthropic-ai/claude-code\bin/cli.js")
        ));
        assert!(windows_shim_matches_entry(
            script,
            Path::new("node_modules/@anthropic-ai/claude-code/./bin/cli.js")
        ));
        assert!(!windows_shim_matches_entry(
            script,
            Path::new(r"node_modules\@anthropic-ai/a-different-package\bin/cli.js")
        ));
    }

    fn install(prefix: &Path, version: &str) -> std::path::PathBuf {
        #[cfg(windows)]
        let package = prefix.join("node_modules").join(PACKAGE);
        #[cfg(unix)]
        let package = prefix.join("lib/node_modules").join(PACKAGE);
        fs::create_dir_all(package.join("bin")).unwrap();
        let manifest = serde_json::json!({
            "name": PACKAGE, "version": version, "bin": {"claude": "./bin/cli.js"}
        });
        fs::write(package.join("package.json"), manifest.to_string()).unwrap();
        fs::write(package.join("bin/cli.js"), "").unwrap();

        #[cfg(windows)]
        {
            let launcher = prefix.join("Claude.CMD");
            fs::write(
                &launcher,
                r#""%_prog%" "%dp0%\node_modules\@anthropic-ai\claude-code\bin\cli.js" %*"#,
            )
            .unwrap();
            launcher
        }
        #[cfg(unix)]
        {
            let launcher = prefix.join("bin/claude");
            fs::create_dir_all(launcher.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(package.join("bin/cli.js"), &launcher).unwrap();
            launcher
        }
    }

    #[test]
    fn global_npm_installation_matches_the_detected_launchers_manifest() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let first = fixture.path().join("selected 安装");
        let second = fixture.path().join("different prefix");
        let selected = install(&first, "1.0.0");
        let other = install(&second, "2.0.0");
        let active = npm_installation(&selected, PACKAGE).unwrap();
        assert_eq!(
            active.prefix.canonicalize().unwrap(),
            first.canonicalize().unwrap()
        );
        let bin = if cfg!(windows) {
            active.prefix.clone()
        } else {
            active.prefix.join("bin")
        };
        assert_eq!(active.launcher.parent(), Some(bin.as_path()));
        assert_eq!(active.version, "1.0.0");
        assert_eq!(npm_installation(&other, PACKAGE).unwrap().version, "2.0.0");

        fs::remove_file(&selected).unwrap();
        fs::write(&selected, "echo separate installation").unwrap();
        assert!(npm_installation(&selected, PACKAGE).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn changing_a_toolchain_symlink_changes_the_detected_npm_prefix() {
        use std::os::unix::fs::symlink;

        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        install(&first, "2.0.0");
        install(&second, "2.0.0");
        let alias = fixture.path().join("active");
        let launcher = alias.join("bin/claude");

        symlink(&first, &alias).unwrap();
        let reviewed = npm_installation(&launcher, PACKAGE).unwrap();
        let meta = super::super::metadata_for("claude-code").unwrap();
        let expected = reviewed.launcher.to_str();
        assert!(meta
            .reviewed_npm_installation(Some(&launcher), expected)
            .is_ok());
        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();
        let current = npm_installation(&launcher, PACKAGE).unwrap();
        assert_eq!(reviewed.version, current.version);
        assert_ne!(reviewed.prefix, current.prefix);
        assert_eq!(reviewed.prefix, first.canonicalize().unwrap());
        assert_eq!(current.prefix, second.canonicalize().unwrap());
        assert!(meta
            .reviewed_npm_installation(Some(&launcher), expected)
            .is_err());
    }
}
