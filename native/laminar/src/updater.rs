//! Installing and updating, through Velopack (https://velopack.io).
//!
//! `Setup.exe` (built by `vpk pack`, see .github/workflows/release.yml) installs Laminar
//! per user in `%LocalAppData%\LaminarDock` (the package id), without admin rights. Releases live on the
//! project's GitHub Releases page; the dock checks them, downloads a newer version quietly
//! and shows an update icon. Nothing installs until the user picks "Restart and Update".
//!
//! A copy that wasn't installed by Setup.exe (a developer build in `target\release`) has
//! no updates and never touches the sign-in entry.

use std::sync::{Mutex, OnceLock};

use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager, VelopackApp, VelopackAsset};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

/// Where releases are published.
pub const REPO: &str = "https://github.com/davesousa/Better-Bar";

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: windows::core::PCWSTR = w!("Laminar Dock");

/// What the last update check found.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Not installed through Setup.exe (developer build): no updates.
    NotInstalled,
    /// Not checked yet.
    Idle,
    Checking,
    UpToDate,
    /// This version is downloaded and ready to install.
    Ready(String),
    Failed(String),
}

static STATUS: Mutex<Status> = Mutex::new(Status::Idle);
static READY: Mutex<Option<VelopackAsset>> = Mutex::new(None);

/// Run first thing in `main`. Velopack starts the app with special arguments during
/// install, update and uninstall; those runs do their job here and exit.
pub fn startup() {
    VelopackApp::build()
        // Updates install only when the user asks (the dock restarts to install).
        .set_auto_apply_on_startup(false)
        .on_before_uninstall_fast_callback(|_| {
            quit_running_dock();
            set_start_at_login(false);
            // In case the dock couldn't put Windows' taskbar back itself.
            crate::taskbar::set_visible(true);
        })
        .run();
}

fn manager() -> Option<UpdateManager> {
    UpdateManager::new(GithubSource::new(REPO, None, false), None, None).ok()
}

/// The installed version, or None for a copy Setup.exe didn't install. Looked up once.
fn installed_version() -> Option<&'static str> {
    static V: OnceLock<Option<String>> = OnceLock::new();
    V.get_or_init(|| manager().map(|m| m.get_current_version_as_string())).as_deref()
}

/// True when this copy was installed by Setup.exe (so it can update).
pub fn installed() -> bool {
    installed_version().is_some()
}

/// "0.1.0", or "0.1.0 (developer build)" when not installed.
pub fn version_label() -> String {
    match installed_version() {
        Some(v) => v.to_string(),
        None => format!("{} (developer build)", env!("CARGO_PKG_VERSION")),
    }
}

pub fn status() -> Status {
    STATUS.lock().map(|s| s.clone()).unwrap_or(Status::Idle)
}

fn set_status(s: Status) {
    if let Ok(mut g) = STATUS.lock() {
        *g = s;
    }
}

/// Check GitHub for a newer version on a background thread and download it quietly.
/// Posts `msg` to `hwnd` when done; read the result with `status()`.
pub fn check(hwnd: HWND, msg: u32) {
    if matches!(status(), Status::Checking) {
        return;
    }
    let target = hwnd.0 as isize;
    set_status(Status::Checking);
    std::thread::spawn(move || {
        let result = (|| -> Status {
            let Some(m) = manager() else { return Status::NotInstalled };
            // Downloaded earlier but not installed yet: no need to ask GitHub again.
            if let Some(asset) = m.get_update_pending_restart() {
                let v = asset.Version.clone();
                *READY.lock().unwrap() = Some(asset);
                return Status::Ready(v);
            }
            match m.check_for_updates() {
                Ok(UpdateCheck::UpdateAvailable(info)) => match m.download_updates(&info, None) {
                    Ok(()) => {
                        let v = info.TargetFullRelease.Version.clone();
                        *READY.lock().unwrap() = Some(info.TargetFullRelease.clone());
                        Status::Ready(v)
                    }
                    Err(e) => Status::Failed(format!("download failed: {e}")),
                },
                Ok(_) => Status::UpToDate,
                Err(e) => Status::Failed(format!("{e}")),
            }
        })();
        set_status(result);
        unsafe {
            let _ = PostMessageW(Some(HWND(target as _)), msg, WPARAM(0), LPARAM(0));
        }
    });
}

/// Start Velopack's updater for the downloaded version. It waits for this process to exit,
/// installs, and starts the new version. The caller then shuts the dock down cleanly.
pub fn begin_install() -> Result<(), String> {
    let asset = READY.lock().ok().and_then(|g| g.clone()).ok_or("no update downloaded")?;
    let m = manager().ok_or("not installed")?;
    m.wait_exit_then_apply_updates(&asset, false, true, Vec::<String>::new()).map_err(|e| e.to_string())
}

/// Release notes page for a version.
pub fn notes_url(version: &str) -> String {
    format!("{REPO}/releases/tag/v{version}")
}

/// Start Laminar when the user signs in (per user, HKCU\…\Run). Installed copies only.
pub fn set_start_at_login(on: bool) {
    if !installed() {
        return;
    }
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let value: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(Some(0)).collect();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, REG_SZ.0, Some(value.as_ptr() as _), (value.len() * 2) as u32);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME);
        }
    }
}

/// Ask every running dock window to quit (as "Quit Laminar" does, so Windows' taskbar is
/// restored), then wait up to 10 s for them to go.
fn quit_running_dock() {
    for _ in 0..50 {
        let Ok(h) = (unsafe { FindWindowW(w!("LaminarDock"), None) }) else { return };
        if h.is_invalid() {
            return;
        }
        unsafe {
            let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
