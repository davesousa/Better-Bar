//! Laminar system services. Blocking functions meant to be called from worker threads.
//!
//! Threading model: every function in this crate blocks the calling thread (most return
//! in well under 100 ms; `apps::installed_apps`, `files::search_files` and `weather::*`
//! can take longer). Call them from worker threads, never from the UI thread. Each
//! worker thread must initialise COM once with [`com_init_sta`] before calling anything
//! in `apps`, `audio`, `media`, `radios` or `files`. WinRT async operations are waited on
//! with a bounded timeout (see the internal `wait!` macro), so nothing here can hang
//! forever on a misbehaving system component.

#![cfg(windows)]

/// Waits for a WinRT `IAsyncOperation`/`IAsyncAction` with a timeout (milliseconds).
///
/// `windows-future`'s `join()` blocks indefinitely and does not pump messages, so instead
/// we register a completion callback (`when`) that sets a Win32 event, wait on it with
/// `CoWaitForMultipleHandles` (which pumps COM/window messages so STA-bound completions
/// can run), then fetch the result with `GetResults()` on the calling thread (many WinRT
/// result types are not `Send`). On timeout the operation is cancelled and `None` is
/// returned. The expression must evaluate to `windows::core::Result<IAsync...>`.
macro_rules! wait {
    ($op:expr, $ms:expr) => {{
        (|| {
            let op = $op.ok()?;
            let sig = $crate::util::Signal::new()?;
            let sig2 = sig.clone();
            op.when(move |_| sig2.set()).ok()?;
            if sig.wait($ms) {
                op.GetResults().ok()
            } else {
                let _ = op.Cancel();
                None
            }
        })()
    }};
}

pub mod apps;
pub mod audio;
pub mod files;
pub mod media;
pub mod power;
pub mod radios;
pub mod search;
pub mod stats;
pub mod theme;
pub mod update;
pub mod weather;

mod util;

pub use apps::AppEntry;

/// Initialises COM on the calling thread as a single-threaded apartment
/// (`CoInitializeEx(COINIT_APARTMENTTHREADED)`); also sufficient for the WinRT APIs used
/// here. Call once per worker thread before using `apps`, `audio`, `media`, `radios` or
/// `files`. Returns `true` if COM is usable on this thread (including when it was
/// already initialised, even in a different apartment mode).
pub fn com_init_sta() -> bool {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    // S_OK / S_FALSE => fine. RPC_E_CHANGED_MODE => already MTA, still usable.
    hr.is_ok() || hr == windows::Win32::Foundation::RPC_E_CHANGED_MODE
}
