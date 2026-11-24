use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_QUERY};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(crate) fn saved_path() -> Option<OsString> {
    let mut token = null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE,
            &mut token,
        )
    } == 0
    {
        return None;
    }

    let mut block = null_mut();
    let created = unsafe { CreateEnvironmentBlock(&mut block, token, 0) };
    unsafe { CloseHandle(token) };
    if created == 0 {
        return None;
    }

    let path = unsafe { path_from_block(block.cast()) };
    unsafe { DestroyEnvironmentBlock(block) };
    path
}

unsafe fn path_from_block(mut entry: *const u16) -> Option<OsString> {
    // CreateEnvironmentBlock returns NUL-terminated entries followed by an empty entry.
    while *entry != 0 {
        let mut len = 0;
        while *entry.add(len) != 0 {
            len += 1;
        }
        let value = std::slice::from_raw_parts(entry, len);
        if let Some(name) = value.get(..5) {
            if name.iter().zip(b"PATH=").all(|(&unit, letter)| {
                u8::try_from(unit).is_ok_and(|ascii| ascii.eq_ignore_ascii_case(letter))
            }) {
                return Some(OsString::from_wide(&value[5..]));
            }
        }
        entry = entry.add(len + 1);
    }
    None
}
