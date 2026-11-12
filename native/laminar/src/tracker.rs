//! Running-window tracking: which windows would appear on the Windows taskbar,
//! which app each belongs to, and which is in front.
//!
//! Event-driven: WinEvent hooks tell us when windows show, hide, cloak,
//! minimize or take focus; the dock then rescans (debounced). No polling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::dock::WM_APP_RESCAN;

#[derive(Clone, Debug)]
pub struct Win {
    pub hwnd: HWND,
    /// Lower-case app identity: AppUserModelID if the app has one, else the exe path.
    pub app: String,
    /// Lower-case full exe path of the owning process.
    pub exe: String,
    // Kept for the window list in the upcoming styled menus and previews.
    #[allow(dead_code)]
    pub minimized: bool,
    #[allow(dead_code)]
    pub title: String,
}

/// Remembers each window's identity so a rescan doesn't re-query every process.
#[derive(Default)]
pub struct Tracker {
    identity: HashMap<isize, (String, String)>,
}

static NOTIFY: AtomicIsize = AtomicIsize::new(0);

/// Hook the window events that can change the dock (out-of-context: callbacks
/// arrive on this thread through its message loop).
pub fn install_hooks(notify: HWND) -> Vec<HWINEVENTHOOK> {
    NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
    let ranges = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
        (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
        (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
    ];
    ranges
        .iter()
        .map(|(a, b)| unsafe {
            SetWinEventHook(*a, *b, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)
        })
        .collect()
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 || hwnd.is_invalid() {
        return;
    }
    // Only top-level windows matter (child controls show and hide constantly).
    if event != EVENT_SYSTEM_FOREGROUND && GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    let target = HWND(NOTIFY.load(Ordering::Relaxed) as _);
    let _ = PostMessageW(Some(target), WM_APP_RESCAN, WPARAM(event as usize), LPARAM(hwnd.0 as isize));
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn is_explorer_taskbar(h: HWND) -> bool {
    matches!(class_name(h).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") && !crate::tray::is_laminar_window(h)
}

fn cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut v as *mut u32 as _, 4).is_ok() && v != 0 }
}

fn title(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The same rules the Windows taskbar uses, approximately.
fn is_taskbar_window(h: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(h).as_bool() || cloaked(h) {
            return false;
        }
        let ex = WINDOW_EX_STYLE(GetWindowLongPtrW(h, GWL_EXSTYLE) as u32);
        if ex.contains(WS_EX_APPWINDOW) {
            return true;
        }
        if ex.contains(WS_EX_TOOLWINDOW) || ex.contains(WS_EX_NOACTIVATE) {
            return false;
        }
        if GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false) {
            return false;
        }
        if title(h).is_empty() {
            return false;
        }
        !matches!(
            class_name(h).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "LaminarDock" | "Windows.UI.Core.CoreWindow"
        )
    }
}

fn window_aumid(h: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(h).ok()?;
        let mut v = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&v).ok().map(|p| {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as _));
            s
        });
        let _ = PropVariantClear(&mut v);
        s.filter(|s| !s.is_empty())
    }
}

/// (packaged AppUserModelID, exe path) of the window's process.
fn process_info(h: HWND) -> (Option<String>, String) {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (None, String::new());
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let aumid = if GetApplicationUserModelId(proc, &mut len, Some(PWSTR(buf.as_mut_ptr()))).is_ok() {
            Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
        } else {
            None
        };
        let mut len = buf.len() as u32;
        let exe = if QueryFullProcessImageNameW(proc, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };
        let _ = CloseHandle(proc);
        (aumid, exe)
    }
}

unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp.0 as *mut Vec<HWND>);
    v.push(h);
    true.into()
}

impl Tracker {
    /// All taskbar-style windows, front to back (most recently used first).
    pub fn scan(&mut self) -> Vec<Win> {
        let mut all: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
        }
        let mut out = Vec::new();
        let mut seen = HashMap::new();
        for h in all {
            if !is_taskbar_window(h) {
                continue;
            }
            let key = h.0 as isize;
            let (app, exe) = match self.identity.get(&key) {
                Some(v) => v.clone(),
                None => {
                    let (paumid, exe) = process_info(h);
                    let exe = crate::apps::stable_path(&exe);
                    let app = window_aumid(h).or(paumid).unwrap_or_else(|| exe.clone());
                    let v = (app.to_lowercase(), exe.to_lowercase());
                    self.identity.insert(key, v.clone());
                    v
                }
            };
            seen.insert(key, ());
            if app.is_empty() {
                continue;
            }
            out.push(Win { hwnd: h, app, exe, minimized: unsafe { IsIconic(h).as_bool() }, title: title(h) });
        }
        self.identity.retain(|k, _| seen.contains_key(k));
        out
    }
}

/// The top-level window that currently has focus.
pub fn foreground() -> HWND {
    unsafe { GetAncestor(GetForegroundWindow(), GA_ROOTOWNER) }
}

/// Bring a window to the front, restoring it if minimized.
pub fn activate(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(h);
    }
}

/// Bring a window forward while another app is in charge of input (during a drag, the
/// app being dragged from is). Windows only lets the process that sent the last input
/// change the foreground window, so first send an empty mouse input (no movement, no
/// buttons; the drag carries on). PowerToys uses the same step.
pub fn activate_during_drag(h: HWND) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_MOUSE};
    unsafe {
        let input = INPUT { r#type: INPUT_MOUSE, ..Default::default() };
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        SetForegroundWindow(h).as_bool()
    }
}

pub fn minimize(h: HWND) {
    unsafe {
        let _ = ShowWindowAsync(h, SW_MINIMIZE);
    }
}

/// Politely ask a window to close (the app can still prompt to save).
pub fn close(h: HWND) {
    unsafe {
        let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
