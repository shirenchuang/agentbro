use super::{package_command, package_probe_output, ProgramUninstallTarget};
use crate::agents::executable;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PACKAGE: &str = "aider-chat";
const PIP_PROBE: &str = include_str!("aider_pip_probe.py");
const PIP_UNINSTALL: &[&str] = &["-E", "-m", "pip", "--isolated", "uninstall", "-y", PACKAGE];

#[derive(serde::Serialize)]
enum Manager {
    Uv { tools: PathBuf, bin: PathBuf },
    Pipx { home: PathBuf, bin: PathBuf },
    Pip,
}

pub(super) struct AiderInstallation {
    manager: Manager,
    program: PathBuf,
    version: String,
    target: ProgramUninstallTarget,
}

impl AiderInstallation {
    fn new(
        manager: Manager,
        program: PathBuf,
        owner: PathBuf,
        version: String,
        launcher: &Path,
        fingerprint: &str,
    ) -> Option<Self> {
        let details =
            serde_json::to_vec(&(&manager, &program, &owner, &version, fingerprint)).ok()?;
        let identity = format!("{:x}", Sha256::digest(details));
        Some(Self {
            manager,
            program,
            version,
            target: ProgramUninstallTarget {
                path: launcher.display().to_string(),
                identity,
            },
        })
    }

    pub(super) fn manager_name(&self) -> &'static str {
        match self.manager {
            Manager::Uv { .. } => "uv",
            Manager::Pipx { .. } => "pipx",
            Manager::Pip => "pip",
        }
    }

    pub(super) fn version(&self) -> &str {
        &self.version
    }

    pub(super) fn target(&self) -> &ProgramUninstallTarget {
        &self.target
    }

    pub(super) fn uninstall_display(&self) -> String {
        let program = match self.manager {
            Manager::Uv { .. } => "uv tool",
            Manager::Pipx { .. } => "pipx",
            Manager::Pip => {
                return format!("\"{}\" {}", self.program.display(), PIP_UNINSTALL.join(" "))
            }
        };
        format!("{program} uninstall {PACKAGE}")
    }

    pub(super) fn uninstall_command(&self) -> Command {
        let path = executable::augmented_path_env();
        let mut command = package_command(&self.program, path.as_deref());
        match &self.manager {
            Manager::Uv { tools, bin } => {
                command
                    .env("UV_TOOL_DIR", tools)
                    .env("UV_TOOL_BIN_DIR", bin)
                    .args(["tool", "uninstall", PACKAGE]);
            }
            Manager::Pipx { home, bin } => {
                command
                    .env("PIPX_HOME", home)
                    .env("PIPX_BIN_DIR", bin)
                    .args(["uninstall", PACKAGE]);
            }
            Manager::Pip => {
                command
                    .env("PYTHON_MANAGER_AUTOMATIC_INSTALL", "false")
                    .args(PIP_UNINSTALL);
            }
        }
        command
    }
}

pub(super) async fn find(launcher: &Path) -> Option<AiderInstallation> {
    discover(launcher, cfg!(windows), executable::find_binary).await
}

pub(super) fn reviewed(
    detected: Option<AiderInstallation>,
    expected: Option<&ProgramUninstallTarget>,
) -> Result<AiderInstallation, String> {
    detected
        .filter(|installation| Some(installation.target()) == expected)
        .ok_or_else(|| "The reviewed Aider installation changed or could not be verified. Refresh Agent Management and review it again.".to_string())
}

