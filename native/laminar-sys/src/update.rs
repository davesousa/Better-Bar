//! Windows Update status for Launchpad's control center.
//!
//! Uses the Windows Update Agent API offline: it reads the result of Windows' own last
//! scan (no network, no new scan), so it's quick and never triggers a download. Blocking;
//! call it from a worker thread with COM initialised.

use windows::core::BSTR;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER};
use windows::Win32::System::UpdateAgent::{ISystemInformation, IUpdateSession, SystemInformation, UpdateSession};
use windows::Win32::Foundation::VARIANT_FALSE;

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    /// Updates are installed and Windows needs a restart to finish.
    RestartRequired,
    /// Updates found by Windows' last scan that aren't installed yet.
    Pending(u32),
    UpToDate,
    /// Windows Update couldn't be asked (service disabled, policy, error).
    Unknown,
}

/// Restart needed? Asks Windows Update, then the registry flag it sets.
fn restart_required() -> bool {
    let wua = unsafe { CoCreateInstance::<_, ISystemInformation>(&SystemInformation, None, CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER) }
        .ok()
        .and_then(|s| unsafe { s.RebootRequired() }.ok())
        .map(|v| v.as_bool())
        .unwrap_or(false);
    wua || registry_key_exists("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\RebootRequired")
}

fn registry_key_exists(path: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ};
    let mut k = HKEY::default();
    unsafe {
        let ok = RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(path), Some(0), KEY_READ, &mut k).is_ok();
        if ok {
            let _ = RegCloseKey(k);
        }
        ok
    }
}

/// Updates Windows' last scan found that aren't installed (or hidden by the user).
fn pending_count() -> Option<u32> {
    unsafe {
        let session: IUpdateSession = CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER).ok()?;
        let searcher = session.CreateUpdateSearcher().ok()?;
        searcher.SetOnline(VARIANT_FALSE).ok()?;
        let result = searcher.Search(&BSTR::from("IsInstalled=0 and IsHidden=0")).ok()?;
        let n = result.Updates().ok()?.Count().ok()?;
        Some(n.max(0) as u32)
    }
}

pub fn status() -> UpdateStatus {
    if restart_required() {
        return UpdateStatus::RestartRequired;
    }
    match pending_count() {
        Some(0) => UpdateStatus::UpToDate,
        Some(n) => UpdateStatus::Pending(n),
        None => UpdateStatus::Unknown,
    }
}
