//! Installing and updating, through Velopack (https://velopack.io).
//!
//! `Setup.exe` (built by `vpk pack`, see .github/workflows/release.yml) installs Laminar
//! per user in `%LocalAppData%\LaminarDock` (the package id), without admin rights. Releases live on the
//! project's GitHub Releases page; the dock checks them, downloads a newer version quietly
//! and shows an update icon. Nothing installs until the user picks "Restart and Update".
//!
//! A copy that wasn't installed by Setup.exe (a developer build in `target\release`) has
//! no updates and never touches the sign-in shortcut.

use std::sync::{Mutex, OnceLock};

use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager, VelopackApp, VelopackAsset};
use std::os::windows::ffi::OsStrExt;

use windows::core::{w, Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{FOLDERID_Startup, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
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
            // A version downloaded earlier but not installed (an install that failed, say).
            // Always ask GitHub too, so a newer release replaces it instead of the dock
            // offering the old one forever.
            let pending = m.get_update_pending_restart();
            let ready = |asset: VelopackAsset| {
                let v = asset.Version.clone();
                *READY.lock().unwrap() = Some(asset);
                Status::Ready(v)
            };
            match m.check_for_updates() {
                Ok(UpdateCheck::UpdateAvailable(info)) => {
                    let latest = info.TargetFullRelease.clone();
                    if let Some(p) = pending.filter(|p| p.Version == latest.Version) {
                        return ready(p);
                    }
                    match m.download_updates(&info, None) {
                        Ok(()) => ready(latest),
                        Err(e) => Status::Failed(format!("download failed: {e}")),
                    }
                }
                Ok(_) => match pending {
                    Some(p) => ready(p),
                    None => Status::UpToDate,
                },
                // Offline: the earlier download can still be installed.
                Err(e) => match pending {
                    Some(p) => ready(p),
                    None => Status::Failed(format!("{e}")),
                },
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

/// Start Laminar when the user signs in. Installed copies only.
///
/// This uses a shortcut in the user's Startup folder (as most Windows apps do),
/// not the `HKCU\…\Run` registry value older versions wrote. An unsigned program that
/// copies itself into AppData and adds itself to `Run` is what antivirus heuristics call a
/// "startup dropper" (Avira: `Drop.Win64.RegStartupSelfCopy`), so the dock no longer touches
/// `Run` except to remove the old value. The shortcut is only created when missing and the
/// setting is on, and removed when the setting is off.
pub fn set_start_at_login(on: bool) {
    if !installed() {
        return;
    }
    remove_legacy_run_value();
    let Some(lnk) = startup_shortcut_path() else { return };
    if !on {
        let _ = std::fs::remove_file(&lnk);
    } else if !lnk.exists() {
        if let Err(e) = create_startup_shortcut(&lnk) {
            eprintln!("couldn't create the Startup shortcut: {e}");
        }
    }
}

/// Versions up to 0.2.1 started at sign-in through a `Run` registry value. Remove it.
fn remove_legacy_run_value() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME);
    }
}

/// `%AppData%\Microsoft\Windows\Start Menu\Programs\Startup\Laminar Dock.lnk`.
fn startup_shortcut_path() -> Option<std::path::PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None).ok()?;
        let dir = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        Some(std::path::PathBuf::from(dir?).join("Laminar Dock.lnk"))
    }
}

/// Point the shortcut at Velopack's stub (`<install>\Laminar Dock.exe`), which survives
/// updates and starts the current version; fall back to this exe.
fn create_startup_shortcut(lnk: &std::path::Path) -> windows::core::Result<()> {
    let exe = std::env::current_exe().map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
    let stub = exe.parent().and_then(|d| d.parent()).map(|root| root.join("Laminar Dock.exe"));
    let target = match stub {
        Some(s) if s.exists() => s,
        _ => exe,
    };
    let dir = target.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let wide = |p: &std::path::Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let t = wide(&target);
        let d = wide(&dir);
        link.SetPath(PCWSTR(t.as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(d.as_ptr()))?;
        let file: IPersistFile = link.cast()?;
        let l = wide(lnk);
        file.Save(PCWSTR(l.as_ptr()), true)?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    #[test]
    fn startup_shortcut_is_created() {
        let dir = std::env::temp_dir().join("laminar-startup-test");
        let _ = std::fs::create_dir_all(&dir);
        let lnk = dir.join("Laminar Dock.lnk");
        let _ = std::fs::remove_file(&lnk);
        super::create_startup_shortcut(&lnk).expect("shortcut");
        assert!(std::fs::metadata(&lnk).map(|m| m.len() > 0).unwrap_or(false));
        let _ = std::fs::remove_file(&lnk);
        assert!(super::startup_shortcut_path().is_some_and(|p| p.ends_with(r"Startup\Laminar Dock.lnk")));
    }
}
