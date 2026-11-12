//! Small internal helpers shared by the modules.

use windows::core::PWSTR;
use windows::Win32::System::Com::CoTaskMemFree;

/// NUL-terminated UTF-16 copy of `s` for passing as `PCWSTR`.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Converts a COM-allocated `PWSTR` to `String` and frees it with `CoTaskMemFree`.
pub(crate) fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

/// UTF-16 buffer (possibly NUL-terminated) to `String`.
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

/// Full image path of a process (`QueryFullProcessImageNameW`), if accessible.
pub(crate) fn process_image_path(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// `FileDescription` from an executable's version resource (e.g. "Google Chrome").
pub(crate) fn file_description(path: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let wpath = wide(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wpath.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wpath.as_ptr()), None, size, data.as_mut_ptr() as *mut _).ok()?;
        // First language/codepage pair from the translation table.
        let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let tr = wide("\\VarFileInfo\\Translation");
        if !VerQueryValueW(data.as_ptr() as *const _, PCWSTR(tr.as_ptr()), &mut p, &mut len).as_bool() || len < 4 {
            return None;
        }
        let lang = *(p as *const u16);
        let cp = *(p as *const u16).add(1);
        let key = wide(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
        let mut sp: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut slen = 0u32;
        if !VerQueryValueW(data.as_ptr() as *const _, PCWSTR(key.as_ptr()), &mut sp, &mut slen).as_bool() || slen == 0 {
            return None;
        }
        let s = std::slice::from_raw_parts(sp as *const u16, slen as usize);
        let s = from_wide(s).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// A Win32 auto-reset event shared between a WinRT completion callback (any thread) and
/// the waiting thread. Kept alive by `Arc` so a late callback after a timeout is harmless.
pub(crate) struct Signal(usize);

impl Signal {
    pub(crate) fn new() -> Option<std::sync::Arc<Signal>> {
        let h = unsafe { windows::Win32::System::Threading::CreateEventW(None, false, false, windows::core::PCWSTR::null()) }.ok()?;
        Some(std::sync::Arc::new(Signal(h.0 as usize)))
    }

    pub(crate) fn set(&self) {
        unsafe {
            let _ = windows::Win32::System::Threading::SetEvent(windows::Win32::Foundation::HANDLE(self.0 as *mut _));
        }
    }

    /// Waits up to `ms` while pumping COM calls and window messages
    /// (`CoWaitForMultipleHandles`). Pumping matters on STA threads: some WinRT
    /// operations (storage queries, stream opens) complete by posting back to the
    /// caller's apartment and would otherwise never finish. Returns true if signalled.
    pub(crate) fn wait(&self, ms: u64) -> bool {
        use windows::Win32::System::Com::{CoWaitForMultipleHandles, COWAIT_DISPATCH_CALLS, COWAIT_DISPATCH_WINDOW_MESSAGES};
        use windows::Win32::System::Threading::WaitForSingleObject;
        let h = windows::Win32::Foundation::HANDLE(self.0 as *mut _);
        let ms = ms.min(u32::MAX as u64 - 1) as u32;
        let flags = (COWAIT_DISPATCH_CALLS.0 | COWAIT_DISPATCH_WINDOW_MESSAGES.0) as u32;
        match unsafe { CoWaitForMultipleHandles(flags, ms, &[h]) } {
            Ok(0) => true,
            Ok(_) => false,
            Err(e) if e.code() == windows::Win32::Foundation::RPC_S_CALLPENDING => false,
            // COM not initialised on this thread (or flags rejected): plain wait.
            Err(_) => (unsafe { WaitForSingleObject(h, ms) }) == windows::Win32::Foundation::WAIT_OBJECT_0,
        }
    }
}

impl Drop for Signal {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(self.0 as *mut _));
        }
    }
}