async fn discover(
    launcher: &Path,
    windows: bool,
    find_binary: impl Fn(&str) -> Option<PathBuf>,
) -> Option<AiderInstallation> {
    let canonical = launcher.canonicalize().ok()?;
    let fingerprint = file_fingerprint(&canonical)?;
    let matches = |entry: &Path, exposed: &Path| {
        matches_launcher(entry, exposed, &canonical, &fingerprint, windows)
    };
    let path = executable::augmented_path_env();
    let (uv, pipx) = tokio::join!(
        async {
            let program = find_binary("uv")?;
            let listed = run_probe(package_command(&program, path.as_deref()).args([
                "tool",
                "list",
                "--show-paths",
                "--color",
                "never",
            ]))
            .await?;
            let tool = uv_tool(&listed, windows, &matches)?;
            let tools = tool.root.parent()?.to_path_buf();
            AiderInstallation::new(
                Manager::Uv {
                    tools,
                    bin: tool.bin,
                },
                program,
                tool.root,
                tool.version,
                launcher,
                &fingerprint,
            )
        },
        async {
            let program = find_binary("pipx")?;
            let pipx = || {
                let mut command = package_command(&program, path.as_deref());
                command.env("PYTHONIOENCODING", "utf-8");
                command
            };
            let listed = run_probe(pipx().args(["list", "--json"])).await?;
            let bin = run_probe(pipx().args(["environment", "--value", "PIPX_BIN_DIR"])).await?;
            let bin = PathBuf::from(bin.trim());
            let tool = pipx_tool(&listed, &bin, &matches)?;
            let home = tool.root.parent()?.parent()?.to_path_buf();
            AiderInstallation::new(
                Manager::Pipx {
                    home,
                    bin: tool.bin,
                },
                program,
                tool.root,
                tool.version,
                launcher,
                &fingerprint,
            )
        },
    );
    let installation = match (uv, pipx) {
        (Some(_), Some(_)) => return None,
        (Some(tool), None) | (None, Some(tool)) => Some(tool),
        (None, None) => {
            let python_launchers: &[&str] = if windows {
                &["python", "py", "python3"]
            } else {
                &["python3"]
            };
            let mut found = None;
            for name in python_launchers {
                let Some(program) = find_binary(name) else {
                    continue;
                };
                found = pip_tool(&program, path.as_deref(), launcher, &fingerprint).await;
                if found.is_some() {
                    break;
                }
            }
            found
        }
    }?;
    (launcher.canonicalize().ok().as_ref() == Some(&canonical)
        && file_fingerprint(&canonical).as_ref() == Some(&fingerprint))
    .then_some(installation)
}

