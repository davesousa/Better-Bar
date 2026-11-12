//! Installed applications, as listed in Start's "All apps".
//!
//! Uses the Shell namespace folder `shell:AppsFolder` (`SHCreateItemFromParsingName` →
//! `IShellItem::BindToHandler(BHID_EnumItems)` → `IEnumShellItems`). This is the same
//! source Start uses, so it covers both Win32 shortcuts and packaged (UWP/MSIX) apps in
//! one list, without walking Start Menu folders or the package manager ourselves.
//!
//! Launching: an entry's `id` can be launched with
//! `ShellExecuteW(.., "shell:AppsFolder\\" + id, ..)` (or `explorer.exe shell:AppsFolder\<id>`).
//! Requires COM on the calling thread ([`crate::com_init_sta`]).

use crate::util::{take_pwstr, wide};
use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::UI::Shell::{IEnumShellItems, IShellItem, SHCreateItemFromParsingName, BHID_EnumItems, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING};

/// One launchable entry from "All apps".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Parsing name relative to AppsFolder: an AppUserModelID (`Microsoft.WindowsCalculator_8wekyb3d8bbwe!App`,
    /// `Chrome`) or a known-folder path (`{6D809377-...}\Notepad++\notepad++.exe`).
    pub id: String,
    /// Display name as shown in Start.
    pub name: String,
}

/// Enumerates `shell:AppsFolder`, filters obvious non-apps (uninstallers, readmes, help
/// files, web links), sorts by name (case-insensitive) and dedupes by id.
/// Typically 50-300 ms cold. Returns an empty list on failure.
pub fn installed_apps() -> Vec<AppEntry> {
    let mut out = enumerate().unwrap_or_default();
    out.retain(|a| !is_junk(a));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    let mut seen = std::collections::HashSet::new();
    out.retain(|a| seen.insert(a.id.to_lowercase()));
    out
}

fn enumerate() -> windows::core::Result<Vec<AppEntry>> {
    let path = wide("shell:AppsFolder");
    unsafe {
        let folder: IShellItem = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None::<&IBindCtx>)?;
        let en: IEnumShellItems = folder.BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)?;
        let mut out = Vec::with_capacity(256);
        loop {
            let mut batch: [Option<IShellItem>; 64] = std::array::from_fn(|_| None);
            let mut fetched = 0u32;
            if en.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            for item in batch.iter().take(fetched as usize).flatten() {
                let id = item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).map(take_pwstr).unwrap_or_default();
                let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(take_pwstr).unwrap_or_default();
                if !id.is_empty() && !name.trim().is_empty() {
                    out.push(AppEntry { id, name: name.trim().to_string() });
                }
            }
        }
        Ok(out)
    }
}

fn is_junk(a: &AppEntry) -> bool {
    let name = a.name.to_lowercase();
    let id = a.id.to_lowercase();
    // "Uninstall Foo" / "Foo Uninstall", or an uninstaller exe (unins000.exe, uninst.exe,
    // uninstall.exe). Whole-word match so "Revo Uninstaller" (an app) survives.
    let file = id.rsplit(['\\', '/']).next().unwrap_or(&id);
    if name.split_whitespace().any(|w| w == "uninstall") || file.starts_with("unins") || file.starts_with("uninst") {
        return true;
    }
    const DOC_EXT: [&str; 11] = [".url", ".chm", ".txt", ".pdf", ".html", ".htm", ".rtf", ".hlp", ".ini", ".log", ".md"];
    if DOC_EXT.iter().any(|e| id.ends_with(e)) || id.starts_with("http://") || id.starts_with("https://") {
        return true;
    }
    // Name-based doc filters only for path-style entries; packaged apps such as
    // "Get Help" or "Tips" are legitimate apps.
    let path_like = id.contains('\\') || id.contains('/');
    if path_like {
        const DOC_WORDS: [&str; 10] = ["read me", "readme", "help", "documentation", "release notes", "manual", "license", "website", "user guide", "changelog"];
        if DOC_WORDS.iter().any(|w| name == *w || name.ends_with(&format!(" {w}")) || name.starts_with(&format!("{w} "))) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(id: &str, name: &str) -> AppEntry {
        AppEntry { id: id.into(), name: name.into() }
    }
    #[test]
    fn junk_filter() {
        assert!(is_junk(&e("{X}\\Foo\\unins000.exe", "Uninstall Foo")));
        assert!(is_junk(&e("{X}\\Foo\\readme.txt", "Foo")));
        assert!(is_junk(&e("{X}\\Foo\\foo.chm", "Foo Help")));
        assert!(is_junk(&e("{X}\\Foo\\docs.exe", "Foo Help")));
        assert!(is_junk(&e("https://example.com/", "Example")));
        assert!(!is_junk(&e("Microsoft.GetHelp_8wekyb3d8bbwe!App", "Get Help")));
        assert!(!is_junk(&e("{X}\\Foo\\foo.exe", "Foo")));
        assert!(!is_junk(&e("{X}\\Revo Uninstaller\\RevoUnin.exe", "Revo Uninstaller")));
        assert!(is_junk(&e("{X}\\HandBrake\\uninst.exe", "Uninstall")));
        assert!(is_junk(&e("{X}\\Plex\\Uninstall.exe", "Plex Uninstall")));
        assert!(is_junk(&e("Foo.{GUID}", "Uninstall Foo 4.0")));
    }
}
