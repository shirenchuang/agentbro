#[cfg(any(target_os = "macos", target_os = "windows", test))]
use std::path::Path;

pub const ID: &str = "gemini-desktop";

#[cfg(target_os = "macos")]
pub const BUNDLE_ID: &str = "com.google.GeminiMacOS";

#[cfg(target_os = "macos")]
pub(super) fn is_macos_bundle(path: &Path) -> bool {
    std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(path.join("Contents/Info.plist"))
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == BUNDLE_ID
        })
}

#[cfg(target_os = "macos")]
pub(super) fn has_bundled_runtime() -> bool {
    use std::os::unix::fs::PermissionsExt;

    super::programs::installed_app_path_for_agent(ID).is_some_and(|path| {
        Path::new(&path)
            .join("Contents/Helpers/agy")
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

#[cfg(target_os = "macos")]
pub fn coding_host_pid(pid: u32) -> Option<u32> {
    let tree = crate::terminal::process_tree::build_tree();
    let mut current = pid;
    for _ in 0..20 {
        let process = tree.get(&current)?;
        let executable = Path::new(&process.command);
        if let Some(bundle) = executable
            .ancestors()
            .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        {
            if executable == bundle.join("Contents/MacOS/Gemini") && is_macos_bundle(bundle) {
                return Some(current);
            }
        }
        if current == process.ppid || process.ppid <= 1 {
            break;
        }
        current = process.ppid;
    }
    None
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn windows_release(launcher: &Path) -> Option<semver::Version> {
    if !launcher.is_file()
        || !launcher
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("Gemini.exe"))
    {
        return None;
    }
    if launcher.with_file_name("GEMINI_BUILD_INFO.txt").is_file() {
        return packaged_version(launcher);
    }
    // The launcher's file version belongs to the bootstrapper. Gemini keeps
    // the application and its build marker together in a versioned directory.
    std::fs::read_dir(launcher.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let executable = entry.path().join("Gemini.exe");
            (executable.is_file() && executable.with_file_name("GEMINI_BUILD_INFO.txt").is_file())
                .then(|| packaged_version(&executable))
                .flatten()
        })
        .max_by(semver::Version::cmp_precedence)
}

#[cfg(any(target_os = "windows", test))]
fn packaged_version(executable: &Path) -> Option<semver::Version> {
    executable
        .parent()?
        .file_name()?
        .to_str()?
        .strip_prefix("app-")?
        .parse()
        .ok()
}
