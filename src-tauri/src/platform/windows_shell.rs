use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, IApplicationActivationManager, AO_NOERRORUI,
};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub(crate) struct ShellApartment;

impl ShellApartment {
    pub(crate) fn initialize() -> Result<Self, String> {
        let flags = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32;
        HRESULT(unsafe { CoInitializeEx(null(), flags) })
            .ok()
            .map_err(|error| format!("initialize Windows Shell: {error}"))?;
        Ok(Self)
    }

    pub(crate) fn open(&self, target: &OsStr) -> io::Result<()> {
        let target: Vec<u16> = target.encode_wide().chain([0]).collect();
        let mut execute = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpFile: target.as_ptr(),
            nShow: SW_SHOWNORMAL,
            ..Default::default()
        };
        if unsafe { ShellExecuteExW(&mut execute) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(crate) fn activate_packaged_app(&self, app_id: &str) -> windows::core::Result<u32> {
        let app_id: Vec<u16> = app_id.encode_utf16().chain([0]).collect();
        let manager: IApplicationActivationManager =
            unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) }?;
        unsafe {
            manager.ActivateApplication(PCWSTR(app_id.as_ptr()), PCWSTR::null(), AO_NOERRORUI)
        }
    }
}

impl Drop for ShellApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
