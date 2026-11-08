//! Dock contents: which apps are pinned, their names and their icons.
//!
//! Phase 0 uses a built-in list of common apps and keeps the ones installed on
//! this PC. Apps are addressed through `shell:AppsFolder\<AppUserModelID>`, which
//! works for both classic (.exe) and Store/packaged apps, the same way Start does.

use windows::core::{Interface, HSTRING, PCWSTR, Result};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{DeleteObject, HBITMAP};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, ShellExecuteW, SIGDN_NORMALDISPLAY,
    SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Launch,
    App,
    Separator,
    Trash,
}

pub struct Item {
    pub kind: Kind,
    pub name: String,
    /// What ShellExecute opens.
    pub target: String,
    /// 256 px icon from the shell (None for the launch button and separators).
    pub bitmap: Option<HBITMAP>,
}

impl Item {
    pub fn is_icon(&self) -> bool {
        self.kind != Kind::Separator
    }
}

impl Drop for Item {
    fn drop(&mut self) {
        if let Some(b) = self.bitmap.take() {
            unsafe {
                let _ = DeleteObject(b.into());
            }
        }
    }
}

/// Common apps, in two groups (a separator goes between them). Missing ones are skipped.
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

fn shell_item(parsing_name: &str) -> Result<IShellItem> {
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(parsing_name), None) }
}

fn display_name(item: &IShellItem) -> Result<String> {
    unsafe {
        let p = item.GetDisplayName(SIGDN_NORMALDISPLAY)?;
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        Ok(s)
    }
}

fn icon(item: &IShellItem, px: i32) -> Option<HBITMAP> {
    unsafe {
        let factory: IShellItemImageFactory = item.cast().ok()?;
        factory.GetImage(SIZE { cx: px, cy: px }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()
    }
}

fn load(parsing_name: &str, kind: Kind) -> Option<Item> {
    let item = shell_item(parsing_name).ok()?;
    let name = display_name(&item).ok()?;
    let bitmap = icon(&item, 256)?;
    Some(Item { kind, name, target: parsing_name.to_string(), bitmap: Some(bitmap) })
}

fn sep() -> Item {
    Item { kind: Kind::Separator, name: String::new(), target: String::new(), bitmap: None }
}

/// Build the dock: launch button, group A, separator, group B, separator, Recycle Bin.
pub fn pinned() -> Vec<Item> {
    let mut items = vec![Item {
        kind: Kind::Launch,
        name: "Launchpad".into(),
        target: String::new(),
        bitmap: None,
    }];
    let app = |id: &&str| load(&format!("shell:AppsFolder\\{id}"), Kind::App);
    items.extend(GROUP_A.iter().filter_map(app).take(6));
    items.push(sep());
    items.extend(GROUP_B.iter().filter_map(app).take(6));
    items.push(sep());
    if let Some(t) = load("shell:RecycleBinFolder", Kind::Trash) {
        items.push(t);
    }
    items
}

/// Open an app (or the Recycle Bin) on a background thread so the dock never blocks.
pub fn launch(target: &str) {
    let target = target.to_string();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        CoUninitialize();
    });
}
