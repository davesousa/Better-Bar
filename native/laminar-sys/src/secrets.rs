//! Connector secrets (API keys, tokens, private calendar links) in Windows Credential
//! Manager, never in config.json. Each is a generic credential named
//! `LaminarDock:<connector>:<field>`, visible (and removable) in Control Panel →
//! Credential Manager → Windows Credentials. Stored per user, encrypted by Windows.

use crate::util::wide;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Security::Credentials::*;

fn target(connector: &str, field: &str) -> Vec<u16> {
    wide(&format!("LaminarDock:{connector}:{field}"))
}

/// Read a secret, if stored.
pub fn get(connector: &str, field: &str) -> Option<String> {
    let t = target(connector, field);
    unsafe {
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(PCWSTR(t.as_ptr()), CRED_TYPE_GENERIC, None, &mut p).ok()?;
        let c = &*p;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let s = String::from_utf8(bytes.to_vec()).ok();
        CredFree(p as *const _);
        s.filter(|s| !s.is_empty())
    }
}

/// Store (or replace) a secret. An empty value deletes it.
pub fn set(connector: &str, field: &str, value: &str) -> bool {
    if value.is_empty() {
        return delete(connector, field);
    }
    let mut t = target(connector, field);
    let mut user = wide("Laminar Dock");
    let blob = value.as_bytes();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(t.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0).is_ok() }
}

pub fn delete(connector: &str, field: &str) -> bool {
    let t = target(connector, field);
    unsafe { CredDeleteW(PCWSTR(t.as_ptr()), CRED_TYPE_GENERIC, None).is_ok() }
}

/// True if a secret is stored (without returning it).
pub fn has(connector: &str, field: &str) -> bool {
    get(connector, field).is_some()
}
