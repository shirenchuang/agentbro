use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn command_exists(binary: &str) -> bool {
    find_binary(binary).is_some()
}

pub fn command_path(binary: &str) -> PathBuf {
    find_binary(binary).unwrap_or_else(|| PathBuf::from(binary))
}

pub fn augmented_path_env() -> Option<OsString> {
    std::env::join_paths(candidate_dirs()).ok()
}

pub fn find_binary(binary: &str) -> Option<PathBuf> {
    if binary.eq_ignore_ascii_case("codex") {
        return find_codex_cli_binary();
    }
    find_binary_generic(binary)
}

pub fn find_codex_cli_binary() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        find_windows_codex_cli_binary()
    }

    #[cfg(not(target_os = "windows"))]
    {
        find_binary_generic("codex")
    }
}

#[cfg(target_os = "windows")]
pub fn codex_desktop_app_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(local_app_data) = windows_local_app_data_dir() {
        candidates.push(
            local_app_data
                .join("OpenAI")
                .join("Codex")
                .join("app")
                .join("Codex.exe"),
        );
        candidates.push(
            local_app_data
                .join("Programs")
                .join("Codex")
                .join("Codex.exe"),
        );
    }

    for program_files in ["ProgramFiles", "ProgramW6432"] {
        if let Some(root) = std::env::var_os(program_files).map(PathBuf::from) {
            candidates.push(root.join("OpenAI").join("Codex").join("Codex.exe"));
            candidates.push(root.join("Codex").join("Codex.exe"));
        }
    }

    dedupe_paths(candidates)
}

#[cfg(target_os = "windows")]
pub fn codex_desktop_app_user_model_ids() -> Vec<String> {
    let mut candidates = vec!["OpenAI.Codex_2p2nqsd0c76g0!App".to_string()];

    for program_files in ["ProgramFiles", "ProgramW6432"] {
        if let Some(root) = std::env::var_os(program_files).map(PathBuf::from) {
            candidates.extend(windows_codex_app_user_model_ids(&root.join("WindowsApps")));
        }
    }

    dedupe_strings(candidates)
}

#[cfg(not(target_os = "windows"))]
pub fn codex_desktop_app_user_model_ids() -> Vec<String> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn codex_desktop_app_candidates() -> Vec<PathBuf> {
    Vec::new()
}

fn find_binary_generic(binary: &str) -> Option<PathBuf> {
    binary_candidates(binary)
        .into_iter()
        .find_map(|candidate| find_binary_exact(&candidate))
}

fn find_binary_exact(binary: &str) -> Option<PathBuf> {
    let path = Path::new(binary);
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if let Some(path) = which(binary) {
        return Some(path);
    }
    candidate_dirs()
        .into_iter()
        .map(|dir| dir.join(binary))
        .find(|path| path.is_file())
}

fn binary_candidates(binary: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        windows_binary_candidates(binary)
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![binary.to_string()]
    }
}

#[cfg(target_os = "windows")]
fn windows_binary_candidates(binary: &str) -> Vec<String> {
    let path = Path::new(binary);
    if path.extension().is_some()
        || binary.contains(std::path::MAIN_SEPARATOR)
        || binary.contains('/')
        || binary.contains('\\')
    {
        return vec![binary.to_string()];
    }

    let lower = binary.to_ascii_lowercase();
    let mut candidates = match lower.as_str() {
        "node" => vec![format!("{binary}.exe"), format!("{binary}.cmd")],
        "npm" | "npx" => vec![
            format!("{binary}.cmd"),
            format!("{binary}.exe"),
            format!("{binary}.bat"),
        ],
        "cmd" | "powershell" | "pwsh" => {
            vec![format!("{binary}.exe"), format!("{binary}.cmd")]
        }
        _ => vec![
            format!("{binary}.exe"),
            format!("{binary}.cmd"),
            format!("{binary}.bat"),
        ],
    };
    candidates.push(binary.to_string());
    candidates
}

#[cfg(target_os = "windows")]
fn find_windows_codex_cli_binary() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend(windows_codex_bundled_cli_candidates());

    for dir in candidate_dirs()
        .into_iter()
        .filter(|dir| !is_windows_app_execution_path(dir))
    {
        for binary in windows_binary_candidates("codex") {
            candidates.push(dir.join(binary));
        }
    }

    for binary in windows_binary_candidates("codex") {
        candidates.extend(which_all(&binary));
    }

    dedupe_paths(candidates)
        .into_iter()
        .filter(|path| !is_windows_app_execution_path(path))
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn windows_codex_bundled_cli_candidates() -> Vec<PathBuf> {
    let Some(local_app_data) = windows_local_app_data_dir() else {
        return Vec::new();
    };
    let codex_bin = local_app_data.join("OpenAI").join("Codex").join("bin");
    let mut candidates = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&codex_bin) {
        let mut version_dirs = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .collect::<Vec<_>>();
        version_dirs.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, dir) in version_dirs {
            candidates.push(dir.join("codex.exe"));
            candidates.push(dir.join("codex.cmd"));
        }
    }

    candidates.push(codex_bin.join("codex.exe"));
    candidates.push(codex_bin.join("codex.cmd"));
    candidates
}

