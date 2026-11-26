use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY,
};

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const MAX_PATH_BYTES: u32 = 65_536;

pub(super) fn find_registered(
    executables: &[String],
    matches_agent: impl Fn(&str) -> bool,
) -> Option<String> {
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
                .filter(|path| matches_agent(path))
            {
                return Some(path);
            }
        }
    }
    None
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
    let text = String::from_utf16(text).ok()?;
    let text = text.trim();
    Some(
        text.strip_prefix('"')
            .and_then(|path| path.strip_suffix('"'))
            .unwrap_or(text)
            .to_owned(),
    )
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
