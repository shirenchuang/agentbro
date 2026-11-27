use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY,
};

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const MAX_PATH_BYTES: u32 = 65_536;

struct AppRegistration {
    hive: HKEY,
    view: u32,
    key: Vec<u16>,
    executable: String,
}

pub(super) fn find_registered(
    executables: &[String],
    matches_agent: impl Fn(&str) -> bool,
) -> Option<String> {
    find_registration(executables, matches_agent).map(|registration| registration.executable)
}

fn find_registration(
    executables: &[String],
    matches_executable: impl Fn(&str) -> bool,
) -> Option<AppRegistration> {
    let keys: Vec<Vec<u16>> = executables
        .iter()
        .map(|name| {
            format!(r"{APP_PATHS}\{name}")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    for (hive, view) in [
        (HKEY_CURRENT_USER, 0),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6464KEY),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6432KEY),
    ] {
        for key in &keys {
            if let Some(path) = read_registered_path(hive, view, key, None)
                .and_then(executable_path)
                .filter(|path| matches_executable(path))
            {
                return Some(AppRegistration {
                    hive,
                    view,
                    key: key.clone(),
                    executable: path,
                });
            }
        }
    }
    None
}

pub(super) fn launch_command(executable: &Path, registered_names: &[String]) -> Command {
    let mut command = Command::new(executable);
    if let Some(path) = app_search_path(executable, registered_names) {
        command.env("PATH", path);
    }
    command
}

fn app_search_path(executable: &Path, registered_names: &[String]) -> Option<std::ffi::OsString> {
    let selected = executable.canonicalize().ok()?;
    let registration = find_registration(registered_names, |path| {
        Path::new(path)
            .canonicalize()
            .is_ok_and(|registered| registered == selected)
    })?;
    let name: Vec<_> = "Path".encode_utf16().chain([0]).collect();
    let prefix = read_registered_text(
        registration.hive,
        registration.view,
        &registration.key,
        Some(&name),
    )?;
    let mut paths: Vec<_> = env::split_paths(&prefix)
        .filter(|directory| directory.is_absolute())
        .collect();
    if paths.is_empty() {
        return None;
    }
    if let Some(inherited) = env::var_os("PATH") {
        paths.extend(env::split_paths(&inherited));
    }
    env::join_paths(paths).ok()
}

pub(super) fn gemini_install_location() -> Option<String> {
    let key: Vec<_> = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Gemini"
        .encode_utf16()
        .chain([0])
        .collect();
    let name: Vec<_> = "InstallLocation".encode_utf16().chain([0]).collect();
    [
        (HKEY_CURRENT_USER, 0),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6464KEY),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6432KEY),
    ]
    .into_iter()
    .find_map(|(hive, view)| {
        let root = PathBuf::from(read_registered_path(hive, view, &key, Some(&name))?);
        if !root.is_absolute() {
            return None;
        }
        let launcher = root.join("Gemini.exe");
        super::super::gemini_desktop::windows_release(&launcher)?;
        launcher.into_os_string().into_string().ok()
    })
}

fn read_registered_path(
    hive: HKEY,
    view: u32,
    key: &[u16],
    name: Option<&[u16]>,
) -> Option<String> {
    let text = read_registered_text(hive, view, key, name)?;
    let text = text.trim();
    Some(
        text.strip_prefix('"')
            .and_then(|path| path.strip_suffix('"'))
            .unwrap_or(text)
            .to_owned(),
    )
}

