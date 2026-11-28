use sha2::{Digest, Sha256};
use std::fmt::Display;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, S_OK, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForSingleObject, INFINITE,
};
use windows_sys::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER, TDM_CLICK_BUTTON, TDN_CREATED, TDN_TIMER,
    TD_WARNING_ICON,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, PostMessageW, IDCANCEL, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
};

pub(crate) const STARTUP_TIMEOUT: Duration = Duration::from_secs(45);
const CLOSE_BUTTON: i32 = 100;

pub(crate) struct ReopenEvent(OwnedHandle);

impl ReopenEvent {
    pub(crate) fn open(directory: &Path, port: u16) -> io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let path_bytes = directory
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let name = format!(
            "Local\\AgentBro.reopen.{port}.{:x}",
            Sha256::digest(path_bytes)
        )
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();
        let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle) }))
    }

    pub(crate) fn signal(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.0.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn wait(&self, timeout: u32) -> io::Result<bool> {
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), timeout) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub(crate) fn watch(
        self,
        app: tauri::AppHandle,
        reopen: fn(&tauri::AppHandle),
    ) -> io::Result<()> {
        thread::Builder::new()
            .name("windows-reopen".to_string())
            .spawn(move || loop {
                match self.wait(INFINITE) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        log::warn!("Could not wait for a Windows reopen request: {error}");
                        return;
                    }
                }
                let handle = app.clone();
                let (finished, completion) = mpsc::channel();
                if let Err(error) = app.run_on_main_thread(move || {
                    reopen(&handle);
                    let _ = finished.send(());
                }) {
                    log::warn!("Could not request the Windows Settings window: {error}");
                    return;
                }
                // WebView creation pumps Windows messages. Let it finish before another open.
                if completion.recv().is_err() {
                    return;
                }
            })
            .map(|_| ())
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    StartingHookService,
    OpeningDesktop,
    Ready,
    Closing,
    Failed,
}

impl Phase {
    fn is_pending(self) -> bool {
        matches!(self, Self::StartingHookService | Self::OpeningDesktop)
    }
}

#[derive(Default)]
pub(crate) struct StartupRecovery {
    phase: Mutex<Phase>,
    changed: Condvar,
}

impl StartupRecovery {
    pub(crate) fn hook_listening(&self) -> bool {
        self.transition(Phase::OpeningDesktop)
    }

    pub(crate) fn setup_finished(&self) -> bool {
        self.transition(Phase::Ready)
    }

    pub(crate) fn is_starting(&self) -> bool {
        self.phase().is_pending()
    }

    fn transition(&self, next: Phase) -> bool {
        let mut phase = self.phase();
        if !matches!(
            (*phase, next),
            (Phase::StartingHookService, Phase::OpeningDesktop)
                | (Phase::OpeningDesktop, Phase::Ready)
                | (
                    Phase::StartingHookService | Phase::OpeningDesktop,
                    Phase::Closing | Phase::Failed
                )
        ) {
            return false;
        }
        *phase = next;
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
        if !self.transition(Phase::Failed) {
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

    fn phase_after_wait(&self, timeout: Duration) -> Phase {
        let (phase, _) = self
            .changed
            .wait_timeout_while(self.phase(), timeout, |phase| phase.is_pending())
            .unwrap_or_else(PoisonError::into_inner);
        *phase
    }

    fn run(&self, timeout: Duration) {
        loop {
            let pending = match self.phase_after_wait(timeout) {
                Phase::StartingHookService => "the Hook service to start",
                Phase::OpeningDesktop => "desktop setup to finish",
                _ => return,
            };
            log::warn!(
                "Startup: Windows is still waiting for {pending} after {} seconds",
                timeout.as_secs(),
            );
            match self.show_dialog() {
                Ok(true) => {
                    if !self.transition(Phase::Closing) {
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
        if !startup.is_starting() {
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

    #[test]
    fn reopen_event_keeps_an_early_request_for_the_matching_profile_and_port() {
        let profile = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let other_profile = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let primary = ReopenEvent::open(profile.path(), PORT).unwrap();
        let duplicate = ReopenEvent::open(profile.path(), PORT).unwrap();
        let separate_port = ReopenEvent::open(profile.path(), PORT + 1).unwrap();
        let separate_profile = ReopenEvent::open(other_profile.path(), PORT).unwrap();

        duplicate.signal().unwrap();
        duplicate.signal().unwrap();
        drop(duplicate);
        assert!(!separate_port.wait(0).unwrap());
        assert!(!separate_profile.wait(0).unwrap());
        assert!(primary.wait(0).unwrap());
        assert!(!primary.wait(0).unwrap());
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
    fn startup_requires_the_hook_listener_and_finished_desktop_setup() {
        let recovery = StartupRecovery::default();
        assert!(!recovery.setup_finished());
        assert!(recovery.is_starting());
        assert!(recovery.hook_listening());
        assert!(matches!(
            recovery.phase_after_wait(Duration::ZERO),
            Phase::OpeningDesktop
        ));
        assert!(recovery.setup_finished());
        assert!(!recovery.is_starting());
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
            wait_for("the fixture Hook listener", || {
                directory.join("hook-ready").is_file().then_some(())
            });
            assert!(recovery.hook_listening());
            std::fs::write(directory.join("waiting-desktop"), []).unwrap();
            wait_for("the fixture startup to finish", || {
                directory.join("ready").is_file().then_some(())
            });
            assert!(recovery.setup_finished());
            worker.join().unwrap();
            return;
        }

        let temp = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let mut late = start_fixture(temp.path());
        let dialog = wait_for("the slow startup dialog", || visible_dialog(late.0.id()));
        std::fs::write(temp.path().join("hook-ready"), []).unwrap();
        wait_for("the fixture to wait for desktop setup", || {
            temp.path().join("waiting-desktop").is_file().then_some(())
        });
        assert_ne!(
            unsafe { IsWindow(dialog) },
            0,
            "the Hook listener alone must keep the dialog open"
        );
        std::fs::write(temp.path().join("ready"), []).unwrap();
        let status = wait_for("automatic dismissal after desktop setup", || {
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
            "AgentBro 仍在初始化桌面界面和 Hook 服务。你可以继续等待，也可以关闭 AgentBro，然后从「开始」菜单重新打开。",
            "继续等待",
            "关闭 AgentBro",
        ],
        0x11 => [
            "AgentBro の起動に時間がかかっています",
            "AgentBro はデスクトップ画面と Hook サービスを初期化しています。このまま待つか、AgentBro を閉じてスタートメニューから開き直してください。",
            "このまま待つ",
            "AgentBro を閉じる",
        ],
        0x12 => [
            "AgentBro를 시작하는 데 시간이 걸리고 있습니다",
            "AgentBro가 데스크톱 화면과 Hook 서비스를 초기화하고 있습니다. 계속 기다리거나 AgentBro를 닫고 시작 메뉴에서 다시 열 수 있습니다.",
            "계속 기다리기",
            "AgentBro 닫기",
        ],
        0x1f => [
            "AgentBro’nun başlatılması uzun sürüyor",
            "AgentBro, masaüstü arayüzünü ve Hook hizmetini başlatıyor. Bekleyebilir veya AgentBro’yu kapatıp Başlat menüsünden yeniden açabilirsiniz.",
            "Beklemeye devam et",
            "AgentBro’yu kapat",
        ],
        _ => [
            "AgentBro is taking longer to start",
            "AgentBro is still initializing its desktop and Hook service. You can keep waiting or close AgentBro and open it again from the Start menu.",
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
