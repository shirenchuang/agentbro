use std::time::Duration;
use tauri::{Emitter, Manager, WebviewWindow};
use tokio::sync::mpsc;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY};

pub(crate) fn watch(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    let (sender, mut changes) = mpsc::channel::<()>(1);
    let sender = Box::into_raw(Box::new(sender));
    // Setup runs on the window's owning thread. WM_NCDESTROY releases this sender.
    if unsafe { SetWindowSubclass(hwnd, Some(display_event), 0, sender as usize) } == 0 {
        unsafe { drop(Box::from_raw(sender)) };
        return Err("Could not subscribe to Windows display changes".to_string());
    }

    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        while changes.recv().await.is_some() {
            // Read monitor geometry after Windows has processed the display messages.
            tokio::time::sleep(Duration::from_millis(150)).await;
            let _ = changes.try_recv();
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let displays = super::display::list_displays_inner(&handle);
                let _ = handle.emit("display-changed", displays);
            });
        }
    });
    Ok(())
}

unsafe extern "system" fn display_event(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    sender: usize,
) -> LRESULT {
    let sender = sender as *mut mpsc::Sender<()>;
    match message {
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            let _ = unsafe { &*sender }.try_send(());
        }
        WM_NCDESTROY => unsafe {
            RemoveWindowSubclass(hwnd, Some(display_event), id);
            drop(Box::from_raw(sender));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