fn read_registered_text(
    hive: HKEY,
    view: u32,
    key: &[u16],
    name: Option<&[u16]>,
) -> Option<String> {
    let name = name.map_or(null(), <[u16]>::as_ptr);
    let flags = view | RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    let mut bytes = 0;
    let status = unsafe {
        RegGetValueW(
            hive,
            key.as_ptr(),
            name,
            flags,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS || !(2..=MAX_PATH_BYTES).contains(&bytes) || bytes % 2 != 0 {
        return None;
    }
    let mut value = vec![0u16; bytes as usize / 2];
    let status = unsafe {
        RegGetValueW(
            hive,
            key.as_ptr(),
            name,
            flags,
            null_mut(),
            value.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS || bytes % 2 != 0 {
        return None;
    }
    let text = value.get(..bytes as usize / 2)?.strip_suffix(&[0])?;
    if text.contains(&0) {
        return None;
    }
    String::from_utf16(text).ok()
}

fn executable_path(target: String) -> Option<String> {
    let mut path = PathBuf::from(target);
    if !path.is_absolute() {
        return None;
    }
    if path.extension().is_none() {
        path.set_extension("exe");
    }
    let is_executable = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    if is_executable && path.is_file() {
        path.into_os_string().into_string().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRegistration {
        reg: PathBuf,
        key: String,
    }

    impl TestRegistration {
        fn set(&self, value_arg: &[&str], kind: &str, value: &str) {
            let status = Command::new(&self.reg)
                .args(["add", &self.key, "/f"])
                .args(value_arg)
                .args(["/t", kind, "/d", value])
                .output()
                .unwrap()
                .status;
            assert!(status.success(), "Could not write the test App Paths entry");
        }
    }

    impl Drop for TestRegistration {
        fn drop(&mut self) {
            let _ = Command::new(&self.reg)
                .args(["delete", &self.key, "/f"])
                .output();
        }
    }

    #[test]
    fn windows_launch_inherits_search_directories_only_from_the_matching_registration() {
        let system32 = PathBuf::from(env::var_os("SystemRoot").unwrap()).join("System32");
        let executable = system32.join("where.exe");
        let name = format!("AgentBro-launch-test-{}.exe", uuid::Uuid::new_v4());
        let registration = TestRegistration {
            reg: system32.join("reg.exe"),
            key: format!(r"HKCU\{APP_PATHS}\{name}"),
        };
        let names = [name];
        registration.set(&["/ve"], "REG_SZ", executable.to_str().unwrap());
        assert_eq!(
            find_registered(&names, |_| true).as_deref(),
            executable.to_str()
        );
        assert!(launch_command(&executable, &names)
            .get_envs()
            .next()
            .is_none());

        let fixture =
            crate::skills::installer::TempInstallDir::create_in(&env::temp_dir()).unwrap();
        let lib = fixture.path().join("Shared helpers 路径");
        let extra = fixture.path().join("Second helper folder");
        std::fs::create_dir(&lib).unwrap();
        std::fs::create_dir(&extra).unwrap();
        let helper = format!("agentbro-helper-{}.exe", uuid::Uuid::new_v4());
        std::fs::write(lib.join(&helper), "").unwrap();
        let fixture_name = fixture.path().file_name().unwrap().to_str().unwrap();
        let prefix = format!(
            r#""%TEMP%\{fixture_name}\Shared helpers 路径";"{}";relative-folder"#,
            extra.display()
        );
        registration.set(&["/v", "Path"], "REG_EXPAND_SZ", &prefix);

        let inherited = env::var_os("PATH");
        let dependency_is_found = |mut command: Command| {
            command
                .current_dir(fixture.path())
                .arg(&helper)
                .output()
                .unwrap()
                .status
                .success()
        };
        assert!(!dependency_is_found(Command::new(&executable)));

        let command = launch_command(&executable, &names);
        let (_, value) = command.get_envs().next().unwrap();
        let directories: Vec<_> = env::split_paths(value.unwrap()).collect();
        assert_eq!(
            directories[..2]
                .iter()
                .map(|path| path.canonicalize().unwrap())
                .collect::<Vec<_>>(),
            [lib.canonicalize().unwrap(), extra.canonicalize().unwrap()]
        );
        assert_eq!(
            &directories[2..],
            inherited
                .as_ref()
                .map(|path| env::split_paths(path).collect::<Vec<_>>())
                .unwrap_or_default()
        );
        assert!(dependency_is_found(command));
        assert!(env::var_os("PATH") == inherited);

        let other = fixture.path().join("where.exe");
        std::fs::copy(&executable, &other).unwrap();
        assert!(launch_command(&other, &names).get_envs().next().is_none());
    }
}
