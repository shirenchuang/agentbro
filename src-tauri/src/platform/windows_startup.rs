use std::fmt::Display;
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
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, PostMessageW, IDCANCEL, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
};

pub(crate) const WINDOW_TIMEOUT: Duration = Duration::from_secs(45);
const CLOSE_BUTTON: i32 = 100;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    Starting,
    Ready,
    Closing,
    Failed,
}

#[derive(Default)]
pub(crate) struct StartupRecovery {
    phase: Mutex<Phase>,
    changed: Condvar,
}

impl StartupRecovery {
    pub(crate) fn windows_created(&self) -> bool {
        self.finish(Phase::Ready)
    }

    fn finish(&self, state: Phase) -> bool {
        let mut phase = self.phase();
        if *phase != Phase::Starting && !(*phase == Phase::Ready && state == Phase::Failed) {
            return false;
        }
        *phase = state;
        self.changed.notify_one();
        true
    }

    pub(crate) fn fail(&self, stage: &str, error: impl Display) {
        let language = unsafe { GetUserDefaultUILanguage() };
        let [heading, guidance] = failure_messages(language & 0x03ff);
        self.show_failure(stage, error, heading, guidance);
    }

    pub(crate) fn fail_hook_listener(&self, port: u16, error: impl Display) {
        let language = unsafe { GetUserDefaultUILanguage() };
        let (heading, guidance) = hook_failure_messages(language & 0x03ff, port);
        self.show_failure(
            &format!("could not open the Hook listener on port {port}"),
            error,
            &heading,
            guidance,
        );
    }

    fn show_failure(&self, stage: &str, error: impl Display, heading: &str, guidance: &str) {
        if !self.finish(Phase::Failed) {
            return;
        }
        log::error!("Startup: {stage}: {error}");
        let details: Vec<u16> = format!("{heading}\n\n{guidance}\n\n{error}")
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            MessageBoxW(
                null_mut(),
                details.as_ptr(),
                windows::core::w!("AgentBro").as_ptr(),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            )
        };
        std::process::exit(1);
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
                    if !self.finish(Phase::Closing) {
                        return;
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
            pfCallback: Some(dismiss_when_startup_finishes),
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

unsafe extern "system" fn dismiss_when_startup_finishes(
    window: HWND,
    message: u32,
    _: usize,
    _: isize,
    state: isize,
) -> i32 {
    if matches!(message as i32, TDN_CREATED | TDN_TIMER) {
        let startup = unsafe { &*(state as *const StartupRecovery) };
        if *startup.phase() != Phase::Starting {
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
    fn startup_lock_excludes_only_the_same_port_and_releases_on_drop() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let launch = try_windows_startup_lock(directory.path(), PORT)
            .unwrap()
            .unwrap();
        assert!(try_windows_startup_lock(directory.path(), PORT)
            .unwrap()
            .is_none());
        assert!(try_windows_startup_lock(directory.path(), PORT + 1)
            .unwrap()
            .is_some());
        drop(launch);
        assert!(try_windows_startup_lock(directory.path(), PORT)
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn hook_port_stays_reserved_without_accepting_connections_during_startup() {
        let mut endpoint = crate::hook_endpoint::HookEndpoint {
            tcp_port: 0,
            socket_path: String::new(),
        };
        let reservation = endpoint.reserve_tcp_socket().unwrap();
        let address = reservation.local_addr().unwrap();
        endpoint.tcp_port = address.port();
        assert!(endpoint.reserve_tcp_socket().is_err());
        assert!(
            std::net::TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_err()
        );

        let listener = reservation.listen(1024).unwrap();
        let client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (_, peer) = listener.accept().await.unwrap();
        assert_eq!(peer, client.local_addr().unwrap());
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

fn failure_messages(language: u16) -> [&'static str; 2] {
    match language {
        0x04 => [
            "AgentBro 无法打开窗口",
            "请检查 Microsoft Edge WebView2 Runtime 是否已安装、WebView2 数据目录是否可写，然后重新打开 AgentBro。",
        ],
        0x11 => [
            "AgentBro のウィンドウを開けません",
            "Microsoft Edge WebView2 Runtime のインストールと WebView2 データフォルダーへの書き込み権限を確認してから、AgentBro を開き直してください。",
        ],
        0x12 => [
            "AgentBro 창을 열 수 없습니다",
            "Microsoft Edge WebView2 Runtime 설치와 WebView2 데이터 폴더의 쓰기 권한을 확인한 다음 AgentBro를 다시 여세요.",
        ],
        0x1f => [
            "AgentBro penceresi açılamadı",
            "Microsoft Edge WebView2 Runtime kurulumunu ve WebView2 veri klasörünün yazılabilir olduğunu kontrol edip AgentBro’yu yeniden açın.",
        ],
        _ => [
            "AgentBro could not open its window",
            "Check the Microsoft Edge WebView2 Runtime installation and make sure its data folder is writable. Then reopen AgentBro.",
        ],
    }
}

fn hook_failure_messages(language: u16, port: u16) -> (String, &'static str) {
    match language {
        0x04 => (
            format!("AgentBro 无法打开 Hook 端口 {port}"),
            "请检查是否有另一个程序占用这个端口，或 Windows 是否限制了对它的访问，然后重新打开 AgentBro。",
        ),
        0x11 => (
            format!("AgentBro は Hook ポート {port} を開けません"),
            "別のアプリがポートを使用していないか、Windows がアクセスを制限していないか確認してから、AgentBro を開き直してください。",
        ),
        0x12 => (
            format!("AgentBro가 Hook 포트 {port}을(를) 열 수 없습니다"),
            "다른 앱이 이 포트를 사용 중이거나 Windows가 액세스를 제한하는지 확인한 다음 AgentBro를 다시 여세요.",
        ),
        0x1f => (
            format!("AgentBro, {port} numaralı Hook bağlantı noktasını açamadı"),
            "Başka bir uygulamanın bu bağlantı noktasını kullanıp kullanmadığını veya Windows'un erişimi kısıtlayıp kısıtlamadığını kontrol edin. Ardından AgentBro'yu yeniden açın.",
        ),
        _ => (
            format!("AgentBro could not open Hook port {port}"),
            "Check whether another app is using this port or Windows is restricting access to it. Then reopen AgentBro.",
        ),
    }
}
