use std::io;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, S_OK};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER, TDM_CLICK_BUTTON, TDN_CREATED, TDN_TIMER,
    TD_WARNING_ICON,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, IDCANCEL};

pub(crate) const WINDOW_TIMEOUT: Duration = Duration::from_secs(45);
const CLOSE_BUTTON: i32 = 100;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    Starting,
    Ready,
    Closing,
}

#[derive(Default)]
pub(crate) struct StartupRecovery {
    phase: Mutex<Phase>,
    changed: Condvar,
}

impl StartupRecovery {
    pub(crate) fn windows_created(&self) -> bool {
        let mut phase = self.phase();
        if *phase == Phase::Closing {
            return false;
        }
        *phase = Phase::Ready;
        self.changed.notify_one();
        true
    }

    pub(crate) fn watch(self: Arc<Self>, timeout: Duration) -> io::Result<JoinHandle<()>> {
        thread::Builder::new()
            .name("windows-startup-recovery".to_string())
            .spawn(move || self.run(timeout))
    }

    fn phase(&self) -> MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn waiting_past_timeout(&self, timeout: Duration) -> bool {
        let (phase, _) = self
            .changed
            .wait_timeout_while(self.phase(), timeout, |phase| *phase == Phase::Starting)
            .unwrap_or_else(PoisonError::into_inner);
        *phase == Phase::Starting
    }

    fn run(&self, timeout: Duration) {
        while self.waiting_past_timeout(timeout) {
            log::warn!(
                "Startup: Windows desktop windows are still being created after {} seconds",
                timeout.as_secs()
            );
            match self.show_dialog() {
                Ok(true) => {
                    {
                        let mut phase = self.phase();
                        if *phase != Phase::Starting {
                            return;
                        }
                        // Prevent late WebView completion from starting normal setup during exit.
                        *phase = Phase::Closing;
                    }
                    log::warn!("Startup: closing AgentBro at the user's request");
                    std::process::exit(1);
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("Could not show the Windows startup recovery dialog: {error:#010x}");
                    return;
                }
            }
        }
    }

    fn show_dialog(&self) -> Result<bool, i32> {
        let language = unsafe { GetUserDefaultUILanguage() };
        let [heading, details, wait, close] = messages(language & 0x03ff)
            .map(|text| text.encode_utf16().chain([0]).collect::<Vec<_>>());
        let buttons = [
            TASKDIALOG_BUTTON {
                nButtonID: IDCANCEL,
                pszButtonText: wait.as_ptr(),
            },
            TASKDIALOG_BUTTON {
                nButtonID: CLOSE_BUTTON,
                pszButtonText: close.as_ptr(),
            },
        ];
        let dialog = TASKDIALOGCONFIG {
            cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
            dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_CALLBACK_TIMER,
            pszWindowTitle: windows::core::w!("AgentBro").as_ptr(),
            Anonymous1: TASKDIALOGCONFIG_0 {
                pszMainIcon: TD_WARNING_ICON,
            },
            pszMainInstruction: heading.as_ptr(),
            pszContent: details.as_ptr(),
            cButtons: buttons.len() as u32,
            pButtons: buttons.as_ptr(),
            nDefaultButton: IDCANCEL,
            pfCallback: Some(dismiss_when_ready),
            lpCallbackData: self as *const Self as isize,
            ..Default::default()
        };
        let mut button = IDCANCEL;
        let result = unsafe { TaskDialogIndirect(&dialog, &mut button, null_mut(), null_mut()) };
        if result < 0 {
            return Err(result);
        }
        Ok(button == CLOSE_BUTTON)
    }
}