async fn run_probe(command: &mut Command) -> Option<String> {
    let output = package_probe_output(command, PROBE_TIMEOUT).await.ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn matches_launcher(
    entry: &Path,
    exposed: &Path,
    launcher: &Path,
    fingerprint: &str,
    windows: bool,
) -> bool {
    let Ok(source) = entry.canonicalize() else {
        return false;
    };
    match exposed.canonicalize() {
        Ok(target) if target == source => launcher == source,
        // Windows tool installers can expose a copy instead of a symlink.
        Ok(target)
            if windows
                && (target == launcher || source == launcher)
                && fs::symlink_metadata(exposed).is_ok_and(|metadata| metadata.is_file()) =>
        {
            let other = if source == launcher { &target } else { &source };
            file_fingerprint(other).as_deref() == Some(fingerprint)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            source == launcher
                && matches!(fs::symlink_metadata(exposed), Err(error) if error.kind() == io::ErrorKind::NotFound)
        }
        _ => false,
    }
}

fn file_fingerprint(path: &Path) -> Option<String> {
    if !fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut digest = Sha256::new();
    io::copy(&mut file, &mut digest).ok()?;
    Some(format!("{:x}", digest.finalize()))
}

struct Tool {
    root: PathBuf,
    bin: PathBuf,
    version: String,
}

fn uv_tool(listed: &str, windows: bool, matches: &impl Fn(&Path, &Path) -> bool) -> Option<Tool> {
    let mut lines = listed
        .lines()
        .skip_while(|line| !line.starts_with("aider-chat v"));
    let (version, root) = lines
        .next()?
        .strip_prefix("aider-chat v")?
        .split_once(" (")?;
    let root = Path::new(root.strip_suffix(')')?);
    if !root.is_absolute() {
        return None;
    }
    let bin = root.join(if windows { "Scripts" } else { "bin" });
    for line in lines.take_while(|line| line.starts_with("- ")) {
        let Some(exposed) = line
            .strip_prefix("- aider (")
            .or_else(|| line.strip_prefix("- aider.exe ("))
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let exposed = Path::new(exposed);
        if let Some(file) = exposed
            .file_name()
            .filter(|file| *file == if windows { "aider.exe" } else { "aider" })
        {
            if exposed.is_absolute() && matches(&bin.join(file), exposed) {
                return Some(Tool {
                    root: root.to_path_buf(),
                    bin: exposed.parent()?.to_path_buf(),
                    version: version.to_string(),
                });
            }
        }
    }
    None
}

fn pipx_tool(listed: &str, bin: &Path, matches: &impl Fn(&Path, &Path) -> bool) -> Option<Tool> {
    if !bin.is_absolute() {
        return None;
    }
    let list: serde_json::Value = serde_json::from_str(listed).ok()?;
    let metadata = &list["venvs"][PACKAGE]["metadata"];
    if metadata["exposure_enabled"].as_bool() == Some(false) {
        return None;
    }
    let package = &metadata["main_package"];
    let version = package["package_version"].as_str()?;
    for entry in package["app_paths"].as_array()? {
        let path = entry.as_str().or_else(|| entry["__Path__"].as_str());
        let Some(source) = path.map(Path::new).filter(|path| path.is_absolute()) else {
            continue;
        };
        if let Some(file) = source
            .file_name()
            .filter(|file| *file == "aider" || *file == "aider.exe")
        {
            if matches(source, &bin.join(file)) {
                return Some(Tool {
                    root: source.parent()?.parent()?.to_path_buf(),
                    bin: bin.to_path_buf(),
                    version: version.to_string(),
                });
            }
        }
    }
    None
}

#[derive(Deserialize)]
struct PipInstallation {
    python: PathBuf,
    site: PathBuf,
    version: String,
}

async fn pip_tool(
    program: &Path,
    path: Option<&OsStr>,
    launcher: &Path,
    fingerprint: &str,
) -> Option<AiderInstallation> {
    let found = run_probe(
        package_command(program, path)
            .env("PYTHON_MANAGER_AUTOMATIC_INSTALL", "false")
            .env("AGENTBRO_AIDER_PIP_PROBE", PIP_PROBE)
            .args([
                "-E",
                "-c",
                "import os;exec(os.environ['AGENTBRO_AIDER_PIP_PROBE'])",
            ])
            .arg(launcher),
    )
    .await?;
    let installation = found.lines().find_map(|line| {
        serde_json::from_str::<PipInstallation>(line.strip_prefix("agentbro-aider ")?).ok()
    })?;
    if !installation.python.is_absolute() || !installation.site.is_absolute() {
        return None;
    }
    AiderInstallation::new(
        Manager::Pip,
        installation.python,
        installation.site,
        installation.version,
        launcher,
        fingerprint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn shared_exposure_belongs_only_to_the_launcher_it_exposes() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = fixture.path().join("Space 路径");
        let windows = cfg!(windows);
        let name = if windows { "aider.exe" } else { "aider" };
        let scripts = if windows { "Scripts" } else { "bin" };
        let uv_root = root.join("uv tools").join(PACKAGE);
        let pipx_root = root.join("pipx home").join("venvs").join(PACKAGE);
        let shared_bin = root.join("shared bin");
        let uv_source = uv_root.join(scripts).join(name);
        let pipx_source = pipx_root.join(scripts).join(name);
        let launcher = shared_bin.join(name);
        for (source, contents) in [
            (&uv_source, b"uv".as_slice()),
            (&pipx_source, b"pipx".as_slice()),
        ] {
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(source, contents).unwrap();
        }
        fs::create_dir_all(&shared_bin).unwrap();
        let uv_list = format!(
            "aider-chat v1.0.0 ({})\n- {} ({})\n",
            uv_root.display(),
            name,
            launcher.display()
        );
        let pipx_list = serde_json::json!({
            "venvs": {"aider-chat": {"metadata": {"main_package": {
                "app_paths": [{"__Path__": pipx_source}],
                "package_version": "2.0.0"
            }}}}
        })
        .to_string();
        for source in [&uv_source, &pipx_source] {
            #[cfg(windows)]
            fs::copy(source, &launcher).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(source, &launcher).unwrap();
            let canonical = launcher.canonicalize().unwrap();
            let fingerprint = file_fingerprint(&canonical).unwrap();
            let matches = |entry: &Path, exposed: &Path| {
                matches_launcher(entry, exposed, &canonical, &fingerprint, windows)
            };
            assert_eq!(
                uv_tool(&uv_list, windows, &matches).is_some(),
                source == &uv_source
            );
            assert_eq!(
                pipx_tool(&pipx_list, &shared_bin, &matches).is_some(),
                source == &pipx_source
            );
            let other = if source == &uv_source {
                &pipx_source
            } else {
                &uv_source
            };
            assert!(!matches_launcher(
                other,
                &launcher,
                &other.canonicalize().unwrap(),
                &file_fingerprint(other).unwrap(),
                windows
            ));
            fs::remove_file(&launcher).unwrap();
        }
    }

    #[test]
    fn reviewed_installation_rejects_a_new_manager_or_launcher() {
        let make = |manager, fingerprint| {
            AiderInstallation::new(
                manager,
                PathBuf::from("tool"),
                PathBuf::from("owner"),
                "1.0.0".into(),
                Path::new("aider"),
                fingerprint,
            )
            .unwrap()
        };
        let original = make(Manager::Pip, "before");
        let expected = original.target().clone();
        assert!(reviewed(Some(original), Some(&expected)).is_ok());
        assert!(reviewed(None, Some(&expected)).is_err());
        assert!(reviewed(Some(make(Manager::Pip, "before")), None).is_err());
        assert!(reviewed(Some(make(Manager::Pip, "after")), Some(&expected)).is_err());
        let uv = Manager::Uv {
            tools: "tools".into(),
            bin: "bin".into(),
        };
        assert!(reviewed(Some(make(uv, "before")), Some(&expected)).is_err());
    }

    #[tokio::test]
    async fn windows_aider_uninstall_reuses_the_detected_python_launcher() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let directory = fixture.path().join("Space 路径");
        fs::create_dir_all(&directory).unwrap();
        let aider = directory.join("aider.exe");
        fs::write(&aider, b"Aider launcher").unwrap();
        #[cfg(windows)]
        let (shim, python) = (
            directory.join("py shim.cmd"),
            directory.join("python runtime.cmd"),
        );
        #[cfg(unix)]
        let (shim, python) = (directory.join("py shim"), directory.join("python runtime"));
        fs::write(
            directory.join("probe.json"),
            format!(
                "agentbro-aider {}",
                serde_json::json!({
                    "python": python, "site": directory, "version": "3.0.0"
                })
            ),
        )
        .unwrap();

        #[cfg(windows)]
        let (probe, uninstall) = (
            "@echo off\r\nif not \"%PYTHON_MANAGER_AUTOMATIC_INSTALL%\"==\"false\" exit /b 1\r\nif not \"%1 %2\"==\"-E -c\" exit /b 1\r\ntype \"%~dp0probe.json\"\r\n",
            "@echo off\r\nif not \"%PYTHON_MANAGER_AUTOMATIC_INSTALL%\"==\"false\" exit /b 1\r\nif not \"%*\"==\"-E -m pip --isolated uninstall -y aider-chat\" exit /b 1\r\necho removed aider-chat\r\n",
        );
        #[cfg(unix)]
        let (probe, uninstall) = (
            "#!/bin/sh\n[ \"$PYTHON_MANAGER_AUTOMATIC_INSTALL\" = false ] && [ \"$1 $2\" = '-E -c' ] || exit 1\ncat \"$(dirname \"$0\")/probe.json\"\n",
            "#!/bin/sh\n[ \"$PYTHON_MANAGER_AUTOMATIC_INSTALL\" = false ] && [ \"$*\" = '-E -m pip --isolated uninstall -y aider-chat' ] || exit 1\necho removed aider-chat\n",
        );
        fs::write(&shim, probe).unwrap();
        fs::write(&python, uninstall).unwrap();
        #[cfg(unix)]
        for path in [&shim, &python] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        for launcher_name in ["python", "py", "python3"] {
            let installation = discover(&aider, true, |name| {
                (name == launcher_name).then(|| shim.clone())
            })
            .await
            .unwrap();
            assert_eq!(installation.manager_name(), "pip");
            assert_eq!(installation.version(), "3.0.0");
            assert!(installation
                .uninstall_display()
                .contains(&*python.to_string_lossy()));
            let mut command = installation.uninstall_command();
            assert_eq!(command.as_std().get_program(), python.as_os_str());
            let result = command.output().await.unwrap();
            assert!(result.status.success());
            assert_eq!(
                String::from_utf8_lossy(&result.stdout).trim(),
                "removed aider-chat"
            );
        }
    }
}
