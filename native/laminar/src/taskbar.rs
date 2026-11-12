//! Taking over from the Windows taskbar, safely.
//!
//! * Hide Explorer's taskbar windows while Laminar runs (Explorer itself keeps
//!   running: desktop, File Explorer, Start and notifications still work).
//! * If the taskbar wasn't set to auto-hide, switch it to auto-hide so it stops
//!   reserving screen space, and remember to switch it back.
//! * A watchdog process waits for Laminar to exit for ANY reason (quit, crash,
//!   Task Manager) and then restores the taskbar.
//! * When the dock is always visible it registers as an "AppBar", so maximized
//!   windows stop above it, like they do above the Windows taskbar.

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABM_GETSTATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABM_SETSTATE, ABS_AUTOHIDE,
    APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, ShowWindow, SW_HIDE, SW_SHOWNA};

use crate::dock::WM_APP_APPBAR;

fn trays() -> Vec<HWND> {
    let mut v = Vec::new();
    // Explorer's own taskbar, never the main dock's tray stand-in (same class name).
    if let Some(h) = crate::tray::explorer_tray() {
        v.push(h);
    }
    unsafe {
        let mut prev: Option<HWND> = None;
        while let Ok(h) = FindWindowExW(None, prev, w!("Shell_SecondaryTrayWnd"), None) {
            v.push(h);
            prev = Some(h);
        }
    }
    v
}

pub fn set_visible(visible: bool) {
    for h in trays() {
        unsafe {
            let _ = ShowWindow(h, if visible { SW_SHOWNA } else { SW_HIDE });
        }
    }
}

fn abd(hwnd: Option<HWND>) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd.unwrap_or_default(),
        ..Default::default()
    }
}

/// Explorer taskbar state bits (ABS_AUTOHIDE etc).
pub fn tray_state() -> u32 {
    let mut d = abd(trays().first().copied());
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) as u32 }
}

pub fn set_tray_state(state: u32) {
    let mut d = abd(trays().first().copied());
    d.lParam = LPARAM(state as isize);
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut d);
    }
}

/// Hide the taskbar. Returns the original state if we had to switch on auto-hide.
pub fn take_over() -> Option<u32> {
    let state = tray_state();
    let changed = if state & ABS_AUTOHIDE == 0 {
        set_tray_state(state | ABS_AUTOHIDE);
        Some(state)
    } else {
        None
    };
    set_visible(false);
    changed
}

pub fn restore(original_state: Option<u32>) {
    if let Some(s) = original_state {
        set_tray_state(s);
    }
    set_visible(true);
}

/// Start `laminar.exe --watchdog <pid> [state]`, a tiny process that restores the taskbar when we exit.
pub fn spawn_watchdog(original_state: Option<u32>) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--watchdog").arg(std::process::id().to_string());
        if let Some(s) = original_state {
            cmd.arg(s.to_string());
        }
        let _ = cmd.creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

/// Body of the watchdog process.
pub fn run_watchdog(pid: u32, original_state: Option<u32>) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
    }
    restore(original_state);
}

/// Reserve (or release) a strip at the bottom of the monitor for the dock.
pub fn appbar(hwnd: HWND, monitor: RECT, height: i32, reserve: bool, registered: &mut bool) {
    let mut d = abd(Some(hwnd));
    d.uCallbackMessage = WM_APP_APPBAR;
    d.uEdge = ABE_BOTTOM;
    d.rc = RECT { left: monitor.left, top: monitor.bottom - height, right: monitor.right, bottom: monitor.bottom };
    unsafe {
        if reserve {
            if !*registered {
                SHAppBarMessage(ABM_NEW, &mut d);
                *registered = true;
            }
            SHAppBarMessage(ABM_QUERYPOS, &mut d);
            d.rc.top = d.rc.bottom - height;
            SHAppBarMessage(ABM_SETPOS, &mut d);
        } else if *registered {
            SHAppBarMessage(ABM_REMOVE, &mut d);
            *registered = false;
        }
    }
}
