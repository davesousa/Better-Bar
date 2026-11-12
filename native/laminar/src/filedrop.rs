//! Dropping files onto the dock (OLE drag and drop).
//!
//! The dock registers an `IDropTarget`. While files are dragged over it the icons
//! magnify as usual; dropping onto an app opens the files with that app, and
//! dropping onto the Recycle Bin moves them there (recoverable).

use windows::core::{implement, Ref, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, POINTL};
use windows::Win32::System::Com::{
    CoCreateInstance, IDataObject, CLSCTX_LOCAL_SERVER, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, DragQueryFileW, IApplicationActivationManager, IShellItemArray,
    SHCreateShellItemArrayFromIDLists, SHFileOperationW, SHParseDisplayName, AO_NONE, FOF_ALLOWUNDO, FO_DELETE, HDROP,
    SHFILEOPSTRUCTW,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;

/// What the dock wants to do with a drag at a given screen point.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    None,
    /// Open the files with the app under the pointer.
    App,
    /// Move the files to the Recycle Bin.
    Trash,
    /// Pin the files (apps, shortcuts, folders, documents) where they're dropped.
    Pin,
}

#[implement(IDropTarget)]
pub struct DropTarget {
    pub callback: fn(Event) -> Target,
}

pub enum Event {
    /// A drag entered the dock, carrying these file paths. The bool: Alt is held
    /// (Windows' "create shortcut" drag, which here means "pin").
    Enter(i32, i32, Vec<String>, bool),
    Over(i32, i32, bool),
    Leave,
    Drop(i32, i32, Vec<String>),
}

/// The drop effect to report, chosen from what the drag source allows.
/// `MK_ALT` in an OLE drag's key state.
const MK_ALT: u32 = 0x20;

fn effect_for(t: Target, allowed: DROPEFFECT) -> DROPEFFECT {
    let prefs: &[DROPEFFECT] = match t {
        Target::None => return DROPEFFECT_NONE,
        Target::App => &[DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE],
        Target::Trash => &[DROPEFFECT_MOVE, DROPEFFECT_COPY],
        Target::Pin => &[DROPEFFECT_LINK, DROPEFFECT_COPY, DROPEFFECT_MOVE],
    };
    prefs.iter().copied().find(|e| allowed.0 & e.0 != 0).unwrap_or(DROPEFFECT_NONE)
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, data: Ref<IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let files = data.ok().map(|d| files_from(d)).unwrap_or_default();
        let t = (self.callback)(Event::Enter(pt.x, pt.y, files, keys.0 & MK_ALT != 0));
        unsafe { *effect = effect_for(t, *effect) };
        Ok(())
    }
    fn DragOver(&self, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let t = (self.callback)(Event::Over(pt.x, pt.y, keys.0 & MK_ALT != 0));
        unsafe { *effect = effect_for(t, *effect) };
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        (self.callback)(Event::Leave);
        Ok(())
    }
    fn Drop(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let files = data.ok().map(|d| files_from(d)).unwrap_or_default();
        let t = (self.callback)(Event::Drop(pt.x, pt.y, files));
        unsafe { *effect = effect_for(t, *effect) };
        Ok(())
    }
}

/// File paths from a drag's data object (CF_HDROP).
fn files_from(data: &IDataObject) -> Vec<String> {
    let fmt = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let mut out = Vec::new();
    unsafe {
        let Ok(mut medium) = data.GetData(&fmt) else { return out };
        let hdrop = HDROP(medium.u.hGlobal.0);
        let n = DragQueryFileW(hdrop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(String::from_utf16_lossy(&buf[..len]));
        }
        ReleaseStgMedium(&mut medium);
    }
    out
}

pub fn register(hwnd: HWND, callback: fn(Event) -> Target) -> Result<IDropTarget> {
    let target: IDropTarget = DropTarget { callback }.into();
    unsafe {
        windows::Win32::System::Ole::RegisterDragDrop(hwnd, &target)?;
    }
    Ok(target)
}

/// Programs, shortcuts and folders: dropping them on the dock pins them (they're never
/// "opened with" the app under the pointer).
pub fn is_pinnable(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [".exe", ".lnk", ".url", ".appref-ms"].iter().any(|e| p.ends_with(e)) || std::path::Path::new(path).is_dir()
}

/// Open files with an app: classic apps get the paths as arguments; packaged
/// (Store) apps are activated for the files through Windows' official API.
pub fn open_with(app_id: &str, exe: Option<String>, files: &[String]) {
    let app = app_id.to_string();
    let files = files.to_vec();
    std::thread::spawn(move || unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        if let Some(exe) = exe {
            let args = files.iter().map(|f| format!("\"{f}\"")).collect::<Vec<_>>().join(" ");
            windows::Win32::UI::Shell::ShellExecuteW(
                None,
                &HSTRING::from("open"),
                &HSTRING::from(exe),
                &HSTRING::from(args),
                PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        } else {
            match CoCreateInstance::<_, IApplicationActivationManager>(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) {
                Ok(mgr) => {
                    let mut pidls: Vec<*const ITEMIDLIST> = Vec::new();
                    for f in &files {
                        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
                        match SHParseDisplayName(&HSTRING::from(f.as_str()), None, &mut pidl, 0, None) {
                            Ok(()) => pidls.push(pidl),
                            Err(e) => eprintln!("open_with: cannot parse {f}: {e:?}"),
                        }
                    }
                    match SHCreateShellItemArrayFromIDLists(&pidls) {
                        Ok(arr) => {
                            let arr: IShellItemArray = arr;
                            if let Err(e) = mgr.ActivateForFile(&HSTRING::from(app.as_str()), &arr, &HSTRING::from("open")) {
                                // Packaged *desktop* apps (Paint, Notepad, Terminal…) don't implement
                                // the file contract; they take the paths on the command line instead.
                                let args = files.iter().map(|f| format!("\"{f}\"")).collect::<Vec<_>>().join(" ");
                                if let Err(e2) = mgr.ActivateApplication(&HSTRING::from(app.as_str()), &HSTRING::from(args), AO_NONE) {
                                    eprintln!("open_with: {app}: file activation {e:?}; command-line activation {e2:?}");
                                }
                            }
                        }
                        Err(e) => eprintln!("open_with: item array failed: {e:?}"),
                    }
                    for p in pidls {
                        windows::Win32::System::Com::CoTaskMemFree(Some(p as _));
                    }
                }
                Err(e) => eprintln!("open_with: activation manager unavailable: {e:?}"),
            }
        }
    });
}

/// Move files to the Recycle Bin (undoable, with the normal Windows prompts).
pub fn recycle(files: &[String]) {
    let mut list: Vec<u16> = Vec::new();
    for f in files {
        list.extend(f.encode_utf16());
        list.push(0);
    }
    list.push(0);
    std::thread::spawn(move || unsafe {
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: PCWSTR(list.as_ptr()),
            fFlags: FOF_ALLOWUNDO.0 as u16,
            ..Default::default()
        };
        let _ = SHFileOperationW(&mut op);
    });
}
