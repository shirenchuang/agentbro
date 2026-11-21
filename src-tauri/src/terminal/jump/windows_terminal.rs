use crate::terminal::process_tree::ProcessInfo;
use std::collections::HashMap;

fn terminal_ancestor(mut pid: u32, tree: &HashMap<u32, ProcessInfo>) -> Option<u32> {
    for _ in 0..20 {
        if pid <= 1 {
            break;
        }
        let process = tree.get(&pid)?;
        if process
            .command
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("WindowsTerminal.exe"))
        {
            return Some(pid);
        }
        pid = process.ppid;
    }
    None
}

fn unique_window<T>(windows: impl IntoIterator<Item = T>) -> Option<T> {
    let mut windows = windows.into_iter();
    let first = windows.next()?;
    windows.next().is_none().then_some(first)
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindowAsync, GWL_EXSTYLE, GW_OWNER,
    SW_RESTORE, WS_EX_TOOLWINDOW,
};

#[cfg(target_os = "windows")]
pub(super) fn focus_existing(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> bool {
    let Some(terminal_pid) = terminal_ancestor(pid, tree) else {
        return false;
    };
    let Some(window) = unique_window(visible_windows(terminal_pid)) else {
        return false;
    };
    unsafe {
        let mut owner = 0;
        if GetWindowThreadProcessId(window, &mut owner) == 0 || owner != terminal_pid {
            return false;
        }
        if IsIconic(window) != 0 && ShowWindowAsync(window, SW_RESTORE) == 0 {
            return false;
        }
        GetForegroundWindow() == window || SetForegroundWindow(window) != 0
    }
}

#[cfg(target_os = "windows")]
fn visible_windows(pid: u32) -> Vec<HWND> {
    struct Search {
        pid: u32,
        windows: Vec<HWND>,
    }

    unsafe extern "system" fn collect(window: HWND, param: LPARAM) -> windows_sys::core::BOOL {
        let search = unsafe { &mut *(param as *mut Search) };
        let mut owner = 0;
        unsafe {
            if GetWindowThreadProcessId(window, &mut owner) != 0
                && owner == search.pid
                && IsWindowVisible(window) != 0
                && GetWindow(window, GW_OWNER).is_null()
                && GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize == 0
            {
                search.windows.push(window);
            }
        }
        1
    }

    let mut search = Search {
        pid,
        windows: Vec::new(),
    };
    if unsafe { EnumWindows(Some(collect), &mut search as *mut Search as LPARAM) } == 0 {
        return Vec::new();
    }
    search.windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_windows_terminal_only_in_the_session_ancestry() {
        let tree = [
            (30, 20, "agent.exe"),
            (20, 10, "pwsh.exe"),
            (10, 1, r"C:\WindowsApps\WINDOWSTERMINAL.EXE"),
            (40, 1, "WindowsTerminal.exe"),
            (50, 1, "agent.exe"),
        ]
        .into_iter()
        .map(|(pid, ppid, command)| {
            (
                pid,
                ProcessInfo {
                    pid,
                    ppid,
                    command: command.to_string(),
                    tty: None,
                },
            )
        })
        .collect();

        assert_eq!(terminal_ancestor(30, &tree), Some(10));
        assert_eq!(terminal_ancestor(50, &tree), None);
        assert_eq!(terminal_ancestor(0, &tree), None);
        assert_eq!(terminal_ancestor(99, &tree), None);
    }

    #[test]
    fn only_selects_an_unambiguous_window() {
        assert_eq!(unique_window([1]), Some(1));
        assert_eq!(unique_window([1, 2]), None);
        assert_eq!(unique_window::<u32>([]), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_enumeration_filters_windows_by_owner_and_visibility() {
        use std::ptr::{null, null_mut};
        use windows_sys::w;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };

        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0) };
            }
        }

        let create = |owner: HWND, visible: bool, ex_style: u32| {
            let window = unsafe {
                CreateWindowExW(
                    ex_style,
                    w!("STATIC"),
                    w!("AgentBro terminal routing fixture"),
                    WS_OVERLAPPEDWINDOW | if visible { WS_VISIBLE } else { 0 },
                    0,
                    0,
                    250,
                    120,
                    owner,
                    null_mut(),
                    null_mut(),
                    null(),
                )
            };
            assert!(!window.is_null(), "create fixture window");
            Window(window)
        };

        let pid = std::process::id();
        let first = create(null_mut(), true, 0);
        let _owned = create(first.0, true, 0);
        let _hidden = create(null_mut(), false, 0);
        let _tool = create(null_mut(), true, WS_EX_TOOLWINDOW);
        assert_eq!(unique_window(visible_windows(pid)), Some(first.0));

        let _second = create(null_mut(), true, 0);
        assert!(unique_window(visible_windows(pid)).is_none());
        assert!(visible_windows(0).is_empty());
    }
}
