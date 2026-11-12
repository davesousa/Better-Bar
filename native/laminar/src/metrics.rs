//! Process measurements for the Phase 0 targets (memory, CPU).

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

fn ft(f: FILETIME) -> f64 {
    (((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64) as f64 / 1e7
}

/// Total CPU time (user + kernel) used by this process, in seconds.
pub fn cpu_seconds() -> f64 {
    let (mut c, mut e, mut k, mut u) = Default::default();
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
    }
    ft(k) + ft(u)
}

pub struct Memory {
    /// All pages in RAM, including shared DLL pages.
    pub working_set: f64,
    /// Committed private memory (what the process has reserved for itself).
    pub private_bytes: f64,
    /// Private pages actually in RAM. This is Task Manager's "Memory" column.
    pub private_working_set: f64,
}

pub fn memory() -> Memory {
    let mut pmc = PROCESS_MEMORY_COUNTERS_EX2 {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
            pmc.cb,
        );
    }
    const MB: f64 = 1048576.0;
    Memory {
        working_set: pmc.WorkingSetSize as f64 / MB,
        private_bytes: pmc.PrivateUsage as f64 / MB,
        private_working_set: pmc.PrivateWorkingSetSize as f64 / MB,
    }
}