#[cfg(target_os = "windows")]
fn windows_codex_app_user_model_ids(windows_apps_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(windows_apps_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(ToString::to_string))
        .filter(|name| name.to_ascii_lowercase().starts_with("openai.codex_"))
        .filter_map(|name| {
            let publisher_id = name.rsplit_once("__")?.1;
            Some(format!("OpenAI.Codex_{publisher_id}!App"))
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_local_app_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join("AppData").join("Local")))
}

#[cfg(target_os = "windows")]
fn is_windows_app_execution_path(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized.contains("\\microsoft\\windowsapps\\") || normalized.contains("\\windowsapps\\")
}

fn which(binary: &str) -> Option<PathBuf> {
    which_all(binary).into_iter().next()
}

fn which_all(binary: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    let command = "where.exe";
    #[cfg(not(target_os = "windows"))]
    let command = "which";

    std::process::Command::new(command)
        .arg(binary)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| {
            output
                .lines()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .collect()
        })
        .unwrap_or_default()
}

fn user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string())
}

fn is_fish(shell: &str) -> bool {
    shell.ends_with("/fish")
}

fn shell_var_fallback_cmd(var: &str) -> String {
    let shell = user_shell();
    if is_fish(&shell) {
        format!("printf '%s' \"${}\"", var)
    } else {
        format!("printf '%s' \"${{{}:-}}\"", var)
    }
}

pub fn login_shell_var(var: &str) -> Option<String> {
    shell_var(var, false)
}

pub fn interactive_login_shell_vars(vars: &[&str]) -> Vec<(String, String)> {
    if vars.is_empty() {
        return Vec::new();
    }
    let shell = user_shell();
    let output = match std::process::Command::new(&shell)
        .args(["-lic", "env"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let output = String::from_utf8_lossy(&output.stdout);
    vars.iter()
        .filter_map(|name| {
            output.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key == *name && !value.trim().is_empty())
                    .then(|| ((*name).to_string(), value.to_string()))
            })
        })
        .collect()
}

fn shell_var(var: &str, interactive: bool) -> Option<String> {
    let shell = user_shell();
    let cmd = shell_var_fallback_cmd(var);
    let shell_mode = if interactive { "-lic" } else { "-lc" };
    let output = std::process::Command::new(&shell)
        .args([shell_mode, &cmd])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

fn login_shell_path() -> Option<Vec<PathBuf>> {
    let shell = user_shell();
    let cmd = if is_fish(&shell) {
        "printf '%s' (string join ':' $PATH)"
    } else {
        "printf '%s' \"$PATH\""
    };
    let output = std::process::Command::new(shell)
        .args(["-lc", cmd])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path_str.is_empty() {
        return None;
    }
    Some(std::env::split_paths(&path_str).collect())
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();

    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }

    if let Some(shell_paths) = login_shell_path() {
        dirs.extend(shell_paths);
    }

    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ]);

    if let Some(home) = dirs::home_dir() {
        dirs.extend([
            home.join(".local").join("bin"),
            home.join(".npm-global").join("bin"),
            home.join(".bun").join("bin"),
            home.join(".cargo").join("bin"),
            home.join(".yarn").join("bin"),
            home.join(".volta").join("bin"),
        ]);
        #[cfg(target_os = "windows")]
        dirs.extend([
            home.join("AppData").join("Roaming").join("npm"),
            home.join("AppData").join("Local").join("pnpm"),
            home.join("scoop").join("shims"),
            home.join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("WindowsApps"),
        ]);
        dirs.extend(nvm_node_bins(&home));
        dirs.extend(volta_bin(&home));
        dirs.extend(mise_shims(&home));
        dirs.extend(vfox_node_bins(&home));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = windows_local_app_data_dir() {
            dirs.insert(local_app_data.join("pnpm"));
            dirs.insert(local_app_data.join("OpenAI").join("Codex").join("bin"));
        }
        if let Ok(app_data) = std::env::var("APPDATA") {
            dirs.insert(PathBuf::from(app_data).join("npm"));
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            let root = PathBuf::from(program_files);
            dirs.insert(root.join("nodejs"));
            dirs.insert(root.join("Git").join("cmd"));
            dirs.insert(root.join("Git").join("bin"));
        }
        if let Ok(program_w6432) = std::env::var("ProgramW6432") {
            let root = PathBuf::from(program_w6432);
            dirs.insert(root.join("Git").join("cmd"));
            dirs.insert(root.join("Git").join("bin"));
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            let root = PathBuf::from(program_files_x86);
            dirs.insert(root.join("nodejs"));
            dirs.insert(root.join("Git").join("cmd"));
            dirs.insert(root.join("Git").join("bin"));
        }
        if let Ok(system_root) = std::env::var("SystemRoot") {
            let root = PathBuf::from(system_root);
            dirs.insert(root.clone());
            dirs.insert(root.join("System32"));
            dirs.insert(root.join("System32").join("OpenSSH"));
            dirs.insert(root.join("System32").join("WindowsPowerShell").join("v1.0"));
        }
    }

    dirs.into_iter().collect()
}

