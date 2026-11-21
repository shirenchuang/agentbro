use std::collections::HashSet;
use std::mem::{size_of, size_of_val};
use std::ptr::{null, null_mut};
use uuid::Uuid;
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

pub(super) const WSL_REGISTRATIONS: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_error(code: u32) -> String {
    format!(
        "Could not read WSL registrations: {}",
        std::io::Error::from_raw_os_error(code as i32)
    )
}

fn read_string(key: HKEY, subkey: *const u16, name: &[u16]) -> Option<String> {
    let mut buffer = [0u16; 257];
    let mut bytes = size_of_val(&buffer) as u32;
    let result = unsafe {
        RegGetValueW(
            key,
            subkey,
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if result != ERROR_SUCCESS || !bytes.is_multiple_of(2) {
        return None;
    }
    let value = buffer.get(..bytes as usize / size_of::<u16>())?;
    String::from_utf16(value.strip_suffix(&[0])?).ok()
}

fn is_ready(key: HKEY, subkey: &[u16], state_name: &[u16]) -> bool {
    let mut state = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            key,
            subkey.as_ptr(),
            state_name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut state as *mut u32).cast(),
            &mut bytes,
        )
    };
    // WSL's installed and running states are the ones ready for Hook access.
    result == ERROR_SUCCESS && matches!(state, 1 | 2)
}

pub(super) fn list_registered_distributions(path: &str) -> Result<Vec<String>, String> {
    let mut key = null_mut();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            KEY_READ,
            &mut key,
        )
    };
    match result {
        ERROR_SUCCESS => {}
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(Vec::new()),
        error => return Err(registry_error(error)),
    }
    let key = RegistryKey(key);
    let default = read_string(key.0, null(), &wide("DefaultDistribution"))
        .and_then(|guid| Uuid::parse_str(&guid).ok());
    let distribution_name = wide("DistributionName");
    let state_name = wide("State");
    let mut distributions = Vec::new();
    for index in 0.. {
        let mut subkey = [0u16; 39];
        let mut name_len = subkey.len() as u32;
        let result = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                subkey.as_mut_ptr(),
                &mut name_len,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        match result {
            ERROR_NO_MORE_ITEMS => break,
            ERROR_MORE_DATA => continue,
            ERROR_SUCCESS => {}
            error => return Err(registry_error(error)),
        }
        let guid = String::from_utf16(&subkey[..name_len as usize])
            .ok()
            .and_then(|name| Uuid::parse_str(&name).ok());
        let Some(guid) = guid else { continue };
        if !is_ready(key.0, &subkey, &state_name) {
            continue;
        }
        if let Some(name) = read_string(key.0, subkey.as_ptr(), &distribution_name) {
            if super::valid_distribution(&name) {
                distributions.push((Some(guid) != default, name));
            }
        }
    }
    distributions.sort_unstable();
    let mut seen = HashSet::new();
    Ok(distributions
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| seen.insert(name.clone()))
        .collect())
}
