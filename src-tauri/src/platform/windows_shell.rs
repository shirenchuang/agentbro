use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, IApplicationActivationManager, AO_NOERRORUI,
};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows_sys::Win32::UI::Shell::{
    AssocQueryStringW, ShellExecuteExW, ASSOCF_APP_TO_APP, ASSOCF_INIT_IGNOREUNKNOWN,
    ASSOCF_IS_PROTOCOL, ASSOCSTR_PROGID, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
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

    pub(crate) fn open_uri(&self, uri: &str) -> io::Result<()> {
        let (scheme, _) = uri
            .split_once(':')
            .filter(|(scheme, _)| !scheme.is_empty())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing URI scheme"))?;
        // The shell may report success after handing an unknown URI to the app picker.
        if !has_protocol_handler(scheme) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No app is registered for the {scheme} protocol"),
            ));
        }
        self.open(OsStr::new(uri))
    }

    pub(crate) fn activate_packaged_app(&self, app_id: &str) -> Result<(), String> {
        let app_id: Vec<u16> = app_id.encode_utf16().chain([0]).collect();
        let manager: IApplicationActivationManager =
            unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) }
                .map_err(|error| error.to_string())?;
        let pid = unsafe {
            manager.ActivateApplication(PCWSTR(app_id.as_ptr()), PCWSTR::null(), AO_NOERRORUI)
        }
        .map_err(|error| error.to_string())?;
        if pid == 0 {
            return Err("Windows did not return a process for the activated app".to_string());
        }
        Ok(())
    }
}

fn has_protocol_handler(scheme: &str) -> bool {
    let scheme: Vec<u16> = scheme.encode_utf16().chain([0]).collect();
    let mut length = 0;
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL | ASSOCF_INIT_IGNOREUNKNOWN | ASSOCF_APP_TO_APP,
            ASSOCSTR_PROGID,
            scheme.as_ptr(),
            null(),
            null_mut(),
            &mut length,
        )
    };
    HRESULT(result).is_ok() && length > 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_shell_resolves_registered_protocols() {
        assert!(super::has_protocol_handler("https"));
        assert!(!super::has_protocol_handler(
            "agentbro-fixture-unregistered-protocol"
        ));
    }
}

impl Drop for ShellApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
