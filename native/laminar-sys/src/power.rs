//! Power actions for the Start/power menu. These change system state immediately —
//! only call them in response to an explicit user click.
//!
//! Lock: `LockWorkStation`. Sleep: `SetSuspendState(false, ..)` (sleep, or hibernate if the
//! machine has sleep disabled; on Modern Standby systems this enters standby).
//! Restart / shut down: `InitiateShutdownW` (graceful: apps get WM_QUERYENDSESSION, apps
//! registered for restart are relaunched, shutdown honours Fast Startup via
//! `SHUTDOWN_HYBRID`). Sign out: `ExitWindowsEx(EWX_LOGOFF)`. Restart/shutdown/sleep first
//! enable `SeShutdownPrivilege` on the process token (`AdjustTokenPrivileges`).
//! Each function returns true if Windows accepted the request.

use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, InitiateShutdownW, LockWorkStation, EWX_LOGOFF, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER, SHUTDOWN_FLAGS, SHUTDOWN_HYBRID, SHUTDOWN_POWEROFF, SHUTDOWN_RESTART, SHUTDOWN_RESTARTAPPS,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

fn enable_shutdown_privilege() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut luid = LUID::default();
        let ok = LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid).is_ok() && {
            let tp = TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }] };
            AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None).is_ok()
        };
        let _ = CloseHandle(token);
        ok
    }
}

fn initiate(flags: SHUTDOWN_FLAGS) -> bool {
    enable_shutdown_privilege();
    let r = unsafe { InitiateShutdownW(None, None, 0, flags, SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED) };
    r == 0
}

/// Locks the workstation (shows the lock screen).
pub fn lock() -> bool {
    unsafe { LockWorkStation() }.is_ok()
}

/// Puts the machine to sleep.
pub fn sleep() -> bool {
    enable_shutdown_privilege();
    unsafe { SetSuspendState(false, false, false) }
}

/// Restarts Windows, relaunching apps that registered for restart.
pub fn restart() -> bool {
    initiate(SHUTDOWN_RESTART | SHUTDOWN_RESTARTAPPS)
}

/// Shuts down (hybrid shutdown, i.e. honours Fast Startup like Start's "Shut down").
pub fn shut_down() -> bool {
    initiate(SHUTDOWN_POWEROFF | SHUTDOWN_HYBRID)
}

/// Signs the current user out.
pub fn sign_out() -> bool {
    unsafe { ExitWindowsEx(EWX_LOGOFF, SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED) }.is_ok()
}
