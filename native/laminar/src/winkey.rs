//! The Windows key opens Launchpad.
//!
//! A low-level keyboard hook watches the Windows keys. A press and release with
//! no other key in between is a "lone" press: we tell the dock to toggle
//! Launchpad, and inject one harmless unassigned key (0xE8) before the release
//! so Windows sees "Win + something" and does not open Start. Win + any other
//! key (Win+E, Win+D, Win+Shift+S…) passes through untouched.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::*;

static TARGET: AtomicIsize = AtomicIsize::new(0);
static ENABLED: AtomicBool = AtomicBool::new(true);
static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static COMBO: AtomicBool = AtomicBool::new(false);

pub struct Hook(HHOOK);

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
    }
}

pub fn install(notify: HWND, enabled: bool) -> Option<Hook> {
    TARGET.store(notify.0 as isize, Ordering::Relaxed);
    ENABLED.store(enabled, Ordering::Relaxed);
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0).ok().map(Hook) }
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn inject_mask_key() {
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe {
        SendInput(&[key(false), key(true)], std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && ENABLED.load(Ordering::Relaxed) {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let injected = k.flags.0 & LLKHF_INJECTED.0 != 0;
        if !injected {
            let is_win = k.vkCode == VK_LWIN.0 as u32 || k.vkCode == VK_RWIN.0 as u32;
            let msg = wp.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if is_win && down {
                if !WIN_DOWN.swap(true, Ordering::Relaxed) {
                    COMBO.store(false, Ordering::Relaxed);
                }
            } else if !is_win && down && WIN_DOWN.load(Ordering::Relaxed) {
                COMBO.store(true, Ordering::Relaxed);
            } else if is_win && up && WIN_DOWN.swap(false, Ordering::Relaxed) && !COMBO.load(Ordering::Relaxed) {
                inject_mask_key();
                let target = HWND(TARGET.load(Ordering::Relaxed) as _);
                let _ = PostMessageW(Some(target), crate::dock::WM_APP_WINKEY, WPARAM(0), LPARAM(0));
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