fn nvm_node_bins(home: &Path) -> Vec<PathBuf> {
    let versions = home.join(".nvm").join("versions").join("node");
    let Ok(entries) = std::fs::read_dir(versions) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("bin"))
        .filter(|path| path.is_dir())
        .collect()
}

fn volta_bin(home: &Path) -> Option<PathBuf> {
    let path = home.join(".volta").join("bin");
    path.is_dir().then_some(path)
}

fn mise_shims(home: &Path) -> Option<PathBuf> {
    let path = home.join(".local").join("share").join("mise").join("shims");
    path.is_dir().then_some(path)
}

fn vfox_node_bin_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".vfox").join("sdks").join("nodejs").join("bin"),
        home.join(".version-fox")
            .join("sdks")
            .join("nodejs")
            .join("bin"),
    ]
}

fn vfox_node_bins(home: &Path) -> Vec<PathBuf> {
    let mut candidates = vfox_node_bin_candidates(home);
    if let Some(vfox_home) = std::env::var_os("VFOX_HOME") {
        candidates.push(
            PathBuf::from(vfox_home)
                .join("sdks")
                .join("nodejs")
                .join("bin"),
        );
    }
    candidates
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

#[cfg(target_os = "windows")]
fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

#[cfg(target_os = "windows")]
fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn vfox_node_bins_include_current_and_legacy_global_links() {
        let home = std::path::Path::new("/Users/example");
        let candidates = super::vfox_node_bin_candidates(home);

        assert_eq!(
            candidates[0],
            std::path::PathBuf::from("/Users/example/.vfox/sdks/nodejs/bin")
        );
        assert_eq!(
            candidates[1],
            std::path::PathBuf::from("/Users/example/.version-fox/sdks/nodejs/bin")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_prefers_spawnable_node_shims() {
        let npm = super::windows_binary_candidates("npm");
        let npx = super::windows_binary_candidates("npx");
        let node = super::windows_binary_candidates("node");

        assert_eq!(npm[0], "npm.cmd");
        assert_eq!(npx[0], "npx.cmd");
        assert_eq!(node[0], "node.exe");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_codex_cli_skips_windowsapps_aliases() {
        assert!(super::is_windows_app_execution_path(std::path::Path::new(
            r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\codex.exe"
        )));
        assert!(super::is_windows_app_execution_path(std::path::Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0.0_x64__abc\app\Codex.exe"
        )));
        assert!(!super::is_windows_app_execution_path(std::path::Path::new(
            r"C:\Users\me\AppData\Local\OpenAI\Codex\bin\123\codex.exe"
        )));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_finds_standard_git_install_dir() {
        let dirs = super::candidate_dirs();
        assert!(
            dirs.iter()
                .any(|dir| dir.to_string_lossy().ends_with(r"Git\cmd")),
            "candidate dirs should include Git for Windows cmd directory"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_finds_standard_system_command_dirs() {
        let Some(system_root) = std::env::var_os("SystemRoot").map(std::path::PathBuf::from) else {
            return;
        };
        let dirs = super::candidate_dirs();
        assert!(dirs.contains(&system_root));
        assert!(dirs.contains(&system_root.join("System32")));
        assert!(dirs.contains(&system_root.join("System32").join("OpenSSH")));
        assert!(dirs.contains(
            &system_root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_augmented_path_includes_common_tool_dirs() {
        let path = super::augmented_path_env().expect("augmented PATH");
        let dirs = std::env::split_paths(&path).collect::<Vec<_>>();

        if let Some(home) = dirs::home_dir() {
            assert!(dirs.contains(&home.join("AppData").join("Roaming").join("npm")));
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            assert!(dirs.contains(
                &std::path::PathBuf::from(program_files)
                    .join("Git")
                    .join("cmd")
            ));
        }
    }
}
