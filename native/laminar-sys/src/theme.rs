//! Light/dark mode and accent colour.
//!
//! Dark mode is the per-user registry pair under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`
//! (`AppsUseLightTheme`, `SystemUsesLightTheme`); after writing, `WM_SETTINGCHANGE` with
//! lParam "ImmersiveColorSet" is broadcast (`SendMessageTimeoutW`, abort-if-hung, 1 s) so
//! Explorer and apps repaint, exactly as the Settings app does. Accent colour comes from
//! `HKCU\Software\Microsoft\Windows\DWM\AccentColor` (the Settings accent), falling back to
//! `DwmGetColorizationColor`. No COM needed.

use crate::util::wide;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, WPARAM};
use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};

const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

fn read_dword(key: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = 4u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()), RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut _), Some(&mut size)) };
    (r == ERROR_SUCCESS).then_some(data)
}

fn write_dword(key: &str, value: &str, data: u32) -> bool {
    let (k, v) = (wide(key), wide(value));
    let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()), REG_DWORD.0, Some(&data as *const u32 as *const _), 4) };
    r == ERROR_SUCCESS
}

/// True when apps are set to dark mode (`AppsUseLightTheme == 0`). Defaults to light.
pub fn apps_use_dark() -> bool {
    read_dword(PERSONALIZE, "AppsUseLightTheme") == Some(0)
}

/// True when the system (taskbar/Start) uses dark mode (`SystemUsesLightTheme == 0`).
pub fn system_uses_dark() -> bool {
    read_dword(PERSONALIZE, "SystemUsesLightTheme") == Some(0)
}

/// Switches both apps and system to dark (`true`) or light, then broadcasts
/// `WM_SETTINGCHANGE("ImmersiveColorSet")`. Returns true if the registry writes succeeded.
/// The broadcast can take up to ~1 s if some top-level window is slow.
pub fn set_dark_mode(dark: bool) -> bool {
    let light = if dark { 0 } else { 1 };
    let ok = write_dword(PERSONALIZE, "AppsUseLightTheme", light) & write_dword(PERSONALIZE, "SystemUsesLightTheme", light);
    let param = wide("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(param.as_ptr() as isize), SMTO_ABORTIFHUNG, 1000, None);
    }
    ok
}

/// The user's accent colour as (r, g, b).
pub fn accent_color() -> Option<(u8, u8, u8)> {
    // DWM\AccentColor is ABGR (0xAABBGGRR).
    if let Some(c) = read_dword("Software\\Microsoft\\Windows\\DWM", "AccentColor") {
        return Some(((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8));
    }
    // DwmGetColorizationColor is ARGB (0xAARRGGBB).
    let mut c = 0u32;
    let mut opaque = windows::core::BOOL(0);
    unsafe { windows::Win32::Graphics::Dwm::DwmGetColorizationColor(&mut c, &mut opaque) }.ok()?;
    Some((((c >> 16) & 0xff) as u8, ((c >> 8) & 0xff) as u8, (c & 0xff) as u8))
}
