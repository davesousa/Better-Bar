//! Multiple displays.
//!
//! The main dock runs on the primary display and owns everything system-wide
//! (taskbar takeover, Windows key, watchdog). With "show on all displays" on, it
//! starts one lightweight helper dock process per extra display
//! (`laminar.exe --monitor N --parent PID`). Helpers draw only their own dock,
//! follow the same settings file live, and quit when the main dock exits.

use std::process::Child;

use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, MONITORINFOF_PRIMARY, WM_CLOSE};

#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub rect: RECT,
    pub primary: bool,
    pub dpi: u32,
}

unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let v = &mut *(lp.0 as *mut Vec<Display>);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(m, &mut mi).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        v.push(Display { rect: mi.rcMonitor, primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0, dpi: dx });
    }
    true.into()
}

/// All displays: the primary first, then the others left to right.
pub fn all() -> Vec<Display> {
    let mut v: Vec<Display> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|d| (!d.primary, d.rect.left, d.rect.top));
    v
}

/// The helper docks this (main) dock has started.
#[derive(Default)]
pub struct Helpers {
    children: Vec<(usize, Child)>,
}

impl Helpers {
    /// Make the running helpers match `wanted` (true: one per extra display; false: none).
    pub fn sync(&mut self, wanted: bool) {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let count = all().len();
        // Drop helpers that exited or whose display is gone.
        self.children.retain_mut(|(i, c)| *i < count && matches!(c.try_wait(), Ok(None)));
        if !wanted {
            self.stop();
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        for i in 1..count {
            if self.children.iter().any(|(k, _)| *k == i) {
                continue;
            }
            if let Ok(child) = std::process::Command::new(&exe)
                .arg("--monitor")
                .arg(i.to_string())
                .arg("--parent")
                .arg(std::process::id().to_string())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                self.children.push((i, child));
            }
        }
    }

    pub fn stop(&mut self) {
        for (_, mut c) in self.children.drain(..) {
            let _ = c.kill();
        }
    }
}

impl Drop for Helpers {
    fn drop(&mut self) {
        self.stop();
    }
}

/// In a helper dock: close our window when the main dock process exits.
pub fn watch_parent(parent_pid: u32, hwnd: HWND) {
    let target = hwnd.0 as isize;
    std::thread::spawn(move || unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) {
            WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
        let _ = PostMessageW(Some(HWND(target as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    });
}
