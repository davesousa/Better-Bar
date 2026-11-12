//! App identity helpers: shell lookups, launching, default pins.
//!
//! An app is identified the way Windows does it: by its AppUserModelID
//! (addressed as `shell:AppsFolder\<id>`, works for classic and Store apps),
//! or by a full .exe path for apps that don't declare one.

use windows::core::{GUID, HSTRING, PCWSTR, Result};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SHGetKnownFolderPath, ShellExecuteW,
    KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::Interface;

use crate::config::Pin;

pub const RECYCLE_BIN: &str = "shell:RecycleBinFolder";

/// A full filesystem path such as `C:\...\app.exe`.
pub fn is_path(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() > 2 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

fn parsing_name(id: &str) -> String {
    // Paths, shell locations and URIs (https:, ms-settings:, ms-screenclip:) pass through.
    let is_uri = id.contains("://") || id.starts_with("ms-");
    if is_path(id) || id.starts_with("shell:") || id.starts_with("::") || is_uri {
        id.to_string()
    } else {
        format!("shell:AppsFolder\\{id}")
    }
}

pub fn shell_item(id: &str) -> Result<IShellItem> {
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(parsing_name(id)), None) }
}

/// Display name and a 256 px icon from the shell (slow; the icon cache calls this once per app).
pub fn extract(id: &str) -> Option<(String, HBITMAP)> {
    unsafe {
        let item = shell_item(id).ok()?;
        let p = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?;
        let name = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let bmp = factory.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        Some((name, bmp))
    }
}

/// Open an app, path or shell location on a background thread so the dock never blocks.
pub fn launch(id: &str) {
    let target = parsing_name(id);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        CoUninitialize();
    });
}

/// Start an app elevated (Windows shows the UAC prompt).
pub fn launch_as_admin(id: &str) {
    let target = parsing_name(id);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteW(None, &HSTRING::from("runas"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        CoUninitialize();
    });
}

/// Show a program file selected in File Explorer.
pub fn reveal(path: &str) {
    let arg = format!("/select,\"{path}\"");
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from("explorer.exe"), &HSTRING::from(arg), PCWSTR::null(), SW_SHOWNORMAL);
        CoUninitialize();
    });
}

/// Open a file in Notepad (used for the settings file).
pub fn edit_file(path: &std::path::Path) {
    let p = path.to_string_lossy().to_string();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from("notepad.exe"), &HSTRING::from(format!("\"{p}\"")), PCWSTR::null(), SW_SHOWNORMAL);
        CoUninitialize();
    });
}

/// Squirrel-installed apps (Slack, Discord, Teams classic and other Electron apps) run from a
/// version folder, `...\slack\app-4.52.182\slack.exe`, that each update replaces, with a
/// stable launcher at `...\slack\slack.exe`. Map the versioned path to the launcher so pins
/// and running windows keep matching after an update. Other paths come back unchanged.
pub fn stable_path(path: &str) -> String {
    let lower = path.to_lowercase();
    let Some(i) = lower.rfind("\\app-") else { return path.to_string() };
    let rest = &path[i + 5..];
    let Some(j) = rest.find('\\') else { return path.to_string() };
    let (ver, file) = (&rest[..j], &rest[j + 1..]);
    if ver.is_empty() || !ver.chars().all(|c| c.is_ascii_digit() || c == '.') || file.contains('\\') {
        return path.to_string();
    }
    let launcher = format!("{}\\{}", &path[..i], file);
    if std::path::Path::new(&launcher).is_file() {
        launcher
    } else {
        path.to_string()
    }
}

/// AppsFolder ids for classic apps are often "{KnownFolderGUID}\relative\path.exe".
/// Resolve those to a real path so they can be matched against running processes.
pub fn resolve_path(id: &str) -> Option<String> {
    if is_path(id) {
        return Some(id.to_lowercase());
    }
    // File Explorer's windows carry no app id and the shell reports no target for it.
    if id.eq_ignore_ascii_case("Microsoft.Windows.Explorer") {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        return Some(format!("{windir}\\explorer.exe").to_lowercase());
    }
    known_folder_path(id).or_else(|| target_path(id))
}

/// The program an AppsFolder entry launches, e.g. "Microsoft.Windows.Explorer" → C:\Windows\explorer.exe.
/// Needed because some apps (Explorer included) don't tag their windows with an app id.
fn target_path(id: &str) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_Link_TargetParsingPath;
    use windows::Win32::UI::Shell::IShellItem2;
    unsafe {
        let item: IShellItem2 = shell_item(id).ok()?.cast().ok()?;
        let p = item.GetString(&PKEY_Link_TargetParsingPath).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| is_path(s)).map(|s| s.to_lowercase())
    }
}

fn known_folder_path(id: &str) -> Option<String> {
    let rest = id.strip_prefix('{')?;
    let close = rest.find('}')?;
    let guid = GUID::try_from(&rest[..close]).ok()?;
    let tail = rest[close + 1..].trim_start_matches('\\');
    unsafe {
        let p = SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None).ok()?;
        let base = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        Some(format!("{}\\{}", base?, tail).to_lowercase())
    }
}

/// Common apps, in two groups; first-run pins are the ones installed on this PC.
const GROUP_A: &[&str] = &[
    "Microsoft.Windows.Explorer",
    "MSEdge",
    "Chrome",
    "Microsoft.OutlookForWindows_8wekyb3d8bbwe!Microsoft.OutlookforWindows",
    "Microsoft.Windows.Photos_8wekyb3d8bbwe!App",
    "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
    "Spotify",
];
const GROUP_B: &[&str] = &[
    "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App",
    "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App",
    "Microsoft.VisualStudioCode",
    "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
    "Microsoft.Paint_8wekyb3d8bbwe!App",
    "Microsoft.ScreenSketch_8wekyb3d8bbwe!App",
    "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel",
];

pub fn default_pins() -> Vec<Pin> {
    let exists = |id: &&&str| shell_item(id).is_ok();
    let mut pins: Vec<Pin> = GROUP_A.iter().filter(exists).take(6).map(|s| Pin::Id(s.to_string())).collect();
    pins.push(Pin::sep());
    pins.extend(GROUP_B.iter().filter(exists).take(6).map(|s| Pin::Id(s.to_string())));
    pins
}