unsafe extern "system" fn dismiss_when_ready(
    window: HWND,
    message: u32,
    _: usize,
    _: isize,
    state: isize,
) -> i32 {
    if matches!(message as i32, TDN_CREATED | TDN_TIMER) {
        let startup = unsafe { &*(state as *const StartupRecovery) };
        if *startup.phase() == Phase::Ready {
            unsafe { PostMessageW(window, TDM_CLICK_BUTTON as u32, IDCANCEL as usize, 0) };
        }
    }
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_dir::try_windows_startup_lock;
    use crate::skills::installer::TempInstallDir;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    };

    const FIXTURE: &str = "AGENTBRO_TEST_STARTUP_RECOVERY_DIR";
    const PORT: u16 = 17894;

    struct Process(Child);

    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn start_fixture(directory: &Path) -> Process {
        Process(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "platform::windows_startup::tests::native_dialog_handles_wait_ready_and_close",
                ])
                .env(FIXTURE, directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    fn wait_for<T>(description: &str, mut probe: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(value) = probe() {
                return value;
            }
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for {description}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn visible_dialog(pid: u32) -> Option<HWND> {
        struct Query {
            pid: u32,
            window: HWND,
        }
        unsafe extern "system" fn inspect(window: HWND, data: isize) -> i32 {
            let query = unsafe { &mut *(data as *mut Query) };
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(window, &mut pid) };
            if pid != query.pid || unsafe { IsWindowVisible(window) } == 0 {
                return 1;
            }
            let mut title = [0u16; 32];
            let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
            if length > 0 && String::from_utf16_lossy(&title[..length as usize]) == "AgentBro" {
                query.window = window;
                return 0;
            }
            1
        }

        let mut query = Query {
            pid,
            window: null_mut(),
        };
        unsafe { EnumWindows(Some(inspect), &mut query as *mut Query as isize) };
        (!query.window.is_null()).then_some(query.window)
    }

    fn click_dialog(window: HWND, button: i32) {
        assert_ne!(
            unsafe { PostMessageW(window, TDM_CLICK_BUTTON as u32, button as usize, 0) },
            0
        );
    }

    #[test]
    fn native_dialog_handles_wait_ready_and_close() {
        if let Some(directory) = std::env::var_os(FIXTURE) {
            let directory = Path::new(&directory);
            let _lock = try_windows_startup_lock(directory, PORT).unwrap().unwrap();
            let recovery = Arc::new(StartupRecovery::default());
            let worker = Arc::clone(&recovery)
                .watch(Duration::from_millis(400))
                .unwrap();
            wait_for("the fixture startup to finish", || {
                directory.join("ready").is_file().then_some(())
            });
            assert!(recovery.windows_created());
            worker.join().unwrap();
            return;
        }

        let temp = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let mut late = start_fixture(temp.path());
        wait_for("the slow startup dialog", || visible_dialog(late.0.id()));
        std::fs::write(temp.path().join("ready"), []).unwrap();
        let status = wait_for("automatic dismissal after window creation", || {
            late.0.try_wait().unwrap()
        });
        assert!(status.success(), "{status}");
        std::fs::remove_file(temp.path().join("ready")).unwrap();

        let mut stalled = start_fixture(temp.path());
        let dialog = wait_for("the stalled startup dialog", || {
            visible_dialog(stalled.0.id())
        });
        assert!(try_windows_startup_lock(temp.path(), PORT)
            .unwrap()
            .is_none());
        click_dialog(dialog, IDCANCEL);
        wait_for("the Keep waiting action to dismiss the dialog", || {
            (unsafe { IsWindow(dialog) } == 0).then_some(())
        });
        let dialog = wait_for("the dialog to return while startup remains blocked", || {
            visible_dialog(stalled.0.id())
        });
        click_dialog(dialog, CLOSE_BUTTON);
        let status = wait_for("the stalled app to exit", || stalled.0.try_wait().unwrap());
        assert_eq!(status.code(), Some(1));
        assert!(try_windows_startup_lock(temp.path(), PORT)
            .unwrap()
            .is_some());
    }
}

// This dialog can appear before the WebView and its locale files are available.
fn messages(language: u16) -> [&'static str; 4] {
    match language {
        0x04 => [
            "AgentBro 启动时间较长",
            "Windows 仍在初始化 WebView2。你可以继续等待，也可以关闭 AgentBro，然后从「开始」菜单重新打开。",
            "继续等待",
            "关闭 AgentBro",
        ],
        0x11 => [
            "AgentBro の起動に時間がかかっています",
            "Windows は WebView2 を初期化しています。このまま待つか、AgentBro を閉じてスタートメニューから開き直してください。",
            "このまま待つ",
            "AgentBro を閉じる",
        ],
        0x12 => [
            "AgentBro를 시작하는 데 시간이 걸리고 있습니다",
            "Windows에서 WebView2를 초기화하고 있습니다. 계속 기다리거나 AgentBro를 닫고 시작 메뉴에서 다시 열 수 있습니다.",
            "계속 기다리기",
            "AgentBro 닫기",
        ],
        0x1f => [
            "AgentBro’nun başlatılması uzun sürüyor",
            "Windows, WebView2’yi başlatıyor. Bekleyebilir veya AgentBro’yu kapatıp Başlat menüsünden yeniden açabilirsiniz.",
            "Beklemeye devam et",
            "AgentBro’yu kapat",
        ],
        _ => [
            "AgentBro is taking longer to start",
            "Windows is still initializing WebView2. You can keep waiting or close AgentBro and open it again from the Start menu.",
            "Keep waiting",
            "Close AgentBro",
        ],
    }
}
